// Modrinth source implementation

use crate::sources::source_trait::{PluginSource, ResolvedVersion};
use crate::sources::version_matcher;
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
    #[serde(rename = "game_versions")]
    pub game_versions: Vec<String>,
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

async fn get_versions(
    plugin_id: &str,
    minecraft_version: Option<&str>,
) -> anyhow::Result<Vec<Version>> {
    let mut url = format!("https://api.modrinth.com/v2/project/{}/version", plugin_id);

    // Add game_versions filter if Minecraft version is provided
    // Modrinth API expects: ?game_versions=["1.20.1"] (JSON array as query param)
    if let Some(mc_version) = minecraft_version {
        let json_array = serde_json::to_string(&[mc_version])
            .map_err(|e| anyhow::anyhow!("Failed to encode Minecraft version: {}", e))?;
        // URL encode the JSON array string
        let encoded = urlencoding::encode(&json_array);
        url = format!("{}?game_versions={}", url, encoded);
    }

    let versions: Vec<Version> = reqwest::get(&url).await?.json().await?;
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
        minecraft_version: Option<&str>,
    ) -> anyhow::Result<ResolvedVersion> {
        // First get the plugin to get the ID
        let plugin = get_plugin(plugin_id).await?;

        // Get versions filtered by Minecraft version if provided
        let mut versions = get_versions(&plugin.id, minecraft_version).await?;

        // If filtering returned no results and we have a Minecraft version, try without filter for better error message
        let mut all_versions = if versions.is_empty() && minecraft_version.is_some() {
            get_versions(&plugin.id, None).await?
        } else {
            Vec::new()
        };

        let version = if let Some(version_str) = requested_version {
            // Find the specific version in filtered results
            let found_version = versions.iter().find(|v| v.version_number == version_str);

            match found_version {
                Some(v) => {
                    // Verify compatibility if Minecraft version is specified
                    if let Some(mc_version) = minecraft_version {
                        let is_compatible = v
                            .game_versions
                            .iter()
                            .any(|gv| version_matcher::matches_mc_version(gv, mc_version));
                        if !is_compatible {
                            anyhow::bail!(
                                "Plugin '{}' version '{}' is not compatible with Minecraft {}. Compatible versions: {}",
                                plugin_id,
                                version_str,
                                mc_version,
                                v.game_versions.join(", ")
                            );
                        }
                    }
                    v
                }
                None => {
                    // Check if version exists but is incompatible
                    if let Some(mc_version) = minecraft_version {
                        if all_versions.is_empty() {
                            all_versions = get_versions(&plugin.id, None).await?;
                        }
                        if let Some(incompatible_version) = all_versions
                            .iter()
                            .find(|v| v.version_number == version_str)
                        {
                            anyhow::bail!(
                                "Plugin '{}' version '{}' is not compatible with Minecraft {}. Compatible versions: {}",
                                plugin_id,
                                version_str,
                                mc_version,
                                incompatible_version.game_versions.join(", ")
                            );
                        }
                    }
                    anyhow::bail!(
                        "Version '{}' not found for plugin '{}'",
                        version_str,
                        plugin_id
                    )
                }
            }
        } else {
            // Get the latest compatible version
            if versions.is_empty() {
                if let Some(mc_version) = minecraft_version {
                    if all_versions.is_empty() {
                        all_versions = get_versions(&plugin.id, None).await?;
                    }
                    anyhow::bail!(
                        "No versions of plugin '{}' are compatible with Minecraft {}. Latest version supports: {}",
                        plugin_id,
                        mc_version,
                        all_versions
                            .first()
                            .map(|v| v.game_versions.join(", "))
                            .unwrap_or_else(|| "unknown".to_string())
                    );
                } else {
                    anyhow::bail!("No versions found for plugin '{}'", plugin_id);
                }
            }

            // Sort by date_published descending to ensure determinism
            versions.sort_by(|a, b| {
                // Sort by date_published descending (newest first)
                b.date_published.cmp(&a.date_published)
            });
            versions.first().unwrap()
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
