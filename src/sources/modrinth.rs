// Modrinth source implementation

use crate::sources::source_trait::{PluginSource, ResolvedVersion};
use async_trait::async_trait;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Project {
    pub id: String,
    #[allow(dead_code)] // Required for deserialization but not used
    pub slug: String,
    #[allow(dead_code)] // Required for deserialization but not used
    pub title: String,
}

#[derive(Debug, Deserialize)]
pub struct Version {
    #[allow(dead_code)] // Required for deserialization but not used
    pub id: String,
    pub version_number: String,
    pub date_published: String,
    pub files: Vec<VersionFile>,
}

#[derive(Debug, Deserialize)]
pub struct VersionFile {
    pub filename: String,
    pub url: String,
    pub hashes: FileHashes,
}

#[derive(Debug, Deserialize)]
pub struct FileHashes {
    pub sha512: String,
}

async fn get_plugin(slug: &str) -> anyhow::Result<Project> {
    let url = format!("https://api.modrinth.com/v2/project/{}", slug);
    let plugin = reqwest::get(url).await?.json().await?;
    Ok(plugin)
}

async fn get_versions(plugin_id: &str) -> anyhow::Result<Vec<Version>> {
    let url = format!("https://api.modrinth.com/v2/project/{}/version", plugin_id);
    let versions: Vec<Version> = reqwest::get(url).await?.json().await?;
    Ok(versions)
}

pub struct ModrinthSource;

#[async_trait]
impl PluginSource for ModrinthSource {
    fn name(&self) -> &'static str {
        "modrinth"
    }

    fn validate_plugin_id(&self, plugin_id: &str) -> anyhow::Result<()> {
        // Modrinth accepts slugs/IDs (alphanumeric, dashes, underscores)
        if plugin_id.is_empty() {
            anyhow::bail!("Modrinth plugin ID cannot be empty");
        }
        Ok(())
    }

    async fn resolve_version(
        &self,
        plugin_id: &str,
        requested_version: Option<&str>,
        _minecraft_version: Option<&str>,
    ) -> anyhow::Result<ResolvedVersion> {
        // First get the plugin to get the ID
        let plugin = get_plugin(plugin_id).await?;

        // Get all versions
        let mut versions = get_versions(&plugin.id).await?;

        let version = if let Some(version_str) = requested_version {
            // Find the specific version
            versions
                .iter()
                .find(|v| v.version_number == version_str)
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "Version '{}' not found for plugin '{}'",
                        version_str,
                        plugin_id
                    )
                })?
        } else {
            // Get the latest version - sort by date_published descending to ensure determinism
            versions.sort_by(|a, b| {
                // Sort by date_published descending (newest first)
                b.date_published.cmp(&a.date_published)
            });
            versions
                .first()
                .ok_or_else(|| anyhow::anyhow!("No versions found for plugin '{}'", plugin_id))?
        };

        // Get the primary file (usually the first one, or the one marked as primary)
        let file = version.files.first().ok_or_else(|| {
            anyhow::anyhow!("No files found for version '{}'", version.version_number)
        })?;

        // Use sha512 from Modrinth API and format as UV-style hash (algorithm:hash)
        let hash = format!("sha512:{}", file.hashes.sha512);

        Ok(ResolvedVersion {
            version: version.version_number.clone(),
            filename: file.filename.clone(),
            url: file.url.clone(),
            hash,
        })
    }
}
