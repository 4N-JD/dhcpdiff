use std::collections::BTreeMap;
use std::net::Ipv4Addr;
use std::sync::LazyLock;

use anyhow::Context;
use ipnet::Ipv4Net;
use regex::Regex;

use crate::formats::isc::{
    collect_options_from_nodes_labeled, insert_statement_options_labeled, statement_source,
    IscBlock, IscNode,
};
use crate::model::{
    ConditionalRule, Filter, FilterMatch, OptionDef, OptionMap, Pool, Reservation, RuleScope,
    SharedNetwork, SourceRef, Subnet,
};

static VCI_SUBSTRING: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"(?i)substring\s*\(\s*option\s+vendor-class-identifier\s*,\s*(\d+)\s*,\s*(\d+)\s*\)\s*=\s*"([^"]+)""#,
    )
    .unwrap()
});
static CLIENT_ID_SUBSTRING: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"(?i)substring\s*\(\s*option\s+dhcp-client-identifier\s*,\s*(\d+)\s*,\s*(\d+)\s*\)\s*=\s*"([^"]+)""#,
    )
    .unwrap()
});

pub fn apply_global_statement(
    global_options: &mut OptionMap,
    definitions: &BTreeMap<String, OptionDef>,
    text: &str,
    source: Option<SourceRef>,
) {
    let text = text.trim();
    insert_statement_options_labeled(
        global_options,
        definitions,
        text,
        source,
        Some("Global"),
    );
}

fn if_declared_in_label(scope: &RuleScope, match_expr: &FilterMatch) -> String {
    let scope_label = match scope {
        RuleScope::Global => "Global".to_string(),
        RuleScope::Subnet { .. } => "Subnet".to_string(),
        RuleScope::SharedNetwork { name } => format!("Shared network {name}"),
    };
    match match_expr.vendor_class_hint() {
        Some(vci) => format!("{scope_label} if {vci}"),
        None => format!("{scope_label} if"),
    }
}

pub fn parse_isc_shared_network(
    block: &IscBlock,
    file: &str,
    vendor_id: &str,
    definitions: &BTreeMap<String, OptionDef>,
    include_infoblox_range: bool,
    parse_match: fn(&str) -> FilterMatch,
) -> anyhow::Result<(SharedNetwork, Vec<Subnet>, Vec<Reservation>, Vec<ConditionalRule>)> {
    let name = extract_quoted(&block.header).unwrap_or_else(|| block.header.clone());
    let mut shared = SharedNetwork {
        name: name.clone(),
        options: BTreeMap::new(),
        source: Some(SourceRef::with_span(vendor_id, file, block.location.line, block.location.end_line)),
    };
    let mut subnets = Vec::new();
    let mut pending_hosts = Vec::new();
    let mut rules = Vec::new();

    for child in &block.children {
        match child {
            IscNode::Block(b) if b.header.starts_with("subnet ") => {
                let (subnet, subnet_rules) = parse_isc_subnet(
                    b,
                    file,
                    vendor_id,
                    definitions,
                    include_infoblox_range,
                    parse_match,
                    Some(name.clone()),
                )?;
                subnets.push(subnet);
                rules.extend(subnet_rules);
            }
            IscNode::Block(b) if b.header.starts_with("host ") => {
                push_pending_host(b, file, vendor_id, definitions, &mut pending_hosts);
            }
            IscNode::Block(b) if is_if_block_header(&b.header) => {
                rules.push(parse_if_block(
                    b,
                    file,
                    vendor_id,
                    definitions,
                    RuleScope::SharedNetwork {
                        name: name.clone(),
                    },
                ));
            }
            IscNode::Statement(s) => {
                insert_statement_options_labeled(
                    &mut shared.options,
                    definitions,
                    &s.text,
                    Some(statement_source(vendor_id, file, s)),
                    Some(&format!("Shared network {name}")),
                );
            }
            _ => {}
        }
    }

    Ok((shared, subnets, pending_hosts, rules))
}

