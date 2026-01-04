// Sources module for package source implementations

use std::collections::HashMap;
use std::sync::Arc;

pub mod github;
pub mod hangar;
pub mod modrinth;
pub mod source_trait;
pub mod spigot;
pub mod version_matcher;

pub use github::GitHubSource;
pub use hangar::HangarSource;
pub use modrinth::ModrinthSource;
pub use spigot::SpigotSource;

// Re-export the trait and types
#[allow(unused_imports)] // ResolvedVersion is part of the public API
pub use source_trait::{PluginSource, ResolvedVersion};

/// Registry for plugin sources
pub struct SourceRegistry {
    sources: HashMap<String, Arc<dyn PluginSource>>,
}

impl SourceRegistry {
    pub fn new() -> Self {
        let mut registry = Self {
            sources: HashMap::new(),
        };

        // Register all sources in priority order
        // Priority: modrinth > hangar > spigot > github
        registry.register(Arc::new(ModrinthSource));
        registry.register(Arc::new(HangarSource));
        registry.register(Arc::new(SpigotSource));
        registry.register(Arc::new(GitHubSource));

        registry
    }

    fn register(&mut self, source: Arc<dyn PluginSource>) {
        self.sources.insert(source.name().to_string(), source);
    }

    pub fn get(&self, source_name: &str) -> Option<&Arc<dyn PluginSource>> {
        self.sources.get(source_name)
    }

    pub fn get_or_error(&self, source_name: &str) -> anyhow::Result<&Arc<dyn PluginSource>> {
        self.get(source_name).ok_or_else(|| {
            anyhow::anyhow!(
                "Unsupported source: '{}'. Supported sources: {}",
                source_name,
                self.sources
                    .keys()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })
    }

    /// Get sources in priority order for searching
    /// Priority: modrinth > hangar > spigot > github
    pub fn get_priority_order(&self) -> Vec<&Arc<dyn PluginSource>> {
        let mut sources = Vec::new();
        // Add sources in priority order
        if let Some(source) = self.get("modrinth") {
            sources.push(source);
        }
        if let Some(source) = self.get("hangar") {
            sources.push(source);
        }
        if let Some(source) = self.get("spigot") {
            sources.push(source);
        }
        if let Some(source) = self.get("github") {
            sources.push(source);
        }
        sources
    }
}

// Global registry instance
lazy_static::lazy_static! {
    pub static ref REGISTRY: SourceRegistry = SourceRegistry::new();
}
