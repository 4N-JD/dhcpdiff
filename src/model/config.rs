use std::collections::BTreeMap;

use ipnet::Ipv4Net;
use serde::{Deserialize, Serialize};

use super::{FilterMatch, OptionDefMap, OptionMap, SourceRef};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Config {
    pub version: u32,
    pub option_definitions: OptionDefMap,
    pub global_options: OptionMap,
    pub global_filters: Vec<Filter>,
    pub conditional_rules: Vec<ConditionalRule>,
    pub shared_networks: BTreeMap<String, SharedNetwork>,
    pub subnets: Vec<Subnet>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            version: 1,
            option_definitions: BTreeMap::new(),
            global_options: BTreeMap::new(),
            global_filters: Vec::new(),
            conditional_rules: Vec::new(),
            shared_networks: BTreeMap::new(),
            subnets: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum RuleScope {
    Global,
    Subnet { cidr: String },
    SharedNetwork { name: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConditionalRule {
    pub match_expr: FilterMatch,
    pub vendor_option_space: Option<String>,
    pub options: OptionMap,
    pub scope: RuleScope,
    pub source: Option<SourceRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SharedNetwork {
    pub name: String,
    pub options: OptionMap,
    pub source: Option<SourceRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Subnet {
    pub network: Ipv4Net,
    pub shared_network: Option<String>,
    pub options: OptionMap,
    pub pools: Vec<Pool>,
    pub reservations: Vec<Reservation>,
    pub filters: Vec<Filter>,
    pub extensions: BTreeMap<String, serde_json::Value>,
    pub source: Option<SourceRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pool {
    pub start: std::net::Ipv4Addr,
    pub end: std::net::Ipv4Addr,
    pub options: OptionMap,
    pub extensions: BTreeMap<String, serde_json::Value>,
    pub source: Option<SourceRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reservation {
    pub mac: String,
    pub ip: std::net::Ipv4Addr,
    pub options: OptionMap,
    pub extensions: BTreeMap<String, serde_json::Value>,
    pub source: Option<SourceRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Filter {
    pub name: String,
    pub match_expr: FilterMatch,
    pub vendor_option_space: Option<String>,
    pub options: OptionMap,
    pub extensions: BTreeMap<String, serde_json::Value>,
    pub source: Option<SourceRef>,
}

impl Config {
    pub fn subnet_count(&self) -> usize {
        self.subnets.len()
    }

    pub fn pool_count(&self) -> usize {
        self.subnets.iter().map(|s| s.pools.len()).sum()
    }

    pub fn reservation_count(&self) -> usize {
        self.subnets.iter().map(|s| s.reservations.len()).sum()
    }

    pub fn filter_count(&self) -> usize {
        self.global_filters.len()
            + self
                .subnets
                .iter()
                .map(|s| s.filters.len())
                .sum::<usize>()
    }
}

pub fn normalize_mac(mac: &str) -> String {
    let hex: String = mac
        .to_ascii_lowercase()
        .chars()
        .filter(|c| c.is_ascii_hexdigit())
        .collect();
    if hex.len() == 12 {
        hex.chars()
            .collect::<Vec<_>>()
            .chunks(2)
            .map(|c| c.iter().collect::<String>())
            .collect::<Vec<_>>()
            .join(":")
    } else {
        mac.to_ascii_lowercase().replace('-', ":")
    }
}
