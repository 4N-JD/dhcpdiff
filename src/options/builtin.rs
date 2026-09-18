use std::collections::BTreeMap;

pub fn load_builtin_names() -> BTreeMap<String, u16> {
    let yaml = include_str!("../../mappings/builtin.yaml");
    let parsed: BuiltinFile = serde_yaml::from_str(yaml).unwrap_or_default();
    let mut map = BTreeMap::new();
    for entry in parsed.options {
        map.insert(entry.name.clone(), entry.code);
        if let Some(aliases) = entry.aliases {
            for alias in aliases {
                map.insert(alias, entry.code);
            }
        }
    }
    map
}

/// Canonical dhcp option name by code (excludes aliases).
pub fn load_builtin_dhcp_names_by_code() -> BTreeMap<u16, String> {
    let yaml = include_str!("../../mappings/builtin.yaml");
    let parsed: BuiltinFile = serde_yaml::from_str(yaml).unwrap_or_default();
    parsed
        .options
        .into_iter()
        .map(|entry| (entry.code, entry.name))
        .collect()
}

pub fn lookup_by_name(name: &str) -> Option<u16> {
    load_builtin_names().get(name).copied()
}

#[derive(Debug, serde::Deserialize, Default)]
struct BuiltinFile {
    #[serde(default)]
    options: Vec<BuiltinOption>,
}

#[derive(Debug, serde::Deserialize)]
struct BuiltinOption {
    name: String,
    code: u16,
    #[serde(default)]
    aliases: Option<Vec<String>>,
}
