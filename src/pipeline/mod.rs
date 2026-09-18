use std::path::{Path, PathBuf};

use anyhow::Context;

use crate::diff::{diff_configs_with_progress, DiffReport, FileMeta};
use crate::options::{OptionResolver, ResolverOptions};
use crate::registry::{Input, VendorRegistry};
use crate::report::{print_report, print_unknowns, OutputFormat};

pub struct PipelineResult {
    pub source: crate::model::Config,
    pub target: crate::model::Config,
    pub source_vendor: String,
    pub target_vendor: String,
    pub source_path: PathBuf,
    pub target_path: PathBuf,
    pub report: DiffReport,
    pub unknowns: Vec<crate::options::UnknownOption>,
}

pub fn load_config(
    registry: &VendorRegistry,
    path: &Path,
    vendor: &str,
    resolver: &OptionResolver,
) -> anyhow::Result<crate::model::Config> {
    Ok(load_config_with_progress(registry, path, vendor, resolver, false)?.0)
}

pub fn load_config_with_progress(
    registry: &VendorRegistry,
    path: &Path,
    vendor: &str,
    resolver: &OptionResolver,
    progress: bool,
) -> anyhow::Result<(crate::model::Config, String)> {
    if progress {
        eprintln!("Parsing {} (vendor={vendor})...", path.display());
    }
    let input = Input::from_path(path.to_path_buf())?;
    let plugin = registry.resolve(vendor, &input)?;
    let vendor_id = plugin.id().to_string();
    let mut config = plugin.normalize(plugin.parse(&input)?, &input)?;
    resolver.resolve_config(&mut config);
    if progress {
        eprintln!(
            "  → {} subnets, {} pools, {} reservations, {} filters, {} conditional rules",
            config.subnet_count(),
            config.pool_count(),
            config.reservation_count(),
            config.filter_count(),
            config.conditional_rules.len(),
        );
    }
    Ok((config, vendor_id))
}

pub fn run_diff(
    registry: &VendorRegistry,
    source_path: &Path,
    target_path: &Path,
    source_vendor: &str,
    target_vendor: &str,
    mapping_path: Option<&Path>,
    ignore_unmapped: bool,
    resolver_options: ResolverOptions,
) -> anyhow::Result<PipelineResult> {
    let resolver = OptionResolver::load(mapping_path, resolver_options)?;

    let (source, source_vendor_id) =
        load_config_with_progress(registry, source_path, source_vendor, &resolver, true)?;
    let (target, target_vendor_id) =
        load_config_with_progress(registry, target_path, target_vendor, &resolver, true)?;

    let unknowns = crate::options::merge_unknowns([
        resolver.collect_unknowns(&source),
        resolver.collect_unknowns(&target),
    ]);

    if !ignore_unmapped && !unknowns.is_empty() {
        print_unknowns(&unknowns);
        anyhow::bail!("unmapped options remain; run 'dhcpdiff map' or pass --ignore-unmapped");
    }

    let report = diff_configs_with_progress(&source, &target, true).with_file_meta(
        FileMeta {
            path: source_path.display().to_string(),
            vendor: source_vendor_id.clone(),
        },
        FileMeta {
            path: target_path.display().to_string(),
            vendor: target_vendor_id.clone(),
        },
    );

    Ok(PipelineResult {
        source,
        target,
        source_vendor: source_vendor_id,
        target_vendor: target_vendor_id,
        source_path: source_path.to_path_buf(),
        target_path: target_path.to_path_buf(),
        report,
        unknowns,
    })
}

pub fn run_normalize(
    registry: &VendorRegistry,
    input_path: &Path,
    vendor: &str,
    mapping_path: Option<&Path>,
    output: &Path,
    resolver_options: ResolverOptions,
) -> anyhow::Result<()> {
    let resolver = OptionResolver::load(mapping_path, resolver_options)?;
    let config = load_config(registry, input_path, vendor, &resolver)?;
    let json = serde_json::to_string_pretty(&config)?;
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(output, json)?;
    Ok(())
}

pub fn export_config_json(config: &crate::model::Config, output: &Path) -> anyhow::Result<()> {
    let json = serde_json::to_string_pretty(config)?;
    std::fs::write(output, json).context("write output")
}

pub fn default_mapping_path() -> PathBuf {
    PathBuf::from("mappings/user.yaml")
}

pub fn print_diff_result(result: &PipelineResult, format: OutputFormat) -> anyhow::Result<i32> {
    print_report(&result.report, format)?;
    if result.report.has_differences {
        Ok(1)
    } else {
        Ok(0)
    }
}
