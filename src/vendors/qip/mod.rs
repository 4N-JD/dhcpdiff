use std::collections::BTreeMap;
use std::net::Ipv4Addr;

use anyhow::Context;
use ipnet::Ipv4Net;

use crate::formats::isc::{
    collect_definitions, collect_options_from_nodes_labeled, option_key_from_name,
    parse_lease_time_statement, parse_option_statement, IscDocument, IscNode,
};
use crate::model::{
    normalize_mac, BoundOption, Config, Filter, FilterMatch, NormalizedValue, Pool, Reservation,
    SourceRef, Subnet,
};
use crate::registry::{
    DetectionScore, FormatFamily, Input, VendorDocument, VendorPlugin,
};

pub struct QipPlugin;

impl VendorPlugin for QipPlugin {
    fn id(&self) -> &'static str {
        "qip"
    }

    fn display_name(&self) -> &'static str {
        "QIP DHCP"
    }

    fn format_family(&self) -> FormatFamily {
        FormatFamily::IscConf
    }

    fn detect(&self, input: &Input) -> DetectionScore {
        if input.content.contains("dynamic-dhcp")
            || input.content.contains("manual-dhcp")
            || input.content.contains("vendor-class ")
        {
            DetectionScore::HIGH
        } else {
            DetectionScore::NONE
        }
    }

    fn parse(&self, input: &Input) -> anyhow::Result<VendorDocument> {
        let doc = crate::formats::isc::parse_isc(&input.content)?;
        Ok(VendorDocument::Isc(doc))
    }

    fn normalize(&self, doc: VendorDocument, input: &Input) -> anyhow::Result<Config> {
        let VendorDocument::Isc(doc) = doc else {
            anyhow::bail!("QIP plugin expects ISC document");
        };
        normalize_qip(&doc, &input.file_name())
    }
}

fn normalize_qip(doc: &IscDocument, file: &str) -> anyhow::Result<Config> {
    let mut config = Config::default();
    config.option_definitions = collect_definitions(&doc.nodes);

    let mut global_filters = Vec::new();
    let mut subnets = Vec::new();

    for node in &doc.nodes {
        match node {
            IscNode::Block(block) if block.header.starts_with("subnet ") => {
                subnets.push(parse_qip_subnet(block, file, &config.option_definitions)?);
            }
            IscNode::Block(block) if block.header.starts_with("vendor-class ") => {
                global_filters.push(parse_qip_vendor_class(block, file, &config.option_definitions)?);
            }
            IscNode::Statement(stmt) => {
                apply_global_statement(&mut config, &stmt.text, file, stmt.location.line);
            }
            _ => {}
        }
    }

    config.global_filters = global_filters;
    config.subnets = subnets;
    Ok(config)
}

fn apply_global_statement(config: &mut Config, text: &str, file: &str, line: u32) {
    let source = Some(SourceRef::new("qip", file, line));
    if let Some((name, value)) = parse_option_statement(text) {
        let key = option_key_from_name(&name, &config.option_definitions);
        let mut val = NormalizedValue::from_raw_text(&value);
        val.sort_ip_list_if_needed();
        config.global_options.insert(
            key,
            BoundOption::new(val, source.clone()).with_declared_in("Global"),
        );
    } else if let Some((key, val)) = parse_lease_time_statement(text) {
        config.global_options.insert(
            key,
            BoundOption::new(val, source).with_declared_in("Global"),
        );
    }
}