pub fn parse_isc_subnet(
    block: &IscBlock,
    file: &str,
    vendor_id: &str,
    definitions: &BTreeMap<String, OptionDef>,
    include_infoblox_range: bool,
    parse_match: fn(&str) -> FilterMatch,
    shared_network: Option<String>,
) -> anyhow::Result<(Subnet, Vec<ConditionalRule>)> {
    let parts: Vec<_> = block.header.split_whitespace().collect();
    let network = parts
        .get(1)
        .context("subnet missing network")?
        .parse::<Ipv4Addr>()?;
    let netmask = parts
        .get(3)
        .context("subnet missing netmask")?
        .parse::<Ipv4Addr>()?;
    let network = Ipv4Net::with_netmask(network, netmask)?;
    let cidr = network.to_string();

    let mut subnet = Subnet {
        network,
        shared_network,
        options: BTreeMap::new(),
        pools: Vec::new(),
        reservations: Vec::new(),
        filters: Vec::new(),
        extensions: BTreeMap::new(),
        source: Some(SourceRef::with_span(vendor_id, file, block.location.line, block.location.end_line)),
    };
    let mut rules = Vec::new();

    for child in &block.children {
        match child {
            IscNode::Block(b) if b.header == "pool" || b.header.starts_with("pool ") => {
                let (ranges, options) =
                    parse_isc_pool(b, file, vendor_id, definitions, include_infoblox_range);
                for (start, end) in ranges {
                    subnet.pools.push(Pool {
                        start,
                        end,
                        options: options.clone(),
                        extensions: BTreeMap::new(),
                        source: Some(SourceRef::with_span(vendor_id, file, b.location.line, b.location.end_line)),
                    });
                }
            }
            IscNode::Block(b) if b.header.starts_with("host ") => {
                if let Some(res) = parse_isc_host(b, file, vendor_id, definitions) {
                    subnet.reservations.push(res);
                }
            }
            IscNode::Block(b) if b.header.starts_with("class ") => {
                subnet.filters.push(parse_isc_class(
                    b,
                    file,
                    vendor_id,
                    definitions,
                    parse_match,
                )?);
            }
            IscNode::Block(b) if is_if_block_header(&b.header) => {
                rules.push(parse_if_block(
                    b,
                    file,
                    vendor_id,
                    definitions,
                    RuleScope::Subnet {
                        cidr: cidr.clone(),
                    },
                ));
            }
            IscNode::Statement(s) => {
                insert_statement_options_labeled(
                    &mut subnet.options,
                    definitions,
                    &s.text,
                    Some(statement_source(vendor_id, file, s)),
                    Some("Subnet"),
                );
            }
            _ => {}
        }
    }

    subnet.pools.sort_by(|a, b| (a.start, a.end).cmp(&(b.start, b.end)));
    subnet.pools.dedup_by(|a, b| a.start == b.start && a.end == b.end && a.options == b.options);

    Ok((subnet, rules))
}

pub fn is_if_block_header(header: &str) -> bool {
    let h = header.trim();
    h.starts_with("if ") || h.starts_with("if(") || h.starts_with("elsif ") || h.starts_with("elsif(")
}

pub fn parse_if_block(
    block: &IscBlock,
    file: &str,
    vendor_id: &str,
    definitions: &BTreeMap<String, OptionDef>,
    scope: RuleScope,
) -> ConditionalRule {
    let mut vendor_option_space = None;
    for child in &block.children {
        if let IscNode::Statement(s) = child {
            let text = s.text.trim();
            if text.starts_with("vendor-option-space ") {
                vendor_option_space = Some(
                    text.strip_prefix("vendor-option-space ")
                        .unwrap_or("")
                        .trim_end_matches(';')
                        .to_string(),
                );
            }
        }
    }

    let match_expr = parse_if_condition(&block.header);
    let declared_in = if_declared_in_label(&scope, &match_expr);
    ConditionalRule {
        match_expr,
        vendor_option_space,
        options: collect_options_from_nodes_labeled(
            &block.children,
            definitions,
            vendor_id,
            file,
            Some(&declared_in),
        ),
        scope,
        source: Some(SourceRef::with_span(vendor_id, file, block.location.line, block.location.end_line)),
    }
}

pub fn parse_if_condition(header: &str) -> FilterMatch {
    let trimmed = header.trim();
    let inner = trimmed
        .strip_prefix("elsif")
        .or_else(|| trimmed.strip_prefix("if"))
        .unwrap_or(trimmed)
        .trim()
        .trim_start_matches('(')
        .trim_end_matches(')')
        .trim();
    parse_match_expression(inner, "if-condition")
}

