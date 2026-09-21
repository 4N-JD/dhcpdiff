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
        } => {
            let mapping_path = mapping.as_deref();
            run_normalize(
                &registry,
                &input,
                &vendor,
                mapping_path,
                &output,
                ResolverOptions::default(),
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
                ResolverOptions::default(),
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
