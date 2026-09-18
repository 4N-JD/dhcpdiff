use std::collections::BTreeMap;

use crate::model::{
    Config, OptionKey, BOOTP_FILENAME, BOOTP_NEXT_SERVER, BOOTP_SERVER_NAME,
    ISC_DEFAULT_LEASE_TIME, ISC_MAX_LEASE_TIME, ISC_MIN_LEASE_TIME,
};

use super::builtin::load_builtin_dhcp_names_by_code;

#[derive(Debug, Clone)]
pub struct OptionLabeler {
    names: BTreeMap<(String, u16), String>,
}

impl OptionLabeler {
    pub fn from_configs(source: &Config, target: &Config) -> Self {
        let mut names = BTreeMap::new();
        for (code, name) in load_builtin_dhcp_names_by_code() {
            names.insert(("dhcp".to_string(), code), name);
        }
        names.insert(
            ("bootp".to_string(), BOOTP_NEXT_SERVER),
            "next-server".to_string(),
        );
        names.insert(("bootp".to_string(), BOOTP_FILENAME), "filename".to_string());
        names.insert(
            ("bootp".to_string(), BOOTP_SERVER_NAME),
            "server-name".to_string(),
        );
        names.insert(
            ("isc".to_string(), ISC_DEFAULT_LEASE_TIME),
            "default-lease-time".to_string(),
        );
        names.insert(
            ("isc".to_string(), ISC_MIN_LEASE_TIME),
            "min-lease-time".to_string(),
        );
        names.insert(
            ("isc".to_string(), ISC_MAX_LEASE_TIME),
            "max-lease-time".to_string(),
        );
        for defs in [&source.option_definitions, &target.option_definitions] {
            for def in defs.values() {
                names
                    .entry((def.space.clone(), def.code))
                    .or_insert_with(|| def.name.clone());
            }
        }
        Self { names }
    }

    pub fn format_key(&self, key: &OptionKey) -> String {
        let label = format!("{}:{}", key.space, key.code);
        match self.names.get(&(key.space.clone(), key.code)) {
            Some(name) => format!("{label} ({name})"),
            None => label,
        }
    }
}
