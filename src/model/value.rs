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
