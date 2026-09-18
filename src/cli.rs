use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};

use crate::pipeline::{default_mapping_path, print_diff_result, run_diff, run_normalize};
use crate::options::ResolverOptions;
use crate::registry::VendorRegistry;

#[derive(Parser)]
#[command(name = "dhcpdiff", about = "Multi-vendor DHCP configuration diff tool")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Compare two DHCP configurations
    Diff {
        #[arg(long)]
        source: PathBuf,
        #[arg(long)]
        target: PathBuf,
        #[arg(long, default_value = "auto")]
        source_vendor: String,
        #[arg(long, default_value = "auto")]
        target_vendor: String,
        #[arg(long)]
        mapping: Option<PathBuf>,
        #[arg(long, value_enum, default_value_t = ReportFormat::Text)]
        format: ReportFormat,
        #[arg(long)]
        ignore_unmapped: bool,
        /// Ignore dhcp option 1 (subnet-mask) when comparing configs
        #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
        ignore_subnet_mask: bool,
    },
    /// Normalize a config to unified JSON IR
    Normalize {
        #[arg(long)]
        input: PathBuf,
        #[arg(long, default_value = "auto")]
        vendor: String,
        #[arg(long, short)]
        output: PathBuf,
        #[arg(long)]
        mapping: Option<PathBuf>,
        /// Ignore dhcp option 1 (subnet-mask) in normalized output
        #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
        ignore_subnet_mask: bool,
    },
    /// Interactively map unknown options (TUI)
    Map {
        #[arg(long)]
        source: PathBuf,
        #[arg(long)]
        target: PathBuf,
        #[arg(long, default_value = "auto")]
        source_vendor: String,
        #[arg(long, default_value = "auto")]
        target_vendor: String,
        #[arg(long)]
        mapping: Option<PathBuf>,
    },
    /// List registered vendor plugins
    Vendors,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum ReportFormat {
    Text,
    Json,
}

impl ReportFormat {
    pub fn into_output_format(self) -> crate::report::OutputFormat {
        match self {
            Self::Text => crate::report::OutputFormat::Text,
            Self::Json => crate::report::OutputFormat::Json,
        }
    }
}

pub fn run() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let registry = VendorRegistry::new();

    match cli.command {
        Commands::Vendors => {
            for plugin in registry.plugins() {
                println!(
                    "{} ({}) — format family: {}",
                    plugin.id(),
                    plugin.display_name(),
                    plugin.format_family().as_str()
                );
            }
        }
        Commands::Normalize {
            input,
            vendor,
            output,
            mapping,
            ignore_subnet_mask,
        } => {
            let mapping_path = mapping.as_deref();
            run_normalize(
                &registry,
                &input,
                &vendor,
                mapping_path,
                &output,
                ResolverOptions {
                    ignore_subnet_mask: Some(ignore_subnet_mask),
                },
            )
                .map_err(|e| anyhow::anyhow!("normalize failed: {e}"))?;
            println!("Wrote {}", output.display());
        }
        Commands::Diff {
            source,
            target,
            source_vendor,
            target_vendor,
            mapping,
            format,
            ignore_unmapped,
            ignore_subnet_mask,
        } => {
            let default_mapping = default_mapping_path();
            let mapping_path = mapping
                .as_deref()
                .or(Some(default_mapping.as_path()));
            let result = run_diff(
                &registry,
                &source,
                &target,
                &source_vendor,
                &target_vendor,
                mapping_path,
                ignore_unmapped,
                ResolverOptions {
                    ignore_subnet_mask: Some(ignore_subnet_mask),
                },
            )?;
            let code = print_diff_result(&result, format.into_output_format())?;
            if code != 0 {
                std::process::exit(code);
            }
        }
        Commands::Map {
            source,
            target,
            source_vendor,
            target_vendor,
            mapping,
        } => {
            let mapping_path = mapping.unwrap_or_else(default_mapping_path);
            crate::tui::run_map_tui(
                &registry,
                &source,
                &target,
                &source_vendor,
                &target_vendor,
                &mapping_path,
            )?;
        }
    }
    Ok(())
}
