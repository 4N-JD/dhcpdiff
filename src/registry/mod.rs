pub mod plugin;

pub use plugin::{
    DetectionScore, FormatFamily, Input, VendorDocument, VendorPlugin,
};

use anyhow::{bail, Context};

pub struct VendorRegistry {
    plugins: Vec<Box<dyn VendorPlugin>>,
}

impl VendorRegistry {
    pub fn new() -> Self {
        let mut registry = Self {
            plugins: Vec::new(),
        };
        registry.register_all();
        registry
    }

    fn register_all(&mut self) {
        #[cfg(feature = "vendor-qip")]
        self.register(Box::new(crate::vendors::qip::QipPlugin));
        #[cfg(feature = "vendor-infoblox")]
        self.register(Box::new(crate::vendors::infoblox::InfobloxPlugin));
        #[cfg(feature = "vendor-bluecat")]
        self.register(Box::new(crate::vendors::bluecat::BluecatPlugin));
        #[cfg(feature = "vendor-microsoft")]
        self.register(Box::new(crate::vendors::microsoft::MicrosoftPlugin));
    }

    pub fn register(&mut self, plugin: Box<dyn VendorPlugin>) {
        self.plugins.push(plugin);
    }

    pub fn plugins(&self) -> &[Box<dyn VendorPlugin>] {
        &self.plugins
    }

    pub fn get(&self, id: &str) -> Option<&dyn VendorPlugin> {
        self.plugins
            .iter()
            .find(|p| p.id() == id)
            .map(|p| p.as_ref())
    }

    pub fn resolve(&self, vendor: &str, input: &Input) -> anyhow::Result<&dyn VendorPlugin> {
        if vendor == "auto" {
            return self.detect(input);
        }
        self.get(vendor)
            .with_context(|| format!("unknown vendor '{vendor}'"))
    }

    pub fn detect(&self, input: &Input) -> anyhow::Result<&dyn VendorPlugin> {
        let mut best: Option<(&dyn VendorPlugin, DetectionScore)> = None;
        for plugin in &self.plugins {
            let score = plugin.detect(input);
            if !score.is_confident() {
                continue;
            }
            match best {
                None => best = Some((plugin.as_ref(), score)),
                Some((_, prev)) if score > prev => best = Some((plugin.as_ref(), score)),
                Some((prev_plugin, prev)) if score == prev => {
                    bail!(
                        "ambiguous vendor detection between '{}' and '{}'",
                        prev_plugin.id(),
                        plugin.id()
                    );
                }
                _ => {}
            }
        }
        best.map(|(p, _)| p)
            .with_context(|| "could not auto-detect vendor; specify --vendor explicitly")
    }

    pub fn parse_and_normalize(
        &self,
        vendor: &str,
        input: &Input,
    ) -> anyhow::Result<crate::model::Config> {
        let plugin = self.resolve(vendor, input)?;
        let doc = plugin.parse(input)?;
        plugin.normalize(doc, input)
    }
}

impl Default for VendorRegistry {
    fn default() -> Self {
        Self::new()
    }
}
