use std::collections::{BTreeMap, BTreeSet};
use std::net::Ipv4Addr;

use crate::model::{
    ClientScenario, ConditionalRule, Config, Filter, OptionMap, Pool, Reservation, RuleScope,
    Subnet,
};

/// Pre-indexed conditional rules for O(1) scope lookup during evaluation.
#[derive(Debug, Clone)]
pub struct RuleIndex<'a> {
    pub global: Vec<&'a ConditionalRule>,
    pub by_subnet: BTreeMap<&'a str, Vec<&'a ConditionalRule>>,
    pub by_shared: BTreeMap<&'a str, Vec<&'a ConditionalRule>>,
}

impl<'a> RuleIndex<'a> {
    pub fn build(config: &'a Config) -> Self {
        let mut global = Vec::new();
        let mut by_subnet: BTreeMap<&str, Vec<&ConditionalRule>> = BTreeMap::new();
        let mut by_shared: BTreeMap<&str, Vec<&ConditionalRule>> = BTreeMap::new();

        for rule in &config.conditional_rules {
            match &rule.scope {
                RuleScope::Global => global.push(rule),
                RuleScope::Subnet { cidr } => {
                    by_subnet.entry(cidr.as_str()).or_default().push(rule);
                }
                RuleScope::SharedNetwork { name } => {
                    by_shared.entry(name.as_str()).or_default().push(rule);
                }
            }
        }

        Self {
            global,
            by_subnet,
            by_shared,
        }
    }

    pub fn rules_for_subnet(&self, subnet: &Subnet) -> Vec<&'a ConditionalRule> {
        let cidr = subnet.network.to_string();
        let mut out = self.global.clone();
        if let Some(rules) = self.by_subnet.get(cidr.as_str()) {
            out.extend_from_slice(rules);
        }
        if let Some(name) = subnet.shared_network.as_deref() {
            if let Some(rules) = self.by_shared.get(name) {
                out.extend_from_slice(rules);
            }
        }
        out
    }
}

pub fn subnet_for_ip<'a>(config: &'a Config, ip: Ipv4Addr) -> Option<&'a Subnet> {
    config.subnets.iter().find(|s| s.network.contains(&ip))
}

/// Merge option layers; later layers win both value and declaration provenance.
pub fn merge_options(layers: &[&OptionMap]) -> OptionMap {
    let mut out = BTreeMap::new();
    for layer in layers {
        for (k, v) in *layer {
            out.insert(k.clone(), v.clone());
        }
    }
    out
}

/// Static inheritance through subnet (no pool/host): global → shared → subnet.
pub fn static_subnet_base(config: &Config, subnet: &Subnet) -> OptionMap {
    let empty = BTreeMap::new();
    let shared = subnet
        .shared_network
        .as_ref()
        .and_then(|n| config.shared_networks.get(n))
        .map(|sn| &sn.options)
        .unwrap_or(&empty);
    merge_options(&[&config.global_options, shared, &subnet.options])
}

pub fn effective_subnet_options(config: &Config, subnet: &Subnet) -> OptionMap {
    static_subnet_base(config, subnet)
}

pub fn effective_pool_options(config: &Config, subnet: &Subnet, pool: &Pool) -> OptionMap {
    let base = static_subnet_base(config, subnet);
    merge_options(&[&base, &pool.options])
}

pub fn pool_for_ip<'a>(subnet: &'a Subnet, ip: Ipv4Addr) -> Option<&'a Pool> {
    subnet
        .pools
        .iter()
        .find(|p| ip >= p.start && ip <= p.end)
}

pub fn effective_reservation_options(
    config: &Config,
    subnet: &Subnet,
    res: &Reservation,
) -> OptionMap {
    let index = RuleIndex::build(config);
    let base = static_subnet_base(config, subnet);
    effective_client_options_with(
        config,
        subnet,
        pool_for_ip(subnet, res.ip),
        Some(res),
        &ClientScenario::baseline(),
        &index,
        &base,
    )
}

/// Effective options a client would receive for the given scenario.
pub fn effective_client_options(
    config: &Config,
    subnet: &Subnet,
    pool: Option<&Pool>,
    reservation: Option<&Reservation>,
    scenario: &ClientScenario,
) -> OptionMap {
    let index = RuleIndex::build(config);
    let base = static_subnet_base(config, subnet);
    effective_client_options_with(config, subnet, pool, reservation, scenario, &index, &base)
}

/// Precomputed filter/conditional contribution for one (subnet, scenario).
#[derive(Debug, Clone, Default)]
pub struct ScenarioOverlay {
    pub options: OptionMap,
    pub allowed_spaces: BTreeSet<String>,
}

