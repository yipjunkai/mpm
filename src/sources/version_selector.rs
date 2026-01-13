// Unified version selection logic

use crate::sources::hash::{self, HashAlgorithm};
use crate::sources::http;
use crate::sources::source_trait::ResolvedVersion;
use crate::sources::version_data::NormalizedVersion;
use crate::sources::version_matcher;
use anyhow::Result;

/// Configuration for version selection
pub struct SelectionConfig {
    /// Plugin identifier for error messages
    pub plugin_id: String,
    /// Whether to treat empty mc_versions as compatible with any MC version
    pub treat_empty_as_compatible: bool,
}

impl SelectionConfig {
    pub fn new(plugin_id: impl Into<String>) -> Self {
        Self {
            plugin_id: plugin_id.into(),
            treat_empty_as_compatible: false,
        }
    }

    pub fn treat_empty_as_compatible(mut self) -> Self {
        self.treat_empty_as_compatible = true;
        self
    }
}

/// Select the appropriate version from a list of normalized versions
///
/// Handles:
/// - Finding specific version vs latest
/// - Minecraft version filtering
/// - Sorting by publication date
/// - Appropriate error messages
pub async fn select_version(
    versions: Vec<NormalizedVersion>,
    requested_version: Option<&str>,
    minecraft_version: Option<&str>,
    config: &SelectionConfig,
) -> Result<ResolvedVersion> {
    // Store all versions for error messages
    let all_versions = versions.clone();

    // Filter by Minecraft version if provided
    let mut filtered_versions = if let Some(mc_version) = minecraft_version {
        filter_by_mc_version(versions, mc_version, config.treat_empty_as_compatible)
    } else {
        versions
    };

    let selected = if let Some(version_str) = requested_version {
        select_specific_version(
            &filtered_versions,
            &all_versions,
            version_str,
            minecraft_version,
            config,
        )?
    } else {
        select_latest_version(
            &mut filtered_versions,
            &all_versions,
            minecraft_version,
            config,
        )?
    };

    // Resolve to final ResolvedVersion (may need to download for hash)
    resolve_download(selected, &config.plugin_id).await
}

/// Filter versions by Minecraft version compatibility
fn filter_by_mc_version(
    versions: Vec<NormalizedVersion>,
    mc_version: &str,
    treat_empty_as_compatible: bool,
) -> Vec<NormalizedVersion> {
    versions
        .into_iter()
        .filter(|v| {
            if v.mc_versions.is_empty() {
                // Empty mc_versions - behavior depends on config
                treat_empty_as_compatible
            } else {
                v.mc_versions
                    .iter()
                    .any(|gv| version_matcher::matches_mc_version(gv, mc_version))
            }
        })
        .collect()
}

/// Select a specific version from the list
fn select_specific_version<'a>(
    filtered_versions: &'a [NormalizedVersion],
    all_versions: &'a [NormalizedVersion],
    version_str: &str,
    minecraft_version: Option<&str>,
    config: &SelectionConfig,
) -> Result<&'a NormalizedVersion> {
    // Try to find in filtered results first
    if let Some(v) = filtered_versions.iter().find(|v| v.version == version_str) {
        // Verify compatibility if Minecraft version is specified
        if let Some(mc_version) = minecraft_version
            && !v.mc_versions.is_empty()
        {
            let is_compatible = v
                .mc_versions
                .iter()
                .any(|gv| version_matcher::matches_mc_version(gv, mc_version));
            if !is_compatible {
                anyhow::bail!(
                    "Plugin '{}' version '{}' is not compatible with Minecraft {}. Compatible versions: {}",
                    config.plugin_id,
                    version_str,
                    mc_version,
                    v.mc_versions.join(", ")
                );
            }
        }
        return Ok(v);
    }

    // Check if version exists but is incompatible
    if let Some(mc_version) = minecraft_version
        && let Some(incompatible_version) = all_versions.iter().find(|v| v.version == version_str)
    {
        anyhow::bail!(
            "Plugin '{}' version '{}' is not compatible with Minecraft {}. Compatible versions: {}",
            config.plugin_id,
            version_str,
            mc_version,
            if incompatible_version.mc_versions.is_empty() {
                "unknown".to_string()
            } else {
                incompatible_version.mc_versions.join(", ")
            }
        );
    }

    anyhow::bail!(
        "Version '{}' not found for plugin '{}'",
        version_str,
        config.plugin_id
    )
}

