use std::net::Ipv4Addr;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
pub enum NormalizedValue {
    Ip(Ipv4Addr),
    IpList(Vec<Ipv4Addr>),
    Bool(bool),
    Int(i64),
    String(String),
    Hex(String),
    Opaque(String),
}

impl NormalizedValue {
    pub fn from_raw_text(raw: &str) -> Self {
        let trimmed = raw.trim().trim_end_matches(';').trim();
        if trimmed.eq_ignore_ascii_case("true") || trimmed.eq_ignore_ascii_case("on") {
            return Self::Bool(true);
        }
        if trimmed.eq_ignore_ascii_case("false") || trimmed.eq_ignore_ascii_case("off") {
            return Self::Bool(false);
        }
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            let hex = trimmed
                .trim_start_matches('[')
                .trim_end_matches(']')
                .chars()
                .filter(|c| !c.is_whitespace())
                .collect::<String>()
                .to_ascii_lowercase();
            return Self::Hex(hex);
        }
        if let Ok(ip) = trimmed.parse::<Ipv4Addr>() {
            return Self::Ip(ip);
        }
        if let Ok(n) = trimmed.parse::<i64>() {
            return Self::Int(n);
        }
        if trimmed.contains(',') {
            let ips: Vec<Ipv4Addr> = trimmed
                .split(',')
                .filter_map(|p| p.trim().parse().ok())
                .collect();
            if !ips.is_empty() && ips.len() == trimmed.split(',').count() {
                return Self::IpList(ips);
            }
        }
        let looks_quoted = trimmed.starts_with('"') && trimmed.ends_with('"') && trimmed.len() >= 2;
        if !looks_quoted {
            if let Some(hex) = parse_colon_hex(trimmed) {
                return Self::Hex(hex);
            }
        }
        let unquoted = trimmed.trim_matches('"');
        Self::String(unquoted.to_string())
    }

    pub fn sort_ip_list_if_needed(&mut self) {
        if let Self::IpList(ips) = self {
            ips.sort();
            ips.dedup();
        }
    }
}

/// ISC string/data option form: colon-separated hex octets (1–2 digits each).
fn parse_colon_hex(s: &str) -> Option<String> {
    if !s.contains(':') {
        return None;
    }
    let parts: Vec<&str> = s.split(':').map(str::trim).collect();
    if parts.len() < 2 {
        return None;
    }
    if !parts.iter().all(|p| {
        (1..=2).contains(&p.len()) && p.chars().all(|c| c.is_ascii_hexdigit())
    }) {
        return None;
    }
    Some(
        parts
            .iter()
            .map(|p| p.to_ascii_lowercase())
            .collect::<Vec<_>>()
            .join(":"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colon_hex_case_insensitive() {
        let upper = NormalizedValue::from_raw_text("AA:BB:CC");
        let lower = NormalizedValue::from_raw_text("aa:bb:cc");
        assert_eq!(upper, lower);
        assert_eq!(upper, NormalizedValue::Hex("aa:bb:cc".into()));
    }

    #[test]
    fn colon_hex_mixed_case_and_single_digit() {
        assert_eq!(
            NormalizedValue::from_raw_text("A:b:0C"),
            NormalizedValue::Hex("a:b:0c".into())
        );
    }

    #[test]
    fn quoted_colon_text_stays_string() {
        assert_eq!(
            NormalizedValue::from_raw_text("\"AA:BB\""),
            NormalizedValue::String("AA:BB".into())
        );
    }

    #[test]
    fn bracket_hex_still_lowercased() {
        assert_eq!(
            NormalizedValue::from_raw_text("[AA BB CC]"),
            NormalizedValue::Hex("aabbcc".into())
        );
    }
}
