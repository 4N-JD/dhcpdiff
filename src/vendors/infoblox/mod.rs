use crate::formats::isc::{collect_definitions, IscDocument, IscNode};
use crate::model::{Config, Reservation, RuleScope};
use crate::registry::{
    DetectionScore, FormatFamily, Input, VendorDocument, VendorPlugin,
};
use crate::vendors::isc_dhcp::{
    apply_global_statement, assign_reservations_to_subnets, is_if_block_header, parse_if_block,
    parse_isc_class, parse_isc_shared_network, parse_isc_subnet, parse_match_if, push_pending_host,
};

pub struct InfobloxPlugin;

impl VendorPlugin for InfobloxPlugin {
    fn id(&self) -> &'static str {
        "infoblox"
    }

    fn display_name(&self) -> &'static str {
        "Infoblox DHCP"
    }

    fn format_family(&self) -> FormatFamily {
        FormatFamily::IscConf
    }

    fn detect(&self, input: &Input) -> DetectionScore {
        if input.content.contains("infoblox-") {
            DetectionScore::CERTAIN
        } else if input.content.contains("class \"") && input.content.contains("match if") {
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
            anyhow::bail!("Infoblox plugin expects ISC document");
        };
        normalize_infoblox(&doc, &input.file_name())
    }
}

fn normalize_infoblox(doc: &IscDocument, file: &str) -> anyhow::Result<Config> {
    let mut config = Config::default();
    config.option_definitions = collect_definitions(&doc.nodes);

    let mut pending_hosts: Vec<Reservation> = Vec::new();

    for node in &doc.nodes {
        match node {
            IscNode::Statement(stmt) => {
                apply_global_statement(
                    &mut config.global_options,
                    &config.option_definitions,
                    &stmt.text,
                    Some(crate::formats::isc::statement_source(
                        "infoblox",
                        file,
                        stmt,
                    )),
                );
            }
            IscNode::Block(block) if block.header.starts_with("class ") => {
                config.global_filters.push(parse_isc_class(
                    block,
                    file,
                    "infoblox",
                    &config.option_definitions,
                    parse_match_if,
                )?);
            }
            IscNode::Block(block) if block.header.starts_with("shared-network ") => {
                let (shared, subnets, hosts, rules) = parse_isc_shared_network(
                    block,
                    file,
                    "infoblox",
                    &config.option_definitions,
                    true,
                    parse_match_if,
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
                    "infoblox",
                    &config.option_definitions,
                    true,
                    parse_match_if,
                    None,
                )?;
                config.subnets.push(subnet);
                config.conditional_rules.extend(rules);
            }
            IscNode::Block(block) if block.header.starts_with("host ") => {
                push_pending_host(
                    block,
                    file,
                    "infoblox",
                    &config.option_definitions,
                    &mut pending_hosts,
                );
            }
            IscNode::Block(block) if is_if_block_header(&block.header) => {
                config.conditional_rules.push(parse_if_block(
                    block,
                    file,
                    "infoblox",
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
