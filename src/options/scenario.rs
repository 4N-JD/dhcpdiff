use std::collections::BTreeSet;

use crate::model::{ClientScenario, Config, FilterMatch, Subnet};
use crate::options::inherit::{vcis_affecting_subnet, RuleIndex};

/// Discover baseline + one scenario per distinct vendor-class identifier found in the config.
pub fn discover_scenarios(config: &Config) -> Vec<ClientScenario> {
    let mut vcis: BTreeSet<String> = BTreeSet::new();

    let mut collect = |m: &FilterMatch| {
        if let Some(v) = m.vendor_class_hint() {
            vcis.insert(v);
        }
    };

    for filter in &config.global_filters {
        collect(&filter.match_expr);
    }
    for subnet in &config.subnets {
        for filter in &subnet.filters {
            collect(&filter.match_expr);
        }
    }
    for rule in &config.conditional_rules {
        collect(&rule.match_expr);
    }

    let mut out = vec![ClientScenario::baseline()];
    for vci in vcis {
        out.push(ClientScenario::with_vendor_class(vci));
    }
    out
}

/// Union of scenarios from two configs (baseline once, all VCIs sorted).
pub fn union_scenarios(source: &Config, target: &Config) -> Vec<ClientScenario> {
    let mut vcis: BTreeSet<String> = BTreeSet::new();
    for cfg in [source, target] {
        for s in discover_scenarios(cfg) {
            if let Some(v) = s.vendor_class {
                vcis.insert(v);
            }
        }
    }
    let mut out = vec![ClientScenario::baseline()];
    for vci in vcis {
        out.push(ClientScenario::with_vendor_class(vci));
    }
    out
}

/// Scenarios relevant to a matched subnet pair: baseline + VCIs that can affect either side.
pub fn scenarios_for_subnet_pair(
    source_cfg: &Config,
    source_subnet: &Subnet,
    source_index: &RuleIndex<'_>,
    target_cfg: &Config,
    target_subnet: &Subnet,
    target_index: &RuleIndex<'_>,
) -> Vec<ClientScenario> {
    let mut vcis = vcis_affecting_subnet(source_cfg, source_subnet, source_index);
    vcis.extend(vcis_affecting_subnet(target_cfg, target_subnet, target_index));

    let mut out = vec![ClientScenario::baseline()];
    for vci in vcis {
        out.push(ClientScenario::with_vendor_class(vci));
    }
    out
}
