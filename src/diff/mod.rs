use std::collections::BTreeMap;
use std::net::Ipv4Addr;

use serde::{Deserialize, Serialize};

use crate::model::{
    BoundOption, ClientScenario, Config, Filter, LocationRef, NormalizedValue, OptionKey, OptionMap,
    Pool, Reservation, SideLocations, SourceRef, Subnet,
};
use crate::options::inherit::{
    effective_client_options_overlay, pool_for_ip, static_subnet_base, subnet_for_ip, RuleIndex,
    ScenarioOverlay,
};
use crate::options::labels::OptionLabeler;
use crate::options::scenario::scenarios_for_subnet_pair;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiffLocations {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<SideLocations>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<SideLocations>,
}

impl DiffLocations {
    pub fn source_only(loc: Option<LocationRef>) -> Self {
        Self {
            source: loc.map(SideLocations::affected_only),
            target: None,
        }
    }

    pub fn target_only(loc: Option<LocationRef>) -> Self {
        Self {
            source: None,
            target: loc.map(SideLocations::affected_only),
        }
    }

    pub fn both(source: Option<LocationRef>, target: Option<LocationRef>) -> Self {
        Self {
            source: source.map(SideLocations::affected_only),
            target: target.map(SideLocations::affected_only),
        }
    }

    pub fn both_sides(source: Option<SideLocations>, target: Option<SideLocations>) -> Self {
        Self { source, target }
    }

    pub fn source_only_side(side: Option<SideLocations>) -> Self {
        Self {
            source: side,
            target: None,
        }
    }

