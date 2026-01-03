// Trait definition for plugin sources

use anyhow::Result;

/// Result of resolving a plugin version
#[derive(Debug, Clone)]
pub struct ResolvedVersion {
    pub version: String,
    pub filename: String,
    pub url: String,
    pub hash: String,
}

/// Trait for plugin sources (Modrinth, Hangar, GitHub, etc.)
#[async_trait::async_trait]
pub trait PluginSource: Send + Sync {
    /// Resolve a plugin version
    ///
    /// # Arguments
    /// * `plugin_id` - The plugin identifier (format depends on source)
    /// * `requested_version` - Optional specific version to resolve
    /// * `minecraft_version` - Optional Minecraft version for compatibility filtering
    ///
    /// # Returns
    /// A `ResolvedVersion` containing version, filename, URL, and hash
    async fn resolve_version(
        &self,
        plugin_id: &str,
        requested_version: Option<&str>,
        minecraft_version: Option<&str>,
    ) -> Result<ResolvedVersion>;

    /// Get the source name (e.g., "modrinth", "hangar", "github")
    fn name(&self) -> &'static str;

    /// Validate the plugin ID format for this source
    fn validate_plugin_id(&self, plugin_id: &str) -> Result<()>;
}
