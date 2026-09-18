use std::collections::BTreeMap;

use crate::model::{BoundOption, NormalizedValue, OptionDef, OptionKey, OptionMap, SourceRef};

use super::ast::{IscNode, IscStatement};

/// Parse ISC BOOTP packet fields into the synthetic `bootp` space.
/// Never mapped to DHCP options 66/67/150.
pub fn parse_bootp_statement(text: &str) -> Option<(OptionKey, NormalizedValue)> {
    let trimmed = text.trim().trim_end_matches(';');
    if let Some(value) = trimmed.strip_prefix("filename ") {
        let value = value.trim().trim_matches('"');
        return Some((
            OptionKey::bootp_filename(),
            NormalizedValue::from_raw_text(&format!("\"{value}\"")),
        ));
    }
    if let Some(value) = trimmed.strip_prefix("next-server ") {
        let value = value.trim();
        let mut val = NormalizedValue::from_raw_text(value);
        val.sort_ip_list_if_needed();
        return Some((OptionKey::bootp_next_server(), val));
    }
    if let Some(value) = trimmed.strip_prefix("server-name ") {
        let value = value.trim().trim_matches('"');
        return Some((
            OptionKey::bootp_server_name(),
            NormalizedValue::from_raw_text(&format!("\"{value}\"")),
        ));
    }
    None
}

pub fn insert_statement_options(
    map: &mut OptionMap,
    definitions: &BTreeMap<String, OptionDef>,
    text: &str,
    source: Option<SourceRef>,
) {
    insert_statement_options_labeled(map, definitions, text, source, None);
}

pub fn insert_statement_options_labeled(
    map: &mut OptionMap,
    definitions: &BTreeMap<String, OptionDef>,
    text: &str,
    source: Option<SourceRef>,
    declared_in: Option<&str>,
) {
    let bind = |value: NormalizedValue| {
        let mut bound = BoundOption::new(value, source.clone());
        if let Some(label) = declared_in {
            bound.declared_in = Some(label.to_string());
        }
        bound
    };

    if let Some((name, value)) = parse_option_statement(text) {
        let key = option_key_from_name(&name, definitions);
        let mut val = NormalizedValue::from_raw_text(&value);
        val.sort_ip_list_if_needed();
        map.insert(key, bind(val));
    } else if let Some((key, val)) = parse_bootp_statement(text) {
        map.insert(key, bind(val));
    } else if let Some((key, val)) = parse_lease_time_statement(text) {
        map.insert(key, bind(val));
    }
}

/// Parse ISC lease-time server statements into the synthetic `isc` space.
/// Never mapped to DHCP option 51 (`option dhcp-lease-time`).
pub fn parse_lease_time_statement(text: &str) -> Option<(OptionKey, NormalizedValue)> {
    let trimmed = text.trim().trim_end_matches(';');
    let (key, value) = if let Some(v) = trimmed.strip_prefix("default-lease-time ") {
        (OptionKey::isc_default_lease_time(), v)
    } else if let Some(v) = trimmed.strip_prefix("min-lease-time ") {
        (OptionKey::isc_min_lease_time(), v)
    } else if let Some(v) = trimmed.strip_prefix("max-lease-time ") {
        (OptionKey::isc_max_lease_time(), v)
    } else {
        return None;
    };
    let n: i64 = value.trim().parse().ok()?;
    Some((key, NormalizedValue::Int(n)))
}

pub fn parse_option_statement(text: &str) -> Option<(String, String)> {
    let trimmed = text.trim();
    if !trimmed.starts_with("option ") {
        return None;
    }
    let rest = trimmed.strip_prefix("option ")?.trim();
    if rest.contains(" code ") {
        return None;
    }
    // `option space VENDOR;` declares a vendor option space — not an assignment
    if rest.starts_with("space ") || rest == "space" {
        return None;
    }
    // `option name = value;` assignment form (ISC config-option syntax)
    if let Some((name, value)) = split_option_assignment(rest) {
        return Some((name, value));
    }
    let mut parts = rest.splitn(2, char::is_whitespace);
    let name = parts.next()?.trim().to_string();
    let value = parts.next()?.trim().to_string();
    Some((name, value))
}

