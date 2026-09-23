use crate::formats::isc::{collect_definitions, IscDocument, IscNode};
use crate::model::{Config, Reservation, RuleScope};
use crate::registry::{
    DetectionScore, FormatFamily, Input, VendorDocument, VendorPlugin,
};
use crate::vendors::isc_dhcp::{
    apply_global_statement, assign_reservations_to_subnets, attach_subclass, is_if_block_header,
    parse_bluecat_match, parse_if_block, parse_isc_class, parse_isc_shared_network,
    parse_isc_subnet, parse_subclass_block, parse_subclass_statement, push_pending_host,
};

pub struct BluecatPlugin;

impl VendorPlugin for BluecatPlugin {
    fn id(&self) -> &'static str {
        "bluecat"
    }

    fn display_name(&self) -> &'static str {
        "BlueCat DHCP"
    }

    fn format_family(&self) -> FormatFamily {
        FormatFamily::IscConf
    }

    fn detect(&self, input: &Input) -> DetectionScore {
        if input.content.contains("dont-use-fsync")
            || (input.content.contains("ping-check") && input.content.contains("server-id-check"))
        {
            DetectionScore::CERTAIN
        } else if input.content.contains("class \"")
            && (input.content.contains("match substring") || input.content.contains("subclass "))
        {
            DetectionScore::MEDIUM
        } else {
            DetectionScore::LOW
        }
    }

    fn parse(&self, input: &Input) -> anyhow::Result<VendorDocument> {
        let doc = crate::formats::isc::parse_isc(&input.content)?;
        Ok(VendorDocument::Isc(doc))
    }

    fn normalize(&self, doc: VendorDocument, input: &Input) -> anyhow::Result<Config> {
        let VendorDocument::Isc(doc) = doc else {
            anyhow::bail!("Bluecat plugin expects ISC document");
        };
        normalize_bluecat(&doc, &input.file_name())
    }
}

fn normalize_bluecat(doc: &IscDocument, file: &str) -> anyhow::Result<Config> {
    let mut config = Config::default();
    config.option_definitions = collect_definitions(&doc.nodes);

    let mut pending_hosts: Vec<Reservation> = Vec::new();

    for node in &doc.nodes {
        match node {
            IscNode::Statement(stmt) => {
                let text = stmt.text.trim();
                if text.starts_with("subclass ") {
                    if let Some((class_name, value)) = parse_subclass_statement(text) {
                        attach_subclass(&mut config.global_filters, &class_name, &value);
                    }
                } else {
                    apply_global_statement(
                        &mut config.global_options,
                        &config.option_definitions,
                        text,
                        Some(crate::formats::isc::statement_source(
                            "bluecat",
                            file,
                            stmt,
                        )),
                    );
                }
            }
            IscNode::Block(block) if block.header.starts_with("class ") => {
                config.global_filters.push(parse_isc_class(
                    block,
                    file,
                    "bluecat",
                    &config.option_definitions,
                    parse_bluecat_match,
                )?);
            }
            IscNode::Block(block) if block.header.starts_with("subclass ") => {
                if let Some((class_name, value)) = parse_subclass_statement(&block.header) {
                    attach_subclass(&mut config.global_filters, &class_name, &value);
                }
                if let Some(rule) =
                    parse_subclass_block(block, file, "bluecat", &config.option_definitions)
                {
                    config.conditional_rules.push(rule);
                }
            }
            IscNode::Block(block) if block.header.starts_with("shared-network ") => {
                let (shared, subnets, hosts, rules) = parse_isc_shared_network(
                    block,
                    file,
                    "bluecat",
                    &config.option_definitions,
                    false,
                    parse_bluecat_match,
                )?;
                config.shared_networks.insert(shared.name.clone(), shared);
                config.subnets.extend(subnets);
                pending_hosts.extend(hosts);
                config.conditional_rules.extend(rules);
            }
            IscNode::Block(block) if block.header.starts_with("subnet ") => {
                let (subnet, rules) = parse_isc_subnet(
                    block,
                    file,
                    "bluecat",
                    &config.option_definitions,
                    false,
                    parse_bluecat_match,
                    None,
                )?;
                config.subnets.push(subnet);
                config.conditional_rules.extend(rules);
            }
            IscNode::Block(block) if block.header.starts_with("host ") => {
                push_pending_host(
                    block,
                    file,
                    "bluecat",
                    &config.option_definitions,
                    &mut pending_hosts,
                );
            }
            IscNode::Block(block) if is_if_block_header(&block.header) => {
                config.conditional_rules.extend(parse_if_block(
                    block,
                    file,
                    "bluecat",
                    &config.option_definitions,
                    RuleScope::Global,
                ));
            }
            _ => {}
        }
    }

    assign_reservations_to_subnets(&mut config.subnets, pending_hosts);
    Ok(config)
}