pub fn parse_isc_pool(
    block: &IscBlock,
    file: &str,
    vendor_id: &str,
    definitions: &BTreeMap<String, OptionDef>,
    include_infoblox_range: bool,
) -> (Vec<(Ipv4Addr, Ipv4Addr)>, OptionMap) {
    let mut ranges = Vec::new();
    let options = collect_options_from_nodes_labeled(
        &block.children,
        definitions,
        vendor_id,
        file,
        Some("Pool"),
    );
    for child in &block.children {
        if let IscNode::Statement(s) = child {
            let text = s.text.trim();
            if let Some(r) = text.strip_prefix("range ") {
                if let Some((a, b)) = parse_range_pair(r) {
                    ranges.push((a, b));
                }
            } else if include_infoblox_range {
                if let Some(r) = text.strip_prefix("infoblox-range ") {
                    if let Some((a, b)) = parse_range_pair(r) {
                        ranges.push((a, b));
                    }
                }
            }
        }
    }
    ranges.sort();
    ranges.dedup();
    (ranges, options)
}

pub fn parse_range_pair(s: &str) -> Option<(Ipv4Addr, Ipv4Addr)> {
    let parts: Vec<_> = s.trim_end_matches(';').split_whitespace().collect();
    if parts.len() >= 2 {
        let a = parts[0].parse().ok()?;
        let b = parts[1].parse().ok()?;
        Some((a, b))
    } else {
        None
    }
}

pub fn push_pending_host(
    block: &IscBlock,
    file: &str,
    vendor_id: &str,
    definitions: &BTreeMap<String, OptionDef>,
    pending: &mut Vec<Reservation>,
) {
    if let Some(res) = parse_isc_host(block, file, vendor_id, definitions) {
        pending.push(res);
    }
}

pub fn parse_isc_host(
    block: &IscBlock,
    file: &str,
    vendor_id: &str,
    definitions: &BTreeMap<String, OptionDef>,
) -> Option<Reservation> {
    let mut mac = None;
    let mut ip = None;
    for child in &block.children {
        if let IscNode::Statement(s) = child {
            let text = s.text.trim();
            if text.starts_with("hardware ethernet ") {
                mac = Some(crate::model::normalize_mac(
                    text.strip_prefix("hardware ethernet ")?.trim_end_matches(';'),
                ));
            } else if text.starts_with("fixed-address ") {
                ip = text
                    .strip_prefix("fixed-address ")?
                    .trim_end_matches(';')
                    .parse()
                    .ok();
            }
        }
    }
    Some(Reservation {
        mac: mac?,
        ip: ip?,
        options: collect_options_from_nodes_labeled(
            &block.children,
            definitions,
            vendor_id,
            file,
            Some("Reservation"),
        ),
        extensions: BTreeMap::new(),
        source: Some(SourceRef::with_span(vendor_id, file, block.location.line, block.location.end_line)),
    })
}

pub fn parse_isc_class(
    block: &IscBlock,
    file: &str,
    vendor_id: &str,
    definitions: &BTreeMap<String, OptionDef>,
    parse_match: fn(&str) -> FilterMatch,
) -> anyhow::Result<Filter> {
    let name = extract_quoted(&block.header).unwrap_or_else(|| block.header.clone());
    let mut match_expr = FilterMatch::Opaque {
        raw: String::new(),
        kind_hint: "unknown".to_string(),
    };
    let mut vendor_option_space = None;

    for child in &block.children {
        if let IscNode::Statement(s) = child {
            let text = s.text.trim();
            if text.starts_with("match if") || text.starts_with("match substring") {
                match_expr = parse_match(text);
            } else if text.starts_with("vendor-option-space ") {
                vendor_option_space = Some(
                    text.strip_prefix("vendor-option-space ")
                        .unwrap_or("")
                        .trim_end_matches(';')
                        .to_string(),
                );
            }
        }
    }

    let declared_in = format!("Class {name}");
    Ok(Filter {
        name,
        match_expr,
        vendor_option_space,
        options: collect_options_from_nodes_labeled(
            &block.children,
            definitions,
            vendor_id,
            file,
            Some(&declared_in),
        ),
        extensions: BTreeMap::new(),
        source: Some(SourceRef::with_span(vendor_id, file, block.location.line, block.location.end_line)),
    })
}

pub fn parse_match_if(text: &str) -> FilterMatch {
    let inner = text
        .trim()
        .strip_prefix("match if")
        .unwrap_or(text)
        .trim()
        .trim_start_matches('(')
        .trim_end_matches(')')
        .trim_end_matches(';')
        .trim();

    parse_match_expression(inner, "match-if")
}

