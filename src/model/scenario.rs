use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClientScenario {
    pub vendor_class: Option<String>,
    pub hostname: Option<String>,
    pub client_id_prefix: Option<String>,
}

impl ClientScenario {
    pub fn baseline() -> Self {
        Self::default()
    }

    pub fn with_vendor_class(v: impl Into<String>) -> Self {
        Self {
            vendor_class: Some(v.into()),
            ..Self::default()
        }
    }

    pub fn scope_suffix(&self) -> String {
        match &self.vendor_class {
            Some(vci) => format!(":vci={vci}"),
            None => String::new(),
        }
    }
}