fn split_option_assignment(rest: &str) -> Option<(String, String)> {
    let mut in_quote = false;
    let mut quote_char = '\0';
    for (i, ch) in rest.char_indices() {
        match ch {
            '"' | '\'' if !in_quote => {
                in_quote = true;
                quote_char = ch;
            }
            c if in_quote && c == quote_char => {
                in_quote = false;
            }
            '=' if !in_quote => {
                let name = rest[..i].trim().to_string();
                let value = rest[i + 1..].trim().trim_end_matches(';').to_string();
                if !name.is_empty() {
                    return Some((name, value));
                }
            }
            _ => {}
        }
    }
    None
}

pub fn parse_option_definition(text: &str) -> Option<OptionDef> {
    let trimmed = text.trim();
    if !trimmed.starts_with("option ") || !trimmed.contains(" code ") {
        return None;
    }
    let rest = trimmed.strip_prefix("option ")?.trim();
    let (name_part, type_part) = rest.split_once(" code ")?;
    let name = name_part.trim().to_string();
    let (space, simple_name) = if let Some((sp, nm)) = name.rsplit_once('.') {
        (sp.to_string(), nm.to_string())
    } else {
        ("dhcp".to_string(), name.clone())
    };
    let mut code_and_type = type_part.splitn(2, '=');
    let code_str = code_and_type.next()?.trim();
    let value_type = code_and_type
        .next()
        .map(|t| t.trim().trim_end_matches(';').to_string())
        .unwrap_or_default();
    let code: u16 = code_str.parse().ok()?;
    Some(OptionDef {
        name: if space == "dhcp" {
            simple_name
        } else {
            format!("{space}.{simple_name}")
        },
        space,
        code,
        value_type,
    })
}

pub fn option_key_from_name(name: &str, definitions: &BTreeMap<String, OptionDef>) -> OptionKey {
    if let Some(def) = definitions.get(name) {
        return OptionKey::qualified(&def.space, def.code);
    }
    if let Some((space, simple)) = name.rsplit_once('.') {
        let fq = format!("{space}.{simple}");
        if let Some(def) = definitions.get(&fq) {
            return OptionKey::qualified(&def.space, def.code);
        }
    }
    crate::options::builtin::lookup_by_name(name)
        .map(|code| OptionKey::dhcp(code))
        .or_else(|| {
            // Microsoft-style zero-padded codes e.g. "006"
            if name.chars().all(|c| c.is_ascii_digit()) {
                name.parse::<u16>().ok().map(OptionKey::dhcp)
            } else {
                None
            }
        })
        .unwrap_or_else(|| OptionKey::unresolved(name))
}

pub fn statement_source(
    vendor_id: &str,
    file: &str,
    stmt: &IscStatement,
) -> SourceRef {
    SourceRef::with_span(vendor_id, file, stmt.location.line, stmt.location.end_line)
}

pub fn collect_options_from_nodes(
    nodes: &[IscNode],
    definitions: &BTreeMap<String, OptionDef>,
    vendor_id: &str,
    file: &str,
) -> OptionMap {
    collect_options_from_nodes_labeled(nodes, definitions, vendor_id, file, None)
}

pub fn collect_options_from_nodes_labeled(
    nodes: &[IscNode],
    definitions: &BTreeMap<String, OptionDef>,
    vendor_id: &str,
    file: &str,
    declared_in: Option<&str>,
) -> OptionMap {
    let mut map = BTreeMap::new();
    for node in nodes {
        if let IscNode::Statement(stmt) = node {
            insert_statement_options_labeled(
                &mut map,
                definitions,
                &stmt.text,
                Some(statement_source(vendor_id, file, stmt)),
                declared_in,
            );
        }
    }
    map
}

pub fn collect_definitions(nodes: &[IscNode]) -> BTreeMap<String, OptionDef> {
    let mut defs = BTreeMap::new();
    walk_nodes(nodes, &mut |text| {
        if let Some(def) = parse_option_definition(text) {
            defs.insert(def.name.clone(), def);
        }
    });
    defs
}

pub fn walk_nodes<F: FnMut(&str)>(nodes: &[IscNode], f: &mut F) {
    for node in nodes {
        match node {
            IscNode::Statement(IscStatement { text, .. }) => f(text),
            IscNode::Block(block) => walk_nodes(&block.children, f),
        }
    }
}