pub fn parse_bluecat_match(text: &str) -> FilterMatch {
    let trimmed = text.trim();
    if trimmed.starts_with("match if") {
        return parse_match_if(text);
    }
    if trimmed.starts_with("match substring") {
        let inner = trimmed
            .strip_prefix("match substring")
            .unwrap_or(trimmed)
            .trim()
            .trim_start_matches('(')
            .trim_end_matches(')')
            .trim_end_matches(';')
            .trim();
        let kind_hint = if inner.starts_with("hardware") {
            "hardware-substring"
        } else {
            "match-substring"
        };
        return FilterMatch::Opaque {
            raw: inner.to_string(),
            kind_hint: kind_hint.to_string(),
        };
    }
    FilterMatch::Opaque {
        raw: trimmed.to_string(),
        kind_hint: "unknown".to_string(),
    }
}

fn parse_match_expression(inner: &str, default_hint: &str) -> FilterMatch {
    if let Some(m) = Regex::new(r#"(?i)option\s+vendor-class-identifier\s*=\s*"([^"]+)""#)
        .ok()
        .and_then(|re| re.captures(inner))
    {
        return FilterMatch::VendorClassExact {
            value: m[1].to_string(),
        };
    }

    if let Some(m) = Regex::new(r#"option vendor-class-identifier="([^"]+)""#)
        .ok()
        .and_then(|re| re.captures(inner))
    {
        return FilterMatch::VendorClassExact {
            value: m[1].to_string(),
        };
    }

    if let Some(m) = VCI_SUBSTRING.captures(inner) {
        return FilterMatch::VendorClassPrefix {
            offset: m[1].parse().unwrap_or(0),
            length: m[2].parse().unwrap_or(0),
            value: m[3].to_string(),
        };
    }

    if let Some(m) = CLIENT_ID_SUBSTRING.captures(inner) {
        return FilterMatch::ClientIdPrefix {
            offset: m[1].parse().unwrap_or(0),
            length: m[2].parse().unwrap_or(0),
            value: m[3].to_string(),
        };
    }

    if let Some(m) = Regex::new(
        r#"substring\(binary-to-ascii\(16, 8, ":", option vendor-class-identifier\), 0,(\d+)\) = "([^"]+)""#,
    )
    .ok()
    .and_then(|re| re.captures(inner))
    {
        return FilterMatch::VendorClassPrefix {
            offset: 0,
            length: m[1].parse().unwrap_or(0),
            value: m[2].to_string(),
        };
    }

    if let Some(m) = Regex::new(r#"option host-name="([^"]+)""#)
        .ok()
        .and_then(|re| re.captures(inner))
    {
        return FilterMatch::HostnameExact {
            value: m[1].to_string(),
        };
    }

    FilterMatch::Opaque {
        raw: inner.to_string(),
        kind_hint: default_hint.to_string(),
    }
}

pub fn extract_quoted(s: &str) -> Option<String> {
    let start = s.find('"')?;
    let rest = &s[start + 1..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

pub fn assign_reservations_to_subnets(subnets: &mut [Subnet], reservations: Vec<Reservation>) {
    for res in reservations {
        let ip = res.ip;
        if let Some(subnet) = subnets.iter_mut().find(|s| s.network.contains(&ip)) {
            subnet.reservations.push(res);
        }
    }
}

pub fn attach_subclass(
    filters: &mut [Filter],
    class_name: &str,
    subclass_value: &str,
) {
    if let Some(filter) = filters.iter_mut().find(|f| f.name == class_name) {
        let subclasses = filter
            .extensions
            .entry("subclasses".to_string())
            .or_insert_with(|| serde_json::Value::Array(Vec::new()));
        if let serde_json::Value::Array(arr) = subclasses {
            arr.push(serde_json::Value::String(subclass_value.to_string()));
        }
    }
}

pub fn parse_subclass_statement(text: &str) -> Option<(String, String)> {
    let text = text.trim().strip_prefix("subclass ")?.trim_end_matches(';').trim();
    let class_name = extract_quoted(text)?;
    let after_name = text[text.find(&format!("\"{class_name}\""))? + class_name.len() + 2..].trim();
    let value = if after_name.starts_with('"') {
        extract_quoted(after_name)?
    } else {
        after_name.to_string()
    };
    Some((class_name, value))
}
