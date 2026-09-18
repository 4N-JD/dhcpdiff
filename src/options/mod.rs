use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::model::{Config, NormalizedValue, OptionDef, OptionKey};

pub mod builtin;
pub mod inherit;
pub mod labels;
pub mod mappings;
pub mod scenario;

#[derive(Debug, Clone, Default)]
pub struct ResolverOptions {
    /// Override `ignore_subnet_mask` from the mapping file. Defaults to true when unset.
    pub ignore_subnet_mask: Option<bool>,
}

#[derive(Debug, Clone, Default)]
pub struct OptionResolver {
    builtin_names: BTreeMap<String, u16>,
    aliases: BTreeMap<String, OptionKey>,
    equivalences: Vec<Equivalence>,
    ignore: BTreeSet<OptionKey>,
}

fn subnet_mask_key() -> OptionKey {
    OptionKey::dhcp(1)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Equivalence {
    pub source: OptionKeyRef,
    pub target: OptionKeyRef,
    pub confirmed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct OptionKeyRef {
    pub space: String,
    pub code: u16,
}

impl From<&OptionKey> for OptionKeyRef {
    fn from(k: &OptionKey) -> Self {
        Self {
            space: k.space.clone(),
            code: k.code,
        }
    }
}

#[derive(Debug, Clone)]
pub struct UnknownOption {
    pub raw_name: String,
    pub key: OptionKey,
    pub usage_count: usize,
    pub example_value: Option<NormalizedValue>,
    pub sites: Vec<String>,
}

impl OptionResolver {
    pub fn load(
        mapping_path: Option<&std::path::Path>,
        options: ResolverOptions,
    ) -> anyhow::Result<Self> {
        let mut resolver = Self {
            builtin_names: builtin::load_builtin_names(),
            ..Default::default()
        };
        let mut ignore_subnet_mask = options.ignore_subnet_mask;
        if let Some(path) = mapping_path {
            if path.exists() {
                let user = mappings::load_user_mappings(path)?;
                if ignore_subnet_mask.is_none() {
                    ignore_subnet_mask = user.ignore_subnet_mask;
                }
                resolver.apply_user_mappings(user);
            }
        }
        resolver.set_ignore_subnet_mask(ignore_subnet_mask.unwrap_or(true));
        Ok(resolver)
    }

    pub fn set_ignore_subnet_mask(&mut self, ignore: bool) {
        let key = subnet_mask_key();
        if ignore {
            self.ignore.insert(key);
        } else {
            self.ignore.remove(&key);
        }
    }

    pub fn apply_user_mappings(&mut self, user: mappings::UserMappings) {
        for alias in user.aliases {
            self.aliases.insert(
                alias.source_name,
                OptionKey::qualified(alias.canonical.space, alias.canonical.code),
            );
        }
        for eq in user.equivalences {
            self.equivalences.push(Equivalence {
                source: eq.source,
                target: eq.target,
                confirmed: eq.confirmed,
            });
        }
        for ig in user.ignore {
            self.ignore.insert(OptionKey::qualified(ig.space, ig.code));
        }
    }

    pub fn resolve_name(&self, name: &str, definitions: &BTreeMap<String, OptionDef>) -> OptionKey {
        if let Some(key) = self.aliases.get(name) {
            return key.clone();
        }
        if let Some(def) = definitions.get(name) {
            return OptionKey::qualified(&def.space, def.code);
        }
        if let Some(code) = self.builtin_names.get(name) {
            return OptionKey::dhcp(*code);
        }
        if name.chars().all(|c| c.is_ascii_digit()) {
            if let Ok(code) = name.parse::<u16>() {
                return OptionKey::dhcp(code);
            }
        }
        OptionKey::unresolved(name)
    }

    pub fn canonicalize_key(&self, key: &OptionKey) -> OptionKey {
        if self.ignore.contains(key) {
            return key.clone();
        }
        for eq in &self.equivalences {
            if eq.confirmed
                && eq.source.space == key.space
                && eq.source.code == key.code
            {
                return OptionKey::qualified(&eq.target.space, eq.target.code);
            }
        }
        key.clone()
    }

    /// Map a vendor option-space name via confirmed equivalences.
    ///
    /// When every confirmed equivalence for `space` shares the same target space,
    /// that target is returned (so `vendor-option-space` stays aligned with remapped
    /// option keys). Conflicting or missing mappings leave the name unchanged.
    pub fn canonicalize_space(&self, space: &str) -> String {
        let mut targets = BTreeSet::new();
        for eq in &self.equivalences {
            if eq.confirmed && eq.source.space == space {
                targets.insert(eq.target.space.clone());
            }
        }
        if targets.len() == 1 {
            targets.into_iter().next().unwrap()
        } else {
            space.to_string()
        }
    }

    fn rewrite_vendor_option_space(&self, space: &mut Option<String>) {
        if let Some(name) = space.as_mut() {
            *name = self.canonicalize_space(name);
        }
    }

    pub fn is_unknown(&self, key: &OptionKey) -> bool {
        key.is_unresolved()
    }

    pub fn is_ignored(&self, key: &OptionKey) -> bool {
        self.ignore.contains(key)
    }

    pub fn collect_unknowns(&self, config: &Config) -> Vec<UnknownOption> {
        let mut map: BTreeMap<OptionKey, UnknownOption> = BTreeMap::new();

        let mut record = |key: &OptionKey, site: &str, value: &NormalizedValue| {
            if self.is_ignored(key) || !self.is_unknown(key) {
                return;
            }
            let raw_name = key
                .unresolved_name()
                .map(str::to_string)
                .unwrap_or_else(|| format!("{}:{}", key.space, key.code));
            let entry = map.entry(key.clone()).or_insert_with(|| UnknownOption {
                raw_name,
                key: key.clone(),
                usage_count: 0,
                example_value: None,
                sites: Vec::new(),
            });
            entry.usage_count += 1;
            if entry.example_value.is_none() {
                entry.example_value = Some(value.clone());
            }
            if !entry.sites.contains(&site.to_string()) {
                entry.sites.push(site.to_string());
            }
        };

        for (k, v) in &config.global_options {
            record(k, "global", &v.value);
        }
        for filter in &config.global_filters {
            for (k, v) in &filter.options {
                record(k, &format!("filter:{}", filter.name), &v.value);
            }
        }
        for (i, rule) in config.conditional_rules.iter().enumerate() {
            for (k, v) in &rule.options {
                record(k, &format!("conditional:{i}"), &v.value);
            }
        }
        for subnet in &config.subnets {
            let sn = subnet.network.to_string();
            for (k, v) in &subnet.options {
                record(k, &format!("subnet:{sn}"), &v.value);
            }
            for pool in &subnet.pools {
                for (k, v) in &pool.options {
                    record(k, &format!("pool:{sn}:{}-{}", pool.start, pool.end), &v.value);
                }
            }
            for res in &subnet.reservations {
                for (k, v) in &res.options {
                    record(k, &format!("reservation:{sn}:{}", res.mac), &v.value);
                }
            }
            for filter in &subnet.filters {
                for (k, v) in &filter.options {
                    record(k, &format!("filter:{sn}:{}", filter.name), &v.value);
                }
            }
        }

        map.into_values().collect()
    }

    pub fn resolve_config(&self, config: &mut Config) {
        config.global_options = self.rewrite_map(std::mem::take(&mut config.global_options), &config.option_definitions);
        for filter in &mut config.global_filters {
            filter.options = self.rewrite_map(std::mem::take(&mut filter.options), &config.option_definitions);
            self.rewrite_vendor_option_space(&mut filter.vendor_option_space);
        }
        for rule in &mut config.conditional_rules {
            rule.options =
                self.rewrite_map(std::mem::take(&mut rule.options), &config.option_definitions);
            self.rewrite_vendor_option_space(&mut rule.vendor_option_space);
        }
        for shared in config.shared_networks.values_mut() {
            shared.options =
                self.rewrite_map(std::mem::take(&mut shared.options), &config.option_definitions);
        }
        for subnet in &mut config.subnets {
            subnet.options = self.rewrite_map(std::mem::take(&mut subnet.options), &config.option_definitions);
            for pool in &mut subnet.pools {
                pool.options = self.rewrite_map(std::mem::take(&mut pool.options), &config.option_definitions);
            }
            for res in &mut subnet.reservations {
                res.options = self.rewrite_map(std::mem::take(&mut res.options), &config.option_definitions);
            }
            for filter in &mut subnet.filters {
                filter.options =
                    self.rewrite_map(std::mem::take(&mut filter.options), &config.option_definitions);
                self.rewrite_vendor_option_space(&mut filter.vendor_option_space);
            }
        }
    }

    fn rewrite_map(
        &self,
        map: BTreeMap<OptionKey, crate::model::BoundOption>,
        definitions: &BTreeMap<String, OptionDef>,
    ) -> BTreeMap<OptionKey, crate::model::BoundOption> {
        let mut out = BTreeMap::new();
        for (k, v) in map {
            let mut key = if let Some(name) = k.unresolved_name() {
                if let Some(mapped) = self.aliases.get(name) {
                    mapped.clone()
                } else if let Some(def) = definitions.get(name) {
                    OptionKey::qualified(&def.space, def.code)
                } else if let Some(code) = self.builtin_names.get(name) {
                    OptionKey::dhcp(*code)
                } else {
                    k
                }
            } else if k.is_unresolved() {
                // Legacy bare unknown:0 — leave as-is
                k
            } else {
                k
            };
            key = self.canonicalize_key(&key);
            if !self.is_ignored(&key) {
                out.insert(key, v);
            }
        }
        out
    }
}

/// Merge unknown-option lists from multiple configs, summing usages and unioning sites.
pub fn merge_unknowns(lists: impl IntoIterator<Item = Vec<UnknownOption>>) -> Vec<UnknownOption> {
    let mut map: BTreeMap<OptionKey, UnknownOption> = BTreeMap::new();
    for list in lists {
        for u in list {
            let entry = map.entry(u.key.clone()).or_insert_with(|| UnknownOption {
                raw_name: u.raw_name.clone(),
                key: u.key.clone(),
                usage_count: 0,
                example_value: None,
                sites: Vec::new(),
            });
            entry.usage_count += u.usage_count;
            if entry.example_value.is_none() {
                entry.example_value = u.example_value;
            }
            for site in u.sites {
                if !entry.sites.contains(&site) {
                    entry.sites.push(site);
                }
            }
        }
    }
    let mut out: Vec<_> = map.into_values().collect();
    out.sort_by(|a, b| a.raw_name.cmp(&b.raw_name));
    out
}
