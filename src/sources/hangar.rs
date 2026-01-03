// Hangar source implementation (PaperMC plugin repository)

use crate::sources::source_trait::{PluginSource, ResolvedVersion};
use crate::sources::version_matcher;
use async_trait::async_trait;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Project {
    #[allow(dead_code)] // Required for deserialization but not used
    id: String,
    #[allow(dead_code)] // Required for deserialization but not used
    name: String,
    #[allow(dead_code)] // Required for deserialization but not used
    namespace: Namespace,
}

#[derive(Debug, Deserialize)]
struct Namespace {
    #[allow(dead_code)] // Required for deserialization but not used
    owner: String,
    #[allow(dead_code)] // Required for deserialization but not used
    slug: String,
}

#[derive(Debug, Clone, Deserialize)]
struct Version {
    #[allow(dead_code)] // Required for deserialization but not used
    id: i64,
    name: String,
    #[serde(rename = "createdAt")]
    created_at: String,
    #[serde(rename = "platformDependencies")]
    platform_dependencies: Vec<PlatformDependency>,
    downloads: Vec<Download>,
}

#[derive(Debug, Clone, Deserialize)]
struct PlatformDependency {
    #[allow(dead_code)] // Used in filtering logic via iterator
    name: String,
    version: String,
}

#[derive(Debug, Clone, Deserialize)]
struct Download {
    name: String,
    #[serde(rename = "fileInfo")]
    file_info: FileInfo,
    #[serde(rename = "downloadUrl")]
    download_url: String,
}

#[derive(Debug, Clone, Deserialize)]
struct FileInfo {
    #[serde(rename = "sha256Hash")]
    sha256_hash: String,
}

pub struct HangarSource;

#[async_trait]
impl PluginSource for HangarSource {
    fn name(&self) -> &'static str {
        "hangar"
    }

    fn validate_plugin_id(&self, plugin_id: &str) -> anyhow::Result<()> {
        // Hangar requires author/slug format
        let parts: Vec<&str> = plugin_id.split('/').collect();
        if parts.len() != 2 || parts[0].is_empty() || parts[1].is_empty() {
            anyhow::bail!(
                "Invalid Hangar plugin ID format. Expected 'author/slug', got '{}'",
                plugin_id
            );
        }
        Ok(())
    }

    async fn resolve_version(
        &self,
        plugin_id: &str,
        requested_version: Option<&str>,
        minecraft_version: Option<&str>,
    ) -> anyhow::Result<ResolvedVersion> {
        // Parse plugin_id as author/slug
        let parts: Vec<&str> = plugin_id.split('/').collect();
        if parts.len() != 2 {
            anyhow::bail!(
                "Invalid Hangar plugin ID format. Expected 'author/slug', got '{}'",
                plugin_id
            );
        }
        let author = parts[0];
        let slug = parts[1];

        // Get plugin info to verify it exists
        let plugin_url = format!(
            "https://hangar.papermc.io/api/v1/projects/{}/{}",
            author, slug
        );
        let _plugin: Project = reqwest::get(&plugin_url)
            .await?
            .json()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to fetch Hangar plugin: {}", e))?;

        // Get all versions
        let versions_url = format!(
            "https://hangar.papermc.io/api/v1/projects/{}/{}/versions",
            author, slug
        );
        let all_versions: Vec<Version> = reqwest::get(&versions_url)
            .await?
            .json()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to fetch Hangar versions: {}", e))?;

        // Filter by Minecraft version if provided
        let mut versions = if let Some(mc_version) = minecraft_version {
            all_versions
                .iter()
                .filter(|v| {
                    v.platform_dependencies
                        .iter()
                        .any(|dep| version_matcher::matches_mc_version(&dep.version, mc_version))
                })
                .cloned()
                .collect()
        } else {
            all_versions.clone()
        };

        let version = if let Some(version_str) = requested_version {
            // Find the specific version in filtered results
            let found_version = versions.iter().find(|v| v.name == version_str);

            match found_version {
                Some(v) => {
                    // Verify compatibility if Minecraft version is specified
                    if let Some(mc_version) = minecraft_version {
                        let is_compatible = v.platform_dependencies.iter().any(|dep| {
                            version_matcher::matches_mc_version(&dep.version, mc_version)
                        });
                        if !is_compatible {
                            let compatible_versions: Vec<String> = v
                                .platform_dependencies
                                .iter()
                                .map(|d| d.version.clone())
                                .collect();
                            anyhow::bail!(
                                "Plugin '{}/{}' version '{}' is not compatible with Minecraft {}. Compatible versions: {}",
                                author,
                                slug,
                                version_str,
                                mc_version,
                                compatible_versions.join(", ")
                            );
                        }
                    }
                    v
                }
                None => {
                    // Check if version exists but is incompatible
                    if let Some(mc_version) = minecraft_version {
                        if let Some(incompatible_version) =
                            all_versions.iter().find(|v| v.name == version_str)
                        {
                            let compatible_versions: Vec<String> = incompatible_version
                                .platform_dependencies
                                .iter()
                                .map(|d| d.version.clone())
                                .collect();
                            anyhow::bail!(
                                "Plugin '{}/{}' version '{}' is not compatible with Minecraft {}. Compatible versions: {}",
                                author,
                                slug,
                                version_str,
                                mc_version,
                                compatible_versions.join(", ")
                            );
                        }
                    }
                    anyhow::bail!(
                        "Version '{}' not found for plugin '{}/{}'",
                        version_str,
                        author,
                        slug
                    )
                }
            }
        } else {
            // Get the latest compatible version
            if versions.is_empty() {
                if let Some(mc_version) = minecraft_version {
                    anyhow::bail!(
                        "No versions of plugin '{}/{}' are compatible with Minecraft {}. Latest version supports: {}",
                        author,
                        slug,
                        mc_version,
                        all_versions
                            .first()
                            .map(|v| {
                                v.platform_dependencies
                                    .iter()
                                    .map(|d| d.version.clone())
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            })
                            .unwrap_or_else(|| "unknown".to_string())
                    );
                } else {
                    anyhow::bail!("No versions found for plugin '{}/{}'", author, slug);
                }
            }

            // Sort by created_at descending to ensure determinism
            versions.sort_by(|a, b| {
                // Sort by created_at descending (newest first)
                b.created_at.cmp(&a.created_at)
            });
            versions.first().unwrap()
        };

        // Get the primary download (usually the first one, or the one marked as primary)
        let download = version.downloads.first().ok_or_else(|| {
            anyhow::anyhow!(
                "No downloads found for version '{}' of plugin '{}/{}'",
                version.name,
                author,
                slug
            )
        })?;

        // Use SHA-256 from Hangar API and format as UV-style hash (algorithm:hash)
        let hash = format!("sha256:{}", download.file_info.sha256_hash);

        Ok(ResolvedVersion {
            version: version.name.clone(),
            filename: download.name.clone(),
            url: download.download_url.clone(),
            hash,
        })
    }
}
