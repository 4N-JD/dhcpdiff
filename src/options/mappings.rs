use std::path::Path;

use serde::{Deserialize, Serialize};

use super::OptionKeyRef;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UserMappings {
    #[serde(default)]
    pub aliases: Vec<AliasMapping>,
    #[serde(default)]
    pub equivalences: Vec<EquivalenceMapping>,
    #[serde(default)]
    pub ignore: Vec<OptionKeyRef>,
    /// When true (default), dhcp option 1 (subnet-mask) is excluded from diff/normalize.
    #[serde(default)]
    pub ignore_subnet_mask: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AliasMapping {
    pub source_name: String,
    pub canonical: OptionKeyRef,
    #[serde(default)]
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EquivalenceMapping {
    pub source: OptionKeyRef,
    pub target: OptionKeyRef,
    #[serde(default)]
    pub confirmed: bool,
}

pub fn load_user_mappings(path: &Path) -> anyhow::Result<UserMappings> {
    let content = std::fs::read_to_string(path)?;
    let mappings: UserMappings = serde_yaml::from_str(&content)?;
    Ok(mappings)
}

pub fn save_user_mappings(path: &Path, mappings: &UserMappings) -> anyhow::Result<()> {
    let content = serde_yaml::to_string(mappings)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, content)?;
    Ok(())
}
