use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Input {
    pub path: PathBuf,
    pub content: String,
}

impl Input {
    pub fn from_path(path: PathBuf) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(&path)?;
        Ok(Self { path, content })
    }

    pub fn file_name(&self) -> String {
        self.path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormatFamily {
    IscConf,
    Xml,
    Json,
}

impl FormatFamily {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::IscConf => "isc-conf",
            Self::Xml => "xml",
            Self::Json => "json",
        }
    }
}

#[derive(Debug, Clone)]
pub enum VendorDocument {
    Isc(crate::formats::isc::IscDocument),
    Xml(crate::formats::xml::XmlDocument),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct DetectionScore(u8);

impl DetectionScore {
    pub const NONE: Self = Self(0);
    pub const LOW: Self = Self(25);
    pub const MEDIUM: Self = Self(50);
    pub const HIGH: Self = Self(75);
    pub const CERTAIN: Self = Self(100);

    pub fn value(self) -> u8 {
        self.0
    }

    pub fn is_confident(self) -> bool {
        self.0 >= Self::MEDIUM.0
    }
}

pub trait VendorPlugin: Send + Sync {
    fn id(&self) -> &'static str;
    fn display_name(&self) -> &'static str;
    fn format_family(&self) -> FormatFamily;
    fn detect(&self, input: &Input) -> DetectionScore;
    fn parse(&self, input: &Input) -> anyhow::Result<VendorDocument>;
    fn normalize(&self, doc: VendorDocument, input: &Input) -> anyhow::Result<crate::model::Config>;
}