impl ScenarioOverlay {
    pub fn compute(
        config: &Config,
        subnet: &Subnet,
        scenario: &ClientScenario,
        index: &RuleIndex<'_>,
    ) -> Self {
        let mut allowed_spaces: BTreeSet<String> = BTreeSet::new();
        let mut options: OptionMap = BTreeMap::new();

        let mut apply_filter = |filter: &Filter| {
            if filter.match_expr.matches(scenario) {
                if let Some(space) = &filter.vendor_option_space {
                    allowed_spaces.insert(space.clone());
                }
                for (k, v) in &filter.options {
                    if k.space != "dhcp" && !k.is_bootp() && !k.is_unresolved() {
                        allowed_spaces.insert(k.space.clone());
                    }
                    options.insert(k.clone(), v.clone());
                }
            }
        };

        for filter in &config.global_filters {
            apply_filter(filter);
        }
        for filter in &subnet.filters {
            apply_filter(filter);
        }
        for rule in index.rules_for_subnet(subnet) {
            if !rule.match_expr.matches(scenario) {
                continue;
            }
            if let Some(space) = &rule.vendor_option_space {
                allowed_spaces.insert(space.clone());
            }
            for (k, v) in &rule.options {
                if k.space != "dhcp" && !k.is_bootp() && !k.is_unresolved() {
                    allowed_spaces.insert(k.space.clone());
                }
                options.insert(k.clone(), v.clone());
            }
        }

        Self {
            options,
            allowed_spaces,
        }
    }

    pub fn apply(
        &self,
        subnet_base: &OptionMap,
        pool: Option<&Pool>,
        reservation: Option<&Reservation>,
    ) -> OptionMap {
        let empty = BTreeMap::new();
        let pool_opts = pool.map(|p| &p.options).unwrap_or(&empty);
        let res_opts = reservation.map(|r| &r.options).unwrap_or(&empty);

        let mut merged = merge_options(&[subnet_base, pool_opts, res_opts]);
        for (k, v) in &self.options {
            merged.insert(k.clone(), v.clone());
        }

        // Vendor options (any scope, including host) are only effective when a matching
        // filter/conditional activated that vendor-option-space for this scenario.
        gate_vendor_options(merged, &self.allowed_spaces)
    }
}

/// Fast path using a prebuilt rule index and static subnet base.
pub fn effective_client_options_with(
    config: &Config,
    subnet: &Subnet,
    pool: Option<&Pool>,
    reservation: Option<&Reservation>,
    scenario: &ClientScenario,
    index: &RuleIndex<'_>,
    subnet_base: &OptionMap,
) -> OptionMap {
    let overlay = ScenarioOverlay::compute(config, subnet, scenario, index);
    overlay.apply(subnet_base, pool, reservation)
}

/// Fast path with a precomputed scenario overlay (avoids re-scanning filters/rules).
pub fn effective_client_options_overlay(
    subnet_base: &OptionMap,
    pool: Option<&Pool>,
    reservation: Option<&Reservation>,
    overlay: &ScenarioOverlay,
) -> OptionMap {
    overlay.apply(subnet_base, pool, reservation)
}

fn gate_vendor_options(map: OptionMap, allowed_spaces: &BTreeSet<String>) -> OptionMap {
    map.into_iter()
        .filter(|(k, _)| {
            k.space == "dhcp"
                || k.is_bootp()
                || k.is_unresolved()
                || allowed_spaces.contains(&k.space)
        })
        .collect()
}

/// VCIs that can affect options for this subnet (global + subnet-scoped rules/filters).
pub fn vcis_affecting_subnet(
    config: &Config,
    subnet: &Subnet,
    index: &RuleIndex<'_>,
) -> BTreeSet<String> {
    let mut vcis = BTreeSet::new();
    let mut collect = |m: &crate::model::FilterMatch| {
        if let Some(v) = m.vendor_class_hint() {
            vcis.insert(v);
        }
    };
    for filter in &config.global_filters {
        collect(&filter.match_expr);
    }
    for filter in &subnet.filters {
        collect(&filter.match_expr);
    }
    for rule in index.rules_for_subnet(subnet) {
        collect(&rule.match_expr);
    }
    vcis
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{BoundOption, NormalizedValue, OptionKey, SourceRef};

    #[test]
    fn merge_options_preserves_winning_declaration() {
        let global = [(
            OptionKey::dhcp(15),
            BoundOption::new(
                NormalizedValue::String("global.example".into()),
                Some(SourceRef::new("infoblox", "a.conf", 1)),
            )
            .with_declared_in("Global"),
        )]
        .into_iter()
        .collect();
        let subnet = [(
            OptionKey::dhcp(15),
            BoundOption::new(
                NormalizedValue::String("subnet.example".into()),
                Some(SourceRef::new("infoblox", "a.conf", 10)),
            )
            .with_declared_in("Subnet"),
        )]
        .into_iter()
        .collect();
        let pool = [(
            OptionKey::dhcp(15),
            BoundOption::new(
                NormalizedValue::String("pool.example".into()),
                Some(SourceRef::new("infoblox", "a.conf", 20)),
            )
            .with_declared_in("Pool"),
        )]
        .into_iter()
        .collect();

        let over_global = merge_options(&[&global, &subnet]);
        let won = over_global.get(&OptionKey::dhcp(15)).unwrap();
        assert_eq!(won.value, NormalizedValue::String("subnet.example".into()));
        assert_eq!(won.source.as_ref().map(|s| s.line), Some(10));
        assert_eq!(won.declared_in.as_deref(), Some("Subnet"));

        let over_subnet = merge_options(&[&global, &subnet, &pool]);
        let won = over_subnet.get(&OptionKey::dhcp(15)).unwrap();
        assert_eq!(won.value, NormalizedValue::String("pool.example".into()));
        assert_eq!(won.source.as_ref().map(|s| s.line), Some(20));
        assert_eq!(won.declared_in.as_deref(), Some("Pool"));
    }
}
