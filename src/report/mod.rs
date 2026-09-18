use serde_json;

use crate::diff::{DiffEntry, DiffReport, EntityRef};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    Text,
    Json,
}

pub fn print_report(report: &DiffReport, format: OutputFormat) -> anyhow::Result<()> {
    match format {
        OutputFormat::Text => print_text(report),
        OutputFormat::Json => {
            println!("{}", serde_json::to_string(report)?);
        }
    }
    Ok(())
}

fn print_text(report: &DiffReport) {
    if !report.has_differences {
        println!("No differences found.");
        return;
    }

    println!(
        "DHCP configuration differences ({} items):",
        report.entries.len()
    );
    println!();

    for entry in &report.entries {
        match entry {
            DiffEntry::MissingInTarget { entity, detail, .. } => {
                println!("[MISSING] {} — {detail}", format_entity(entity));
            }
            DiffEntry::ExtraInTarget { entity, detail, .. } => {
                println!("[EXTRA]   {} — {detail}", format_entity(entity));
            }
            DiffEntry::Changed {
                entity, detail, ..
            } => {
                println!("[CHANGED] {} — {detail}", format_entity(entity));
            }
            DiffEntry::Unmapped {
                entity,
                option_key,
                detail,
                ..
            } => {
                println!(
                    "[UNMAPPED] {} — {}:{} — {detail}",
                    format_entity(entity),
                    option_key.space,
                    option_key.code
                );
            }
        }
    }
}

fn format_entity(entity: &EntityRef) -> String {
    if let Some(d) = &entity.display {
        let mut parts = vec![d.object_type.clone(), d.name.clone()];
        if let Some(parent) = &d.parent {
            parts.push(format!("({parent})"));
        }
        if let Some(vci) = &d.vci {
            parts.push(format!("[clients with VCI {vci}]"));
        }
        parts.join(" ")
    } else {
        format!("{}:{}", entity.kind, entity.key)
    }
}

pub fn print_unknowns(unknowns: &[crate::options::UnknownOption]) {
    if unknowns.is_empty() {
        return;
    }
    println!("Unknown options requiring mapping ({}):", unknowns.len());
    for u in unknowns {
        println!(
            "  - {} (usages: {}, sites: {})",
            u.raw_name,
            u.usage_count,
            u.sites.join(", ")
        );
        if let Some(v) = &u.example_value {
            println!("      example: {v:?}");
        }
    }
}