    pub fn target_only_side(side: Option<SideLocations>) -> Self {
        Self {
            source: None,
            target: side,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChangedValues {
    pub source: NormalizedValue,
    pub target: NormalizedValue,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "category")]
pub enum DiffEntry {
    #[serde(rename = "missing", alias = "MissingInTarget")]
    MissingInTarget {
        entity: EntityRef,
        detail: String,
        #[serde(default)]
        locations: DiffLocations,
    },
    #[serde(rename = "extra", alias = "ExtraInTarget")]
    ExtraInTarget {
        entity: EntityRef,
        detail: String,
        #[serde(default)]
        locations: DiffLocations,
    },
    #[serde(rename = "changed", alias = "Changed")]
    Changed {
        entity: EntityRef,
        field: String,
        detail: String,
        values: ChangedValues,
        #[serde(default)]
        locations: DiffLocations,
    },
    #[serde(rename = "unmapped", alias = "Unmapped")]
    Unmapped {
        entity: EntityRef,
        option_key: OptionKey,
        detail: String,
        #[serde(default)]
        locations: DiffLocations,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct EntityRef {
    pub kind: String,
    pub key: String,
    /// Human-oriented breakdown of `kind`/`key` for UIs and text reports.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display: Option<EntityDisplay>,
}

/// Structured, human-readable view of an entity key.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct EntityDisplay {
    /// e.g. "Option", "Pool", "Subnet", "Reservation", "Filter"
    pub object_type: String,
    /// Primary identifier (option label, pool range, CIDR, IP, filter name)
    pub name: String,
    /// Containing scope, when applicable (e.g. "Subnet 10.0.0.0/24")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    /// Machine-readable parent id for grouping (subnet CIDR, `global`, option scope, …)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_key: Option<String>,
    /// Vendor-class identifier for scenario diffs, when present
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vci: Option<String>,
    /// Where the winning option value was declared, when distinct from the affected child
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub declared_in: Option<String>,
    /// Machine id for option declaration site (`global`, `subnet:{cidr}`, `pool:…`, …)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub declaration_key: Option<String>,
    /// Option identity `space:code` for ignore-parent grouping
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub option_id: Option<String>,
    /// One-line summary suitable for list rows
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FileMeta {
    pub path: String,
    pub vendor: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiffCounts {
    pub missing: usize,
    pub extra: usize,
    pub changed: usize,
    pub unmapped: usize,
    pub total: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffReport {
    pub version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<FileMeta>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<FileMeta>,
    pub counts: DiffCounts,
    pub entries: Vec<DiffEntry>,
    pub has_differences: bool,
}

impl Default for DiffReport {
    fn default() -> Self {
        Self {
            version: 1,
            source: None,
            target: None,
            counts: DiffCounts::default(),
            entries: Vec::new(),
            has_differences: false,
        }
    }
}

impl DiffReport {
    pub fn from_entries(entries: Vec<DiffEntry>) -> Self {
        let counts = count_entries(&entries);
        let has_differences = !entries.is_empty();
        Self {
            version: 1,
            source: None,
            target: None,
            counts,
            entries,
            has_differences,
        }
    }

    pub fn with_file_meta(mut self, source: FileMeta, target: FileMeta) -> Self {
        self.source.replace(source);
        self.target.replace(target);
        self
    }
}

fn count_entries(entries: &[DiffEntry]) -> DiffCounts {
    let mut counts = DiffCounts {
        total: entries.len(),
        ..DiffCounts::default()
    };
    for e in entries {
        match e {
            DiffEntry::MissingInTarget { .. } => counts.missing += 1,
            DiffEntry::ExtraInTarget { .. } => counts.extra += 1,
            DiffEntry::Changed { .. } => counts.changed += 1,
            DiffEntry::Unmapped { .. } => counts.unmapped += 1,
        }
    }
    counts
}

fn loc_of(source: &Option<SourceRef>) -> Option<LocationRef> {
    source.as_ref().map(LocationRef::from_source_ref)
}

struct DiffCtx<'a> {
    source: &'a Config,
    target: &'a Config,
    source_index: RuleIndex<'a>,
    target_index: RuleIndex<'a>,
    labels: OptionLabeler,
}

pub fn diff_configs(source: &Config, target: &Config) -> DiffReport {
    diff_configs_with_progress(source, target, false)
}

pub fn diff_configs_with_progress(source: &Config, target: &Config, progress: bool) -> DiffReport {
    let mut entries = Vec::new();
    let ctx = DiffCtx {
        source,
        target,
        source_index: RuleIndex::build(source),
        target_index: RuleIndex::build(target),
        labels: OptionLabeler::from_configs(source, target),
    };

    if progress {
        eprintln!(
            "Diffing {} / {} subnets, {} / {} reservations, {} / {} conditional rules...",
            source.subnet_count(),
            target.subnet_count(),
            source.reservation_count(),
            target.reservation_count(),
            source.conditional_rules.len(),
            target.conditional_rules.len(),
        );
    }

    let source_subnets: BTreeMap<String, &Subnet> = source
        .subnets
        .iter()
        .map(|s| (s.network.to_string(), s))
        .collect();
    let target_subnets: BTreeMap<String, &Subnet> = target
        .subnets
        .iter()
        .map(|s| (s.network.to_string(), s))
        .collect();

    let mut matched = 0usize;
    for (cidr, sn) in &source_subnets {
        if !target_subnets.contains_key(cidr) {
            entries.push(DiffEntry::MissingInTarget {
                entity: entity("subnet", cidr),
                detail: format!("subnet {cidr} missing in target"),
                locations: DiffLocations::source_only(loc_of(&sn.source)),
            });
        } else {
            matched += 1;
            diff_subnet(&ctx, sn, target_subnets[cidr], &mut entries);
        }
    }
    for cidr in target_subnets.keys() {
        if !source_subnets.contains_key(cidr) {
            let sn = target_subnets[cidr];
            entries.push(DiffEntry::ExtraInTarget {
                entity: entity("subnet", cidr),
                detail: format!("subnet {cidr} extra in target"),
                locations: DiffLocations::target_only(loc_of(&sn.source)),
            });
        }
    }

    if progress {
        eprintln!("Compared {matched} matched subnets (pools); comparing reservations...");
    }

    diff_global_filters(&source.global_filters, &target.global_filters, &mut entries);
    diff_options(
        "global",
        &source.global_options,
        &target.global_options,
        &ctx.labels,
        None,
        None,
        &mut entries,
    );
    diff_reservations(&ctx, &mut entries);

    if progress {
        eprintln!("Diff complete ({} entries).", entries.len());
    }

    DiffReport::from_entries(entries)
}

fn scenario_cache_key(scenario: &ClientScenario) -> String {
    scenario
        .vendor_class
        .clone()
        .unwrap_or_else(|| String::from("__baseline__"))
}

fn build_overlays(
    config: &Config,
    subnet: &Subnet,
    index: &RuleIndex<'_>,
    scenarios: &[ClientScenario],
) -> BTreeMap<String, ScenarioOverlay> {
    scenarios
        .iter()
        .map(|s| {
            (
                scenario_cache_key(s),
                ScenarioOverlay::compute(config, subnet, s, index),
            )
        })
        .collect()
}

fn diff_subnet(
    ctx: &DiffCtx<'_>,
    source: &Subnet,
    target: &Subnet,
    entries: &mut Vec<DiffEntry>,
) {
    let cidr = source.network.to_string();
    let scenarios = scenarios_for_subnet_pair(
        ctx.source,
        source,
        &ctx.source_index,
        ctx.target,
        target,
        &ctx.target_index,
    );
    let src_base = static_subnet_base(ctx.source, source);
    let tgt_base = static_subnet_base(ctx.target, target);
    let src_overlays = build_overlays(ctx.source, source, &ctx.source_index, &scenarios);
    let tgt_overlays = build_overlays(ctx.target, target, &ctx.target_index, &scenarios);

    let src_pools: BTreeMap<String, &Pool> = source
        .pools
        .iter()
        .map(|p| (pool_key(p), p))
        .collect();
    let tgt_pools: BTreeMap<String, &Pool> = target
        .pools
        .iter()
        .map(|p| (pool_key(p), p))
        .collect();

    for (k, p) in &src_pools {
        if !tgt_pools.contains_key(k) {
            entries.push(DiffEntry::MissingInTarget {
                entity: entity("pool", &format!("{cidr}:{k}")),
                detail: format!("pool {k} missing in target"),
                locations: DiffLocations::both(loc_of(&p.source), loc_of(&target.source)),
            });
        } else {
            let tgt_pool = tgt_pools[k];
            diff_entity_scenarios(
                ctx,
                &src_base,
                &tgt_base,
                &src_overlays,
                &tgt_overlays,
                Some(p),
                Some(tgt_pool),
                None,
                None,
                &scenarios,
                &format!("pool:{cidr}:{k}"),
                loc_of(&p.source),
                loc_of(&tgt_pool.source),
                entries,
            );
        }
    }
    for k in tgt_pools.keys() {
        if !src_pools.contains_key(k) {
            let p = tgt_pools[k];
            entries.push(DiffEntry::ExtraInTarget {
                entity: entity("pool", &format!("{cidr}:{k}")),
                detail: format!("pool {k} extra in target"),
                locations: DiffLocations::both(loc_of(&source.source), loc_of(&p.source)),
            });
        }
    }

    let src_f: BTreeMap<String, &Filter> = source
        .filters
        .iter()
        .map(|f| (filter_key(f), f))
        .collect();
    let tgt_f: BTreeMap<String, &Filter> = target
        .filters
        .iter()
        .map(|f| (filter_key(f), f))
        .collect();

    for (k, f) in &src_f {
        if !tgt_f.contains_key(k) {
            entries.push(DiffEntry::MissingInTarget {
                entity: entity("filter", &format!("{cidr}:{k}")),
                detail: format!("filter {k} missing in target"),
                locations: DiffLocations::both(loc_of(&f.source), loc_of(&target.source)),
            });
        } else {
            let t = tgt_f[k];
            if f.vendor_option_space != t.vendor_option_space {
                let src_v = opt_string_value(&f.vendor_option_space);
                let tgt_v = opt_string_value(&t.vendor_option_space);
                entries.push(DiffEntry::Changed {
                    entity: entity("filter", &format!("{cidr}:{k}")),
                    field: "vendor_option_space".to_string(),
                    detail: format!("vendor_option_space: {src_v:?} -> {tgt_v:?}"),
                    values: ChangedValues {
                        source: src_v,
                        target: tgt_v,
                    },
                    locations: DiffLocations::both(loc_of(&f.source), loc_of(&t.source)),
                });
            }
        }
    }
    for k in tgt_f.keys() {
        if !src_f.contains_key(k) {
            let f = tgt_f[k];
            entries.push(DiffEntry::ExtraInTarget {
                entity: entity("filter", &format!("{cidr}:{k}")),
                detail: format!("filter {k} extra in target"),
                locations: DiffLocations::both(loc_of(&source.source), loc_of(&f.source)),
            });
        }
    }
}

fn reservations_by_ip(config: &Config) -> BTreeMap<Ipv4Addr, &Reservation> {
    let mut map = BTreeMap::new();
    for subnet in &config.subnets {
        for res in &subnet.reservations {
            map.insert(res.ip, res);
        }
    }
    map
}

fn diff_reservations(ctx: &DiffCtx<'_>, entries: &mut Vec<DiffEntry>) {
    let src = reservations_by_ip(ctx.source);
    let tgt = reservations_by_ip(ctx.target);

    let mut src_bases: BTreeMap<String, OptionMap> = BTreeMap::new();
    let mut tgt_bases: BTreeMap<String, OptionMap> = BTreeMap::new();
    let mut scenario_cache: BTreeMap<String, Vec<ClientScenario>> = BTreeMap::new();
    let mut src_overlay_cache: BTreeMap<String, BTreeMap<String, ScenarioOverlay>> = BTreeMap::new();
    let mut tgt_overlay_cache: BTreeMap<String, BTreeMap<String, ScenarioOverlay>> = BTreeMap::new();

    for (ip, r) in &src {
        let ip_str = ip.to_string();
        match tgt.get(ip) {
            None => {
                let parent_tgt = subnet_for_ip(ctx.target, *ip).and_then(|sn| loc_of(&sn.source));
                let parent_key = subnet_for_ip(ctx.source, *ip).map(|sn| sn.network.to_string());
                entries.push(DiffEntry::MissingInTarget {
                    entity: reservation_entity(&ip_str, parent_key.as_deref()),
                    detail: format!("reservation {ip} ({}) missing in target", r.mac),
                    locations: DiffLocations::both(loc_of(&r.source), parent_tgt),
                });
            }
            Some(t) => {
                if r.mac != t.mac {
                    let src_v = NormalizedValue::String(r.mac.clone());
                    let tgt_v = NormalizedValue::String(t.mac.clone());
                    let parent_key = subnet_for_ip(ctx.source, *ip).map(|sn| sn.network.to_string());
                    entries.push(DiffEntry::Changed {
                        entity: reservation_entity(&ip_str, parent_key.as_deref()),
                        field: "mac".to_string(),
                        detail: format!("mac: {src_v:?} -> {tgt_v:?}"),
                        values: ChangedValues {
                            source: src_v,
                            target: tgt_v,
                        },
                        locations: DiffLocations::both(loc_of(&r.source), loc_of(&t.source)),
                    });
                }
                let src_sn = subnet_for_ip(ctx.source, *ip)
                    .expect("reservation IP must belong to a subnet");
                let tgt_sn = subnet_for_ip(ctx.target, *ip)
                    .expect("reservation IP must belong to a subnet");
                let cidr = src_sn.network.to_string();
                if !src_bases.contains_key(&cidr) {
                    src_bases.insert(cidr.clone(), static_subnet_base(ctx.source, src_sn));
                    tgt_bases.insert(cidr.clone(), static_subnet_base(ctx.target, tgt_sn));
                    let scenarios = scenarios_for_subnet_pair(
                        ctx.source,
                        src_sn,
                        &ctx.source_index,
                        ctx.target,
                        tgt_sn,
                        &ctx.target_index,
                    );
                    src_overlay_cache.insert(
                        cidr.clone(),
                        build_overlays(ctx.source, src_sn, &ctx.source_index, &scenarios),
                    );
                    tgt_overlay_cache.insert(
                        cidr.clone(),
                        build_overlays(ctx.target, tgt_sn, &ctx.target_index, &scenarios),
                    );
                    scenario_cache.insert(cidr.clone(), scenarios);
                }
                let src_base = &src_bases[&cidr];
                let tgt_base = &tgt_bases[&cidr];
                let scenarios = &scenario_cache[&cidr];
                let src_overlays = &src_overlay_cache[&cidr];
                let tgt_overlays = &tgt_overlay_cache[&cidr];

                diff_entity_scenarios(
                    ctx,
                    src_base,
                    tgt_base,
                    src_overlays,
                    tgt_overlays,
                    pool_for_ip(src_sn, *ip),
                    pool_for_ip(tgt_sn, *ip),
                    Some(r),
                    Some(t),
                    scenarios,
                    &format!("reservation:{ip}"),
                    loc_of(&r.source),
                    loc_of(&t.source),
                    entries,
                );
            }
        }
    }
    for (ip, r) in &tgt {
        if !src.contains_key(ip) {
            let parent_src = subnet_for_ip(ctx.source, *ip).and_then(|sn| loc_of(&sn.source));
            let parent_key = subnet_for_ip(ctx.target, *ip).map(|sn| sn.network.to_string());
            entries.push(DiffEntry::ExtraInTarget {
                entity: reservation_entity(&ip.to_string(), parent_key.as_deref()),
                detail: format!("reservation {ip} ({}) extra in target", r.mac),
                locations: DiffLocations::both(parent_src, loc_of(&r.source)),
            });
        }
    }
}

/// Compare baseline + VCI scenarios; skip VCI diffs identical to baseline on both sides.
fn diff_entity_scenarios(
    ctx: &DiffCtx<'_>,
    src_base: &OptionMap,
    tgt_base: &OptionMap,
    src_overlays: &BTreeMap<String, ScenarioOverlay>,
    tgt_overlays: &BTreeMap<String, ScenarioOverlay>,
    src_pool: Option<&Pool>,
    tgt_pool: Option<&Pool>,
    src_res: Option<&Reservation>,
    tgt_res: Option<&Reservation>,
    scenarios: &[ClientScenario],
    scope_prefix: &str,
    src_loc: Option<LocationRef>,
    tgt_loc: Option<LocationRef>,
    entries: &mut Vec<DiffEntry>,
) {
    let baseline_key = scenario_cache_key(&ClientScenario::baseline());
    let src_baseline = effective_client_options_overlay(
        src_base,
        src_pool,
        src_res,
        &src_overlays[&baseline_key],
    );
    let tgt_baseline = effective_client_options_overlay(
        tgt_base,
        tgt_pool,
        tgt_res,
        &tgt_overlays[&baseline_key],
    );
    diff_options(
        scope_prefix,
        &src_baseline,
        &tgt_baseline,
        &ctx.labels,
        src_loc.clone(),
        tgt_loc.clone(),
        entries,
    );

    for scenario in scenarios {
        if scenario.vendor_class.is_none() {
            continue;
        }
        let key = scenario_cache_key(scenario);
        let src_eff = effective_client_options_overlay(
            src_base,
            src_pool,
            src_res,
            &src_overlays[&key],
        );
        let tgt_eff = effective_client_options_overlay(
            tgt_base,
            tgt_pool,
            tgt_res,
            &tgt_overlays[&key],
        );
        if src_eff == src_baseline && tgt_eff == tgt_baseline {
            continue;
        }
        diff_scenario_options(
            &format!("{scope_prefix}{}", scenario.scope_suffix()),
            &src_eff,
            &tgt_eff,
            &src_baseline,
            &tgt_baseline,
            &ctx.labels,
            src_loc.clone(),
            tgt_loc.clone(),
            entries,
        );
    }
}

/// Diff only options this scenario changed relative to each side's baseline.
fn diff_scenario_options(
    scope: &str,
    src_eff: &OptionMap,
    tgt_eff: &OptionMap,
    src_baseline: &OptionMap,
    tgt_baseline: &OptionMap,
    labels: &OptionLabeler,
    src_loc: Option<LocationRef>,
    tgt_loc: Option<LocationRef>,
    entries: &mut Vec<DiffEntry>,
) {
    let mut keys = BTreeMap::new();
    for k in src_eff.keys().chain(tgt_eff.keys()) {
        if src_eff.get(k) != src_baseline.get(k) || tgt_eff.get(k) != tgt_baseline.get(k) {
            keys.insert(k.clone(), ());
        }
    }
    if keys.is_empty() {
        return;
    }
    let src_filtered: OptionMap = keys
        .keys()
        .filter_map(|k| src_eff.get(k).map(|v| (k.clone(), v.clone())))
        .collect();
    let tgt_filtered: OptionMap = keys
        .keys()
        .filter_map(|k| tgt_eff.get(k).map(|v| (k.clone(), v.clone())))
        .collect();
    diff_options(
        scope,
        &src_filtered,
        &tgt_filtered,
        labels,
        src_loc,
        tgt_loc,
        entries,
    );
}

fn diff_global_filters(source: &[Filter], target: &[Filter], entries: &mut Vec<DiffEntry>) {
    let src: BTreeMap<String, &Filter> = source.iter().map(|f| (filter_key(f), f)).collect();
    let tgt: BTreeMap<String, &Filter> = target.iter().map(|f| (filter_key(f), f)).collect();
    for (k, f) in &src {
        if !tgt.contains_key(k) {
            entries.push(DiffEntry::MissingInTarget {
                entity: entity("filter", &format!("global:{k}")),
                detail: format!("global filter {k} missing in target"),
                locations: DiffLocations::source_only(loc_of(&f.source)),
            });
        } else {
            let t = tgt[k];
            if f.vendor_option_space != t.vendor_option_space {
                let src_v = opt_string_value(&f.vendor_option_space);
                let tgt_v = opt_string_value(&t.vendor_option_space);
                entries.push(DiffEntry::Changed {
                    entity: entity("filter", &format!("global:{k}")),
                    field: "vendor_option_space".to_string(),
                    detail: format!("vendor_option_space: {src_v:?} -> {tgt_v:?}"),
                    values: ChangedValues {
                        source: src_v,
                        target: tgt_v,
                    },
                    locations: DiffLocations::both(loc_of(&f.source), loc_of(&t.source)),
                });
            }
        }
    }
    for k in tgt.keys() {
        if !src.contains_key(k) {
            let f = tgt[k];
            entries.push(DiffEntry::ExtraInTarget {
                entity: entity("filter", &format!("global:{k}")),
                detail: format!("global filter {k} extra in target"),
                locations: DiffLocations::target_only(loc_of(&f.source)),
            });
        }
    }
}

fn diff_options(
    scope: &str,
    source: &OptionMap,
    target: &OptionMap,
    labels: &OptionLabeler,
    src_affected: Option<LocationRef>,
    tgt_affected: Option<LocationRef>,
    entries: &mut Vec<DiffEntry>,
) {
    for (k, v) in source {
        if k.is_unresolved() {
            entries.push(DiffEntry::Unmapped {
                entity: option_entity(scope, &format!("{k:?}"), Some(v), None, &src_affected, &tgt_affected),
                option_key: k.clone(),
                detail: "unmapped option in source".to_string(),
                locations: DiffLocations::source_only_side(side_for_option(
                    src_affected.clone(),
                    Some(v),
                )),
            });
            continue;
        }
        let opt = labels.format_key(k);
        match target.get(k) {
            None => entries.push(DiffEntry::MissingInTarget {
                entity: option_entity(scope, &opt, Some(v), None, &src_affected, &tgt_affected),
                detail: format!("option {opt} = {:?} missing in target", v.value),
                locations: DiffLocations::source_only_side(side_for_option(
                    src_affected.clone(),
                    Some(v),
                )),
            }),
            Some(tv) if tv != v => entries.push(DiffEntry::Changed {
                entity: option_entity(scope, &opt, Some(v), Some(tv), &src_affected, &tgt_affected),
                field: opt.clone(),
                detail: format!("{opt}: {:?} -> {:?}", v.value, tv.value),
                values: ChangedValues {
                    source: v.value.clone(),
                    target: tv.value.clone(),
                },
                locations: DiffLocations::both_sides(
                    side_for_option(src_affected.clone(), Some(v)),
                    side_for_option(tgt_affected.clone(), Some(tv)),
                ),
            }),
            _ => {}
        }
    }
    for (k, v) in target {
        if k.is_unresolved() {
            continue;
        }
        if !source.contains_key(k) {
            let opt = labels.format_key(k);
            entries.push(DiffEntry::ExtraInTarget {
                entity: option_entity(scope, &opt, None, Some(v), &src_affected, &tgt_affected),
                detail: format!("option {opt} = {:?} extra in target", v.value),
                locations: DiffLocations::target_only_side(side_for_option(
                    tgt_affected.clone(),
                    Some(v),
                )),
            });
        }
    }
}

fn side_for_option(
    affected: Option<LocationRef>,
    bound: Option<&BoundOption>,
) -> Option<SideLocations> {
    let declaration = bound
        .and_then(|b| b.source.as_ref())
        .map(LocationRef::from_source_ref);
    match (affected, declaration) {
        (Some(aff), decl) => Some(SideLocations::with_declaration(aff, decl)),
        (None, Some(decl)) => Some(SideLocations::affected_only(decl)),
        (None, None) => None,
    }
}

fn option_entity(
    scope: &str,
    opt: &str,
    src: Option<&BoundOption>,
    tgt: Option<&BoundOption>,
    src_affected: &Option<LocationRef>,
    tgt_affected: &Option<LocationRef>,
) -> EntityRef {
    let mut entity = entity("option", &format!("{scope}:{opt}"));
    let declared_in =
        declared_in_for(src, src_affected).or_else(|| declared_in_for(tgt, tgt_affected));
    let declaration_key = declaration_key_for(src, src_affected, scope)
        .or_else(|| declaration_key_for(tgt, tgt_affected, scope));
    let option_id = option_id_from_label(opt);
    if let Some(display) = entity.display.as_mut() {
        display.declared_in = declared_in;
        display.declaration_key = declaration_key;
        display.option_id = option_id;
    }
    entity
}

fn option_id_from_label(opt: &str) -> Option<String> {
    let trimmed = opt.trim();
    if trimmed.is_empty() {
        return None;
    }
    let without_name = match trimmed.rfind(" (") {
        Some(i) if trimmed.ends_with(')') => &trimmed[..i],
        _ => trimmed,
    };
    let id = without_name.trim();
    if id.contains(':') {
        Some(id.to_string())
    } else {
        None
    }
}

fn scope_without_vci(scope: &str) -> &str {
    scope.split(":vci=").next().unwrap_or(scope)
}

fn cidr_from_affected_scope(scope: &str) -> Option<String> {
    let base = scope_without_vci(scope);
    if let Some(rest) = base.strip_prefix("pool:") {
        return rest.split_once(':').map(|(cidr, _)| cidr.to_string());
    }
    if base.starts_with("reservation:") || base == "global" || base.is_empty() {
        return None;
    }
    if base.contains('/') {
        return Some(base.to_string());
    }
    None
}

fn affected_as_declaration_key(scope: &str) -> String {
    let base = scope_without_vci(scope);
    if base.is_empty() {
        "global".to_string()
    } else {
        base.to_string()
    }
}

fn declaration_key_for(
    bound: Option<&BoundOption>,
    affected: &Option<LocationRef>,
    scope: &str,
) -> Option<String> {
    let bound = bound?;
    let is_local = match (bound.source.as_ref(), affected.as_ref()) {
        (Some(decl), Some(aff)) => decl.file == aff.file && decl.line == aff.line,
        (None, _) => true,
        (Some(_), None) => false,
    };
    if is_local {
        return Some(affected_as_declaration_key(scope));
    }

    let label = bound.declared_in.as_deref().unwrap_or("");
    if label.is_empty() {
        if let Some(decl) = bound.source.as_ref() {
            return Some(format!("{}:{}", decl.file, decl.line));
        }
        return Some(affected_as_declaration_key(scope));
    }

    let lower = label.to_ascii_lowercase();
    if lower == "global" || lower.starts_with("global ") {
        return Some("global".into());
    }
    if lower == "subnet" || lower.starts_with("subnet ") {
        return cidr_from_affected_scope(scope).map(|c| format!("subnet:{c}"));
    }
    if lower == "pool" || lower.starts_with("pool ") {
        let base = scope_without_vci(scope);
        if base.starts_with("pool:") {
            return Some(base.to_string());
        }
        return Some(format!("decl:{label}"));
    }
    // Class / if and other labels: group by subnet when peelable.
    if let Some(cidr) = cidr_from_affected_scope(scope) {
        return Some(format!("subnet:{cidr}"));
    }
    if scope_without_vci(scope) == "global" {
        return Some("global".into());
    }
    Some(format!("decl:{label}"))
}

fn declared_in_for(bound: Option<&BoundOption>, affected: &Option<LocationRef>) -> Option<String> {
    let bound = bound?;
    let decl = bound.source.as_ref()?;
    let Some(aff) = affected else {
        return bound.declared_in.clone();
    };
    if decl.file != aff.file || decl.line != aff.line {
        return bound.declared_in.clone().or_else(|| {
            Some(format!("{}:{}", decl.file, decl.line))
        });
    }
    None
}

fn opt_string_value(v: &Option<String>) -> NormalizedValue {
    match v {
        Some(s) => NormalizedValue::String(s.clone()),
        None => NormalizedValue::Opaque("null".to_string()),
    }
}

fn pool_key(p: &Pool) -> String {
    format!("{}-{}", p.start, p.end)
}

fn filter_key(f: &Filter) -> String {
    format!("{}:{}", f.name, f.match_expr.stable_key())
}

fn entity(kind: &str, key: &str) -> EntityRef {
    let display = Some(build_entity_display(kind, key));
    EntityRef {
        kind: kind.to_string(),
        key: key.to_string(),
        display,
    }
}

fn reservation_entity(ip: &str, parent_key: Option<&str>) -> EntityRef {
    let mut display = build_entity_display("reservation", ip);
    if let Some(cidr) = parent_key {
        display.parent = Some(format!("Subnet {cidr}"));
        display.parent_key = Some(cidr.to_string());
        display.summary = format!("Reservation {ip} in subnet {cidr}");
    }
    EntityRef {
        kind: "reservation".into(),
        key: ip.to_string(),
        display: Some(display),
    }
}

fn build_entity_display(kind: &str, key: &str) -> EntityDisplay {
    match kind {
        "subnet" => EntityDisplay {
            object_type: "Subnet".into(),
            name: key.to_string(),
            parent: None,
            parent_key: None,
            vci: None,
            declared_in: None,
            declaration_key: None,
            option_id: None,
            summary: format!("Subnet {key}"),
        },
        "pool" => {
            // key = "{cidr}:{start}-{end}"
            if let Some((cidr, range)) = key.split_once(':') {
                EntityDisplay {
                    object_type: "Pool".into(),
                    name: range.to_string(),
                    parent: Some(format!("Subnet {cidr}")),
                    parent_key: Some(cidr.to_string()),
                    vci: None,
                    declared_in: None,
                    declaration_key: None,
                    option_id: None,
                    summary: format!("Pool {range} in subnet {cidr}"),
                }
            } else {
                EntityDisplay {
                    object_type: "Pool".into(),
                    name: key.to_string(),
                    parent: None,
                    parent_key: None,
                    vci: None,
                    declared_in: None,
                    declaration_key: None,
                    option_id: None,
                    summary: format!("Pool {key}"),
                }
            }
        }
        "reservation" => EntityDisplay {
            object_type: "Reservation".into(),
            name: key.to_string(),
            parent: None,
            parent_key: None,
            vci: None,
            declared_in: None,
            declaration_key: None,
            option_id: None,
            summary: format!("Reservation {key}"),
        },
        "filter" => parse_filter_display(key),
        "option" => parse_option_display(key),
        other => EntityDisplay {
            object_type: title_case(other),
            name: key.to_string(),
            parent: None,
            parent_key: None,
            vci: None,
            declared_in: None,
            declaration_key: None,
            option_id: None,
            summary: format!("{other}:{key}"),
        },
    }
}

fn title_case(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        None => String::new(),
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
    }
}

fn parse_filter_display(key: &str) -> EntityDisplay {
    // global:{name}:{match} or {cidr}:{name}:{match}
    let parts: Vec<&str> = key.splitn(3, ':').collect();
    if parts.len() >= 2 {
        let scope = parts[0];
        let name = parts[1].to_string();
        let parent = if scope == "global" {
            Some("Global".to_string())
        } else {
            Some(format!("Subnet {scope}"))
        };
        let summary = match &parent {
            Some(p) => format!("Filter {name} ({p})"),
            None => format!("Filter {name}"),
        };
        EntityDisplay {
            object_type: "Filter".into(),
            name,
            parent,
            parent_key: Some(scope.to_string()),
            vci: None,
            declared_in: None,
            declaration_key: None,
            option_id: None,
            summary,
        }
    } else {
        EntityDisplay {
            object_type: "Filter".into(),
            name: key.to_string(),
            parent: None,
            parent_key: None,
            vci: None,
            declared_in: None,
            declaration_key: None,
            option_id: None,
            summary: format!("Filter {key}"),
        }
    }
}

fn parse_option_display(key: &str) -> EntityDisplay {
    // Examples:
    //   global:dhcp:15 (domain-name)
    //   pool:10.0.0.0/24:10.0.0.1-10.0.0.10:dhcp:15 (domain-name)
    //   pool:10.0.0.0/24:10.0.0.1-10.0.0.10:vci=PXEClient:bootp:0 (next-server)
    //   reservation:10.0.0.5:vci=PXEClient:dhcp:67 (dhcp-bootfile-name)
    let (scope_and_maybe_vci, option_name, vci) = split_option_key(key);
    let (object_type, name, parent, parent_key, summary_base) =
        describe_option_scope(&scope_and_maybe_vci, &option_name);

    let summary = match &vci {
        Some(v) => format!("{summary_base} — affects clients with VCI {v}"),
        None => summary_base,
    };

    EntityDisplay {
        object_type,
        name,
        parent,
        parent_key,
        vci,
        declared_in: None,
        // Fallback without BoundOption: treat affected scope as declaration site.
        declaration_key: Some(affected_as_declaration_key(&scope_and_maybe_vci)),
        option_id: option_id_from_label(&option_name),
        summary,
    }
}

fn split_option_key(key: &str) -> (String, String, Option<String>) {
    // Prefer explicit VCI marker. Peel option suffix from the right so VCIs that
    // contain colons (e.g. PXEClient:Arch:00000) stay intact.
    if let Some(idx) = key.find(":vci=") {
        let before = &key[..idx];
        let after = &key[idx + ":vci=".len()..];
        if let Some((vci, option)) = peel_option_suffix(after) {
            return (before.to_string(), option, Some(vci));
        }
        return (before.to_string(), String::new(), Some(after.to_string()));
    }

    // Peel trailing "space:code (name)" or "space:code"
    if let Some((scope, option)) = peel_option_suffix(key) {
        return (scope, option, None);
    }
    (String::new(), key.to_string(), None)
}

fn peel_option_suffix(key: &str) -> Option<(String, String)> {
    // Match ...:space:digits optional (label)
    let bytes = key.as_bytes();
    // Find last " (" for label, else end
    let label_start = key.rfind(" (");
    let code_region_end = label_start.unwrap_or(key.len());
    // Walk back over digits for code
    let mut i = code_region_end;
    if i == 0 {
        return None;
    }
    while i > 0 && bytes[i - 1].is_ascii_digit() {
        i -= 1;
    }
    if i == code_region_end || i == 0 || bytes[i - 1] != b':' {
        // maybe no numeric code — treat whole key as option if it contains a colon at end segment
        if let Some((scope, option)) = key.rsplit_once(':') {
            // Avoid splitting IPv4-looking scopes incorrectly: require option to look like name
            if option.contains('(') || !option.contains('.') {
                return Some((scope.to_string(), option.to_string()));
            }
        }
        return None;
    }
    // i-1 is colon before code; walk back for space name
    let code_colon = i - 1;
    let mut j = code_colon;
    while j > 0 && bytes[j - 1] != b':' {
        j -= 1;
    }
    if j == 0 {
        return None;
    }
    // j-1 is colon before space, or start
    let space_start = j;
    if space_start == 0 {
        return None;
    }
    let scope = key[..space_start - 1].to_string(); // drop the colon before space
    let option = key[space_start..].to_string();
    if scope.is_empty() || option.is_empty() {
        return None;
    }
    Some((scope, option))
}

fn describe_option_scope(
    scope: &str,
    option_name: &str,
) -> (String, String, Option<String>, Option<String>, String) {
    let name = if option_name.is_empty() {
        "option".to_string()
    } else {
        option_name.to_string()
    };

    if scope.is_empty() || scope == "global" {
        return (
            "Option".into(),
            name.clone(),
            Some("Global".into()),
            Some("global".into()),
            format!("Option {name} (global)"),
        );
    }

    if let Some(rest) = scope.strip_prefix("pool:") {
        // rest = "{cidr}:{range}"
        if let Some((cidr, range)) = rest.split_once(':') {
            let parent = format!("Pool {range} in subnet {cidr}");
            return (
                "Option".into(),
                name.clone(),
                Some(parent.clone()),
                Some(scope.to_string()),
                format!("Option {name} on {parent}"),
            );
        }
        let parent = format!("Pool {rest}");
        return (
            "Option".into(),
            name.clone(),
            Some(parent.clone()),
            Some(scope.to_string()),
            format!("Option {name} on {parent}"),
        );
    }

    if let Some(ip) = scope.strip_prefix("reservation:") {
        let parent = format!("Reservation {ip}");
        return (
            "Option".into(),
            name.clone(),
            Some(parent.clone()),
            Some(scope.to_string()),
            format!("Option {name} on {parent}"),
        );
    }

    (
        "Option".into(),
        name.clone(),
        Some(scope.to_string()),
        Some(scope.to_string()),
        format!("Option {name} ({scope})"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::NormalizedValue;

    #[test]
    fn category_serde_uses_snake_case_and_aliases() {
        let entry = DiffEntry::MissingInTarget {
            entity: entity("subnet", "10.0.0.0/24"),
            detail: "missing".into(),
            locations: DiffLocations::default(),
        };
        let json = serde_json::to_string(&entry).unwrap();
        assert!(json.contains("\"category\":\"missing\""));

        let legacy = r#"{"category":"MissingInTarget","entity":{"kind":"subnet","key":"10.0.0.0/24"},"detail":"missing"}"#;
        let parsed: DiffEntry = serde_json::from_str(legacy).unwrap();
        assert!(matches!(parsed, DiffEntry::MissingInTarget { .. }));
    }

    #[test]
    fn changed_values_serialize_as_typed_normalized_value() {
        let entry = DiffEntry::Changed {
            entity: entity("option", "global:dhcp:15"),
            field: "dhcp:15 (domain-name)".into(),
            detail: "changed".into(),
            values: ChangedValues {
                source: NormalizedValue::String("a.example".into()),
                target: NormalizedValue::String("b.example".into()),
            },
            locations: DiffLocations::default(),
        };
        let v: serde_json::Value = serde_json::to_value(&entry).unwrap();
        assert_eq!(v["category"], "changed");
        assert_eq!(v["values"]["source"]["type"], "String");
        assert_eq!(v["values"]["source"]["value"], "a.example");
    }

    #[test]
    fn counts_and_envelope_fields() {
        let report = DiffReport::from_entries(vec![
            DiffEntry::MissingInTarget {
                entity: entity("subnet", "10.0.0.0/24"),
                detail: "m".into(),
                locations: DiffLocations::default(),
            },
            DiffEntry::Changed {
                entity: entity("option", "x"),
                field: "f".into(),
                detail: "d".into(),
                values: ChangedValues {
                    source: NormalizedValue::Int(1),
                    target: NormalizedValue::Int(2),
                },
                locations: DiffLocations::default(),
            },
        ])
        .with_file_meta(
            FileMeta {
                path: "a.conf".into(),
                vendor: "bluecat".into(),
            },
            FileMeta {
                path: "b.conf".into(),
                vendor: "infoblox".into(),
            },
        );
        assert_eq!(report.version, 1);
        assert_eq!(report.counts.missing, 1);
        assert_eq!(report.counts.changed, 1);
        assert_eq!(report.counts.total, 2);
        assert_eq!(report.source.as_ref().unwrap().vendor, "bluecat");
    }

    #[test]
    fn entity_display_splits_pool_option_with_vci() {
        let e = entity(
            "option",
            "pool:10.162.40.0/24:10.162.40.250-10.162.40.251:vci=PXEClient:bootp:0 (next-server)",
        );
        let d = e.display.expect("display");
        assert_eq!(d.object_type, "Option");
        assert_eq!(d.name, "bootp:0 (next-server)");
        assert_eq!(
            d.parent.as_deref(),
            Some("Pool 10.162.40.250-10.162.40.251 in subnet 10.162.40.0/24")
        );
        assert_eq!(
            d.parent_key.as_deref(),
            Some("pool:10.162.40.0/24:10.162.40.250-10.162.40.251")
        );
        assert_eq!(d.vci.as_deref(), Some("PXEClient"));
        assert_eq!(d.option_id.as_deref(), Some("bootp:0"));
        assert!(d.summary.contains("affects clients with VCI PXEClient"));
    }

    #[test]
    fn entity_display_keeps_colonful_arch_vci_intact() {
        let e = entity(
            "option",
            "pool:10.64.112.0/23:10.64.112.2-10.64.113.249:vci=PXEClient:Arch:00000:isc:0 (default-lease-time)",
        );
        let d = e.display.expect("display");
        assert_eq!(d.name, "isc:0 (default-lease-time)");
        assert_eq!(d.vci.as_deref(), Some("PXEClient:Arch:00000"));
        assert_eq!(d.option_id.as_deref(), Some("isc:0"));
        assert!(d.summary.contains("affects clients with VCI PXEClient:Arch:00000"));
    }

    #[test]
    fn option_entity_sets_declaration_key_and_option_id() {
        use crate::model::{BoundOption, SourceRef};

        let subnet_decl = BoundOption::new(
            NormalizedValue::String("a.example".into()),
            Some(SourceRef::new("test", "a.conf", 2)),
        )
        .with_declared_in("Subnet");
        let pool_aff = Some(LocationRef::from_source_ref(&SourceRef::new(
            "test", "a.conf", 40,
        )));
        let e = option_entity(
            "pool:10.10.11.0/25:10.10.11.79-10.10.11.82",
            "dhcp:15 (domain-name)",
            Some(&subnet_decl),
            None,
            &pool_aff,
            &None,
        );
        let d = e.display.unwrap();
        assert_eq!(d.option_id.as_deref(), Some("dhcp:15"));
        assert_eq!(d.declaration_key.as_deref(), Some("subnet:10.10.11.0/25"));
        assert_eq!(d.declared_in.as_deref(), Some("Subnet"));

        let local = BoundOption::new(
            NormalizedValue::String("b.example".into()),
            Some(SourceRef::new("test", "a.conf", 40)),
        )
        .with_declared_in("Pool");
        let e = option_entity(
            "pool:10.10.11.0/25:10.10.11.79-10.10.11.82",
            "dhcp:15 (domain-name)",
            Some(&local),
            None,
            &pool_aff,
            &None,
        );
        let d = e.display.unwrap();
        assert_eq!(
            d.declaration_key.as_deref(),
            Some("pool:10.10.11.0/25:10.10.11.79-10.10.11.82")
        );
    }

    #[test]
    fn entity_display_pool_and_reservation() {
        let pool = entity("pool", "10.0.80.0/24:10.0.80.106-10.0.80.113");
        let d = pool.display.unwrap();
        assert_eq!(d.object_type, "Pool");
        assert_eq!(d.name, "10.0.80.106-10.0.80.113");
        assert_eq!(d.parent.as_deref(), Some("Subnet 10.0.80.0/24"));
        assert_eq!(d.parent_key.as_deref(), Some("10.0.80.0/24"));

        let res = entity("reservation", "10.0.80.49");
        let d = res.display.unwrap();
        assert_eq!(d.object_type, "Reservation");
        assert_eq!(d.name, "10.0.80.49");
        assert!(d.parent_key.is_none());

        let res = reservation_entity("10.0.80.49", Some("10.0.80.0/24"));
        let d = res.display.unwrap();
        assert_eq!(d.parent.as_deref(), Some("Subnet 10.0.80.0/24"));
        assert_eq!(d.parent_key.as_deref(), Some("10.0.80.0/24"));
    }

    fn subnet_with(
        cidr: &str,
        line: u32,
        pools: Vec<Pool>,
        reservations: Vec<Reservation>,
    ) -> Subnet {
        Subnet {
            network: cidr.parse().unwrap(),
            shared_network: None,
            options: BTreeMap::new(),
            pools,
            reservations,
            filters: Vec::new(),
            extensions: BTreeMap::new(),
            source: Some(SourceRef::new("test", "test.conf", line)),
        }
    }

    fn pool_at(start: &str, end: &str, line: u32) -> Pool {
        Pool {
            start: start.parse().unwrap(),
            end: end.parse().unwrap(),
            options: BTreeMap::new(),
            extensions: BTreeMap::new(),
            source: Some(SourceRef::new("test", "test.conf", line)),
        }
    }

    fn reservation_at(ip: &str, mac: &str, line: u32) -> Reservation {
        Reservation {
            mac: mac.into(),
            ip: ip.parse().unwrap(),
            options: BTreeMap::new(),
            extensions: BTreeMap::new(),
            source: Some(SourceRef::new("test", "test.conf", line)),
        }
    }

    #[test]
    fn missing_pool_includes_target_parent_subnet_location() {
        let source = Config {
            subnets: vec![subnet_with(
                "10.0.0.0/24",
                1,
                vec![pool_at("10.0.0.10", "10.0.0.20", 5)],
                vec![],
            )],
            ..Config::default()
        };
        let target = Config {
            subnets: vec![subnet_with("10.0.0.0/24", 10, vec![], vec![])],
            ..Config::default()
        };
        let report = diff_configs(&source, &target);
        let entry = report
            .entries
            .iter()
            .find(|e| {
                matches!(
                    e,
                    DiffEntry::MissingInTarget { entity, .. } if entity.kind == "pool"
                )
            })
            .expect("missing pool entry");
        match entry {
            DiffEntry::MissingInTarget { locations, .. } => {
                let src = locations.source.as_ref().expect("source loc");
                let tgt = locations.target.as_ref().expect("target parent loc");
                assert_eq!(src.affected.line, 5);
                assert_eq!(tgt.affected.line, 10);
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn missing_reservation_includes_target_parent_when_subnet_exists() {
        let source = Config {
            subnets: vec![subnet_with(
                "10.0.0.0/24",
                1,
                vec![],
                vec![reservation_at("10.0.0.50", "aa:bb:cc:dd:ee:01", 8)],
            )],
            ..Config::default()
        };
        let target = Config {
            subnets: vec![subnet_with("10.0.0.0/24", 20, vec![], vec![])],
            ..Config::default()
        };
        let report = diff_configs(&source, &target);
        let entry = report
            .entries
            .iter()
            .find(|e| {
                matches!(
                    e,
                    DiffEntry::MissingInTarget { entity, .. } if entity.kind == "reservation"
                )
            })
            .expect("missing reservation");
        match entry {
            DiffEntry::MissingInTarget {
                entity, locations, ..
            } => {
                assert_eq!(
                    entity.display.as_ref().and_then(|d| d.parent_key.as_deref()),
                    Some("10.0.0.0/24")
                );
                assert_eq!(
                    entity.display.as_ref().and_then(|d| d.parent.as_deref()),
                    Some("Subnet 10.0.0.0/24")
                );
                assert_eq!(
                    locations.source.as_ref().unwrap().affected.line,
                    8
                );
                assert_eq!(
                    locations.target.as_ref().unwrap().affected.line,
                    20
                );
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn missing_reservation_omits_target_when_subnet_absent() {
        let source = Config {
            subnets: vec![subnet_with(
                "10.0.0.0/24",
                1,
                vec![],
                vec![reservation_at("10.0.0.50", "aa:bb:cc:dd:ee:01", 8)],
            )],
            ..Config::default()
        };
        let target = Config::default();
        let report = diff_configs(&source, &target);
        let entry = report
            .entries
            .iter()
            .find(|e| {
                matches!(
                    e,
                    DiffEntry::MissingInTarget { entity, .. } if entity.kind == "reservation"
                )
            })
            .expect("missing reservation");
        match entry {
            DiffEntry::MissingInTarget { locations, .. } => {
                assert!(locations.source.is_some());
                assert!(locations.target.is_none());
            }
            _ => unreachable!(),
        }
    }
}
