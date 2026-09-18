use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Serialize};

use super::scenario::ClientScenario;

static VCI_EQ_SPACED: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)option\s+vendor-class-identifier\s*=\s*"([^"]+)""#).unwrap()
});
static VCI_EQ_TIGHT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)option\s+vendor-class-identifier="([^"]+)""#).unwrap()
});

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum FilterMatch {
    VendorClassExact { value: String },
    VendorClassPrefix {
        value: String,
        offset: u32,
        length: u32,
    },
    ClientIdPrefix {
        value: String,
        offset: u32,
        length: u32,
    },
    HostnameExact { value: String },
    Opaque { raw: String, kind_hint: String },
}

impl FilterMatch {
    pub fn stable_key(&self) -> String {
        match self {
            Self::VendorClassExact { value } => format!("vce:{value}"),
            Self::VendorClassPrefix {
                value,
                offset,
                length,
            } => format!("vcp:{offset}:{length}:{value}"),
            Self::ClientIdPrefix {
                value,
                offset,
                length,
            } => format!("cip:{offset}:{length}:{value}"),
            Self::HostnameExact { value } => format!("hne:{value}"),
            Self::Opaque { raw, .. } => format!("opaque:{}", raw.to_ascii_lowercase()),
        }
    }

    pub fn matches(&self, scenario: &ClientScenario) -> bool {
        match self {
            Self::VendorClassExact { value } => {
                scenario.vendor_class.as_deref() == Some(value.as_str())
            }
            Self::VendorClassPrefix {
                value,
                offset,
                length,
            } => scenario
                .vendor_class
                .as_ref()
                .map(|vci| {
                    let start = *offset as usize;
                    let end = start.saturating_add(*length as usize);
                    vci.len() >= end && &vci[start..end] == value.as_str()
                })
                .unwrap_or(false),
            Self::ClientIdPrefix {
                value,
                offset,
                length,
            } => scenario
                .client_id_prefix
                .as_ref()
                .map(|cid| {
                    let start = *offset as usize;
                    let end = start.saturating_add(*length as usize);
                    cid.len() >= end && &cid[start..end] == value.as_str()
                })
                .unwrap_or(false),
            Self::HostnameExact { value } => scenario.hostname.as_deref() == Some(value.as_str()),
            Self::Opaque { raw, .. } => opaque_matches_scenario(raw, scenario),
        }
    }

    /// Extract a vendor-class identifier value if this match implies one.
    pub fn vendor_class_hint(&self) -> Option<String> {
        match self {
            Self::VendorClassExact { value } => Some(value.clone()),
            Self::VendorClassPrefix { value, .. } => Some(value.clone()),
            Self::Opaque { raw, .. } => extract_vci_from_opaque(raw),
            _ => None,
        }
    }
}

fn opaque_matches_scenario(raw: &str, scenario: &ClientScenario) -> bool {
    if let Some(value) = extract_vci_from_opaque(raw) {
        return scenario.vendor_class.as_deref() == Some(value.as_str());
    }
    false
}

fn extract_vci_from_opaque(raw: &str) -> Option<String> {
    if let Some(caps) = VCI_EQ_SPACED.captures(raw) {
        return Some(caps[1].to_string());
    }
    if let Some(caps) = VCI_EQ_TIGHT.captures(raw) {
        return Some(caps[1].to_string());
    }
    None
}