fn parse_qip_subnet(
    block: &crate::formats::isc::IscBlock,
    file: &str,
    definitions: &BTreeMap<String, crate::model::OptionDef>,
) -> anyhow::Result<Subnet> {
    let parts: Vec<_> = block.header.split_whitespace().collect();
    let network = parts
        .get(1)
        .context("subnet missing network")?
        .parse::<Ipv4Addr>()?;
    let netmask = parts
        .get(3)
        .context("subnet missing netmask")?
        .parse::<Ipv4Addr>()?;
    let prefix = netmask_to_prefix(netmask)?;
    let network = Ipv4Net::with_netmask(network, netmask)
        .or_else(|_| Ipv4Net::new(network, prefix))?;

    let mut subnet = Subnet {
        network,
        shared_network: None,
        options: BTreeMap::new(),
        pools: Vec::new(),
        reservations: Vec::new(),
        filters: Vec::new(),
        extensions: BTreeMap::new(),
        source: Some(SourceRef::with_span("qip", file, block.location.line, block.location.end_line)),
    };

    for child in &block.children {
        match child {
            IscNode::Block(b) if b.header.starts_with("dynamic-dhcp range ") => {
                subnet.pools.push(parse_qip_pool(b, file, definitions)?);
            }
            IscNode::Block(b) if b.header.starts_with("manual-dhcp ") => {
                subnet
                    .reservations
                    .push(parse_qip_reservation(b, file, definitions)?);
            }
            IscNode::Block(b) if b.header.starts_with("vendor-class ") => {
                subnet
                    .filters
                    .push(parse_qip_vendor_class(b, file, definitions)?);
            }
            IscNode::Statement(s) => {
                if let Some((name, value)) = parse_option_statement(&s.text) {
                    let key = option_key_from_name(&name, definitions);
                    let mut val = NormalizedValue::from_raw_text(&value);
                    val.sort_ip_list_if_needed();
                    subnet.options.insert(
                        key,
                        BoundOption::new(
                            val,
                            Some(SourceRef::with_span(
                                "qip",
                                file,
                                s.location.line,
                                s.location.end_line,
                            )),
                        )
                        .with_declared_in("Subnet"),
                    );
                }
            }
            _ => {}
        }
    }
    Ok(subnet)
}

fn parse_qip_pool(
    block: &crate::formats::isc::IscBlock,
    file: &str,
    definitions: &BTreeMap<String, crate::model::OptionDef>,
) -> anyhow::Result<Pool> {
    let parts: Vec<_> = block.header.split_whitespace().collect();
    let start = parts
        .get(2)
        .context("pool missing start")?
        .parse::<Ipv4Addr>()?;
    let end = parts
        .get(3)
        .context("pool missing end")?
        .parse::<Ipv4Addr>()?;
    Ok(Pool {
        start,
        end,
        options: collect_options_from_nodes_labeled(
            &block.children,
            definitions,
            "qip",
            file,
            Some("Pool"),
        ),
        extensions: BTreeMap::new(),
        source: Some(SourceRef::with_span("qip", file, block.location.line, block.location.end_line)),
    })
}

fn parse_qip_reservation(
    block: &crate::formats::isc::IscBlock,
    file: &str,
    definitions: &BTreeMap<String, crate::model::OptionDef>,
) -> anyhow::Result<Reservation> {
    let parts: Vec<_> = block.header.split_whitespace().collect();
    let mac = normalize_mac(parts.get(1).context("reservation missing mac")?);
    let ip = parts
        .get(2)
        .context("reservation missing ip")?
        .parse::<Ipv4Addr>()?;
    Ok(Reservation {
        mac,
        ip,
        options: collect_options_from_nodes_labeled(
            &block.children,
            definitions,
            "qip",
            file,
            Some("Reservation"),
        ),
        extensions: BTreeMap::new(),
        source: Some(SourceRef::with_span("qip", file, block.location.line, block.location.end_line)),
    })
}

fn parse_qip_vendor_class(
    block: &crate::formats::isc::IscBlock,
    file: &str,
    definitions: &BTreeMap<String, crate::model::OptionDef>,
) -> anyhow::Result<Filter> {
    let name = extract_quoted(&block.header).unwrap_or_else(|| block.header.clone());
    let declared_in = format!("Class {name}");
    Ok(Filter {
        name: name.clone(),
        match_expr: FilterMatch::VendorClassExact { value: name },
        vendor_option_space: None,
        options: collect_options_from_nodes_labeled(
            &block.children,
            definitions,
            "qip",
            file,
            Some(&declared_in),
        ),
        extensions: BTreeMap::new(),
        source: Some(SourceRef::with_span("qip", file, block.location.line, block.location.end_line)),
    })
}

fn extract_quoted(s: &str) -> Option<String> {
    crate::model::extract_quoted_isc(s).map(|(decoded, _)| decoded)
}

fn netmask_to_prefix(mask: Ipv4Addr) -> anyhow::Result<u8> {
    let bits = u32::from(mask);
    if bits == 0 {
        return Ok(0);
    }
    let prefix = bits.count_ones() as u8;
    if (u32::MAX << (32 - prefix)) & bits == bits || prefix > 0 {
        Ok(prefix)
    } else {
        anyhow::bail!("invalid netmask {mask}")
    }
}
