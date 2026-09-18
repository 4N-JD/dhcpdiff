use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::{NormalizedValue, SourceRef};

pub type OptionMap = std::collections::BTreeMap<OptionKey, BoundOption>;
pub type OptionDefMap = std::collections::BTreeMap<String, OptionDef>;

/// An option value together with where the winning declaration was found.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoundOption {
    pub value: NormalizedValue,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<SourceRef>,
    /// Human label for the declaring scope (e.g. `Global if PXEClient`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub declared_in: Option<String>,
}

impl BoundOption {
    pub fn new(value: NormalizedValue, source: Option<SourceRef>) -> Self {
        Self {
            value,
            source,
            declared_in: None,
        }
    }

    pub fn with_declared_in(mut self, label: impl Into<String>) -> Self {
        self.declared_in = Some(label.into());
        self
    }

    pub fn map_value(mut self, f: impl FnOnce(NormalizedValue) -> NormalizedValue) -> Self {
        self.value = f(self.value);
        self
    }
}

impl From<NormalizedValue> for BoundOption {
    fn from(value: NormalizedValue) -> Self {
        Self::new(value, None)
    }
}

/// Equality is value-only so inheritance/scenario comparisons ignore provenance.
impl PartialEq for BoundOption {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value
    }
}

impl Eq for BoundOption {}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OptionKey {
    pub space: String,
    pub code: u16,
}

impl Serialize for OptionKey {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&format!("{}:{}", self.space, self.code))
    }
}

impl<'de> Deserialize<'de> for OptionKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        parse_option_key_str(&s).ok_or_else(|| serde::de::Error::custom("invalid option key"))
    }
}

fn parse_option_key_str(s: &str) -> Option<OptionKey> {
    if let Some((space, code)) = s.rsplit_once(':') {
        let code = code.parse().ok()?;
        return Some(OptionKey {
            space: space.to_string(),
            code,
        });
    }
    None
}

/// Synthetic codes for ISC BOOTP packet fields in the `bootp` space.
/// These are never DHCP option codes.
pub const BOOTP_NEXT_SERVER: u16 = 0;
pub const BOOTP_FILENAME: u16 = 1;
pub const BOOTP_SERVER_NAME: u16 = 2;

impl OptionKey {
    pub fn dhcp(code: u16) -> Self {
        Self {
            space: "dhcp".to_string(),
            code,
        }
    }

    pub fn bootp(code: u16) -> Self {
        Self {
            space: "bootp".to_string(),
            code,
        }
    }

    /// ISC `next-server` (BOOTP siaddr).
    pub fn bootp_next_server() -> Self {
        Self::bootp(BOOTP_NEXT_SERVER)
    }

    /// ISC `filename` (BOOTP file).
    pub fn bootp_filename() -> Self {
        Self::bootp(BOOTP_FILENAME)
    }

    /// ISC `server-name` (BOOTP sname).
    pub fn bootp_server_name() -> Self {
        Self::bootp(BOOTP_SERVER_NAME)
    }

    pub fn qualified(space: impl Into<String>, code: u16) -> Self {
        Self {
            space: space.into(),
            code,
        }
    }

    /// Unresolved option name preserved as `unknown/<name>:0`.
    pub fn unresolved(name: impl AsRef<str>) -> Self {
        Self {
            space: format!("unknown/{}", name.as_ref()),
            code: 0,
        }
    }

    /// True for `unknown/<name>:0` and legacy bare `unknown:0`.
    pub fn is_unresolved(&self) -> bool {
        self.code == 0
            && (self.space.starts_with("unknown/") || self.space == "unknown")
    }

    /// BOOTP packet fields — always client-effective, never vendor-gated.
    pub fn is_bootp(&self) -> bool {
        self.space == "bootp"
    }

    /// Original option name when this key was synthesized for an unresolved name.
    pub fn unresolved_name(&self) -> Option<&str> {
        self.space.strip_prefix("unknown/")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OptionDef {
    pub name: String,
    pub space: String,
    pub code: u16,
    pub value_type: String,
}