/// Select the latest version from the list
fn select_latest_version<'a>(
    filtered_versions: &'a mut [NormalizedVersion],
    all_versions: &'a [NormalizedVersion],
    minecraft_version: Option<&str>,
    config: &SelectionConfig,
) -> Result<&'a NormalizedVersion> {
    if filtered_versions.is_empty() {
        if let Some(mc_version) = minecraft_version {
            anyhow::bail!(
                "No versions of plugin '{}' are compatible with Minecraft {}. Latest version supports: {}",
                config.plugin_id,
                mc_version,
                all_versions
                    .first()
                    .map(|v| {
                        if v.mc_versions.is_empty() {
                            "unknown".to_string()
                        } else {
                            v.mc_versions.join(", ")
                        }
                    })
                    .unwrap_or_else(|| "unknown".to_string())
            );
        } else {
            anyhow::bail!("No versions found for plugin '{}'", config.plugin_id);
        }
    }

    // Sort by published_at descending (newest first)
    filtered_versions.sort_by(|a, b| b.published_at.cmp(&a.published_at));

    Ok(filtered_versions.first().unwrap())
}

/// Resolve a NormalizedVersion to a ResolvedVersion
/// Downloads the file if hash is not available
async fn resolve_download(version: &NormalizedVersion, plugin_id: &str) -> Result<ResolvedVersion> {
    let download = &version.download;

    if let Some(hash) = &download.hash {
        // Hash is available from API
        let filename = download
            .filename
            .clone()
            .unwrap_or_else(|| format!("{}.jar", version.version));

        Ok(ResolvedVersion {
            version: version.version.clone(),
            filename,
            url: download.url.clone(),
            hash: hash.clone(),
        })
    } else {
        // Need to download to compute hash
        let response = http::download_with_response(&download.url).await?;

        if !response.status().is_success() {
            anyhow::bail!(
                "Failed to download plugin '{}' version '{}': HTTP {}",
                plugin_id,
                version.version,
                response.status()
            );
        }

        let filename = download
            .filename
            .clone()
            .unwrap_or_else(|| http::extract_filename(&response, &download.url));

        let data = response.bytes().await?;
        let hash = hash::compute_hash(&data, HashAlgorithm::Sha256);

        Ok(ResolvedVersion {
            version: version.version.clone(),
            filename,
            url: download.url.clone(),
            hash,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sources::version_data::DownloadInfo;

    fn make_version(version: &str, mc_versions: Vec<&str>) -> NormalizedVersion {
        NormalizedVersion {
            version: version.to_string(),
            published_at: "2024-01-01T00:00:00Z".to_string(),
            mc_versions: mc_versions.into_iter().map(String::from).collect(),
            download: DownloadInfo::with_hash(
                "https://example.com/file.jar",
                "file.jar",
                "sha256:abc123",
            ),
        }
    }

    #[test]
    fn test_filter_by_mc_version() {
        let versions = vec![
            make_version("1.0", vec!["1.20.1"]),
            make_version("2.0", vec!["1.21"]),
            make_version("3.0", vec!["1.20.1", "1.21"]),
        ];

        let filtered = filter_by_mc_version(versions, "1.20.1", false);
        assert_eq!(filtered.len(), 2);
        assert_eq!(filtered[0].version, "1.0");
        assert_eq!(filtered[1].version, "3.0");
    }

    #[test]
    fn test_filter_empty_as_compatible() {
        let versions = vec![
            make_version("1.0", vec!["1.20.1"]),
            make_version("2.0", vec![]), // Empty mc_versions
        ];

        // Without treat_empty_as_compatible
        let filtered = filter_by_mc_version(versions.clone(), "1.20.1", false);
        assert_eq!(filtered.len(), 1);

        // With treat_empty_as_compatible
        let filtered = filter_by_mc_version(versions, "1.20.1", true);
        assert_eq!(filtered.len(), 2);
    }
}
