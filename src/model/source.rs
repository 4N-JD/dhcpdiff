use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceRef {
    pub vendor: String,
    pub file: String,
    pub line: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_line: Option<u32>,
}

impl SourceRef {
    pub fn new(vendor: impl Into<String>, file: impl Into<String>, line: u32) -> Self {
        Self {
            vendor: vendor.into(),
            file: file.into(),
            line,
            end_line: None,
        }
    }

    pub fn with_span(
        vendor: impl Into<String>,
        file: impl Into<String>,
        line: u32,
        end_line: Option<u32>,
    ) -> Self {
        Self {
            vendor: vendor.into(),
            file: file.into(),
            line,
            end_line: end_line.or(Some(line)),
        }
    }

    pub fn with_end_line(mut self, end_line: u32) -> Self {
        self.end_line = Some(end_line);
        self
    }
}

/// Location attached to a diff entry for UI navigation / snippet extraction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocationRef {
    pub file: String,
    pub line: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_line: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub focus_line: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vendor: Option<String>,
}

impl LocationRef {
    pub fn from_source_ref(r: &SourceRef) -> Self {
        Self {
            file: r.file.clone(),
            line: r.line,
            end_line: r.end_line,
            focus_line: None,
            vendor: Some(r.vendor.clone()),
        }
    }

    pub fn with_focus(mut self, focus_line: u32) -> Self {
        self.focus_line = Some(focus_line);
        self
    }

    pub fn same_span(&self, other: &Self) -> bool {
        self.file == other.file
            && self.line == other.line
            && self.end_line == other.end_line
    }
}

/// Per-side locations: the compared child scope and optional winning declaration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SideLocations {
    pub affected: LocationRef,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub declaration: Option<LocationRef>,
}

impl SideLocations {
    pub fn affected_only(affected: LocationRef) -> Self {
        Self {
            affected,
            declaration: None,
        }
    }

    pub fn with_declaration(affected: LocationRef, declaration: Option<LocationRef>) -> Self {
        let declaration = declaration.filter(|d| !d.same_span(&affected));
        Self {
            affected,
            declaration,
        }
    }
}

impl<'de> Deserialize<'de> for SideLocations {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Nested {
            affected: LocationRef,
            #[serde(default)]
            declaration: Option<LocationRef>,
        }

        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Wire {
            Nested(Nested),
            Flat(LocationRef),
        }

        match Wire::deserialize(deserializer)? {
            Wire::Nested(n) => Ok(Self {
                affected: n.affected,
                declaration: n.declaration,
            }),
            Wire::Flat(loc) => Ok(Self::affected_only(loc)),
        }
    }
}

impl From<&SourceRef> for LocationRef {
    fn from(r: &SourceRef) -> Self {
        Self::from_source_ref(r)
    }
}
