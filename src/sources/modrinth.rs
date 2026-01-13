// Modrinth source implementation

use crate::sources::hash::HashAlgorithm;
use crate::sources::http;
use crate::sources::source_trait::{PluginSource, ResolvedVersion};
use crate::sources::version_data::{DownloadInfo, NormalizedVersion};
use crate::sources::version_selector::{self, SelectionConfig};
use async_trait::async_trait;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Project {
    #[allow(dead_code)]
    pub id: String,
}

#[derive(Debug, Deserialize)]
pub struct Version {
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

pub struct ModrinthSource;

impl ModrinthSource {
    /// Normalize a Modrinth API version to our common format
    fn normalize_version(v: &Version) -> Option<NormalizedVersion> {
        let file = v.files.first()?;
        Some(NormalizedVersion {
            version: v.version_number.clone(),
            published_at: v.date_published.clone(),
            mc_versions: v.game_versions.clone(),
            download: DownloadInfo::with_hash(
                &file.url,
                &file.filename,
                crate::sources::hash::format_hash(&file.hashes.sha512, HashAlgorithm::Sha512),
            ),
        })
    }

    /// Fetch versions from the Modrinth API
    async fn fetch_versions(
        plugin_id: &str,
        minecraft_version: Option<&str>,
    ) -> anyhow::Result<Vec<NormalizedVersion>> {
        let mut url = format!("https://api.modrinth.com/v2/project/{}/version", plugin_id);

        // Add game_versions filter if Minecraft version is provided
        if let Some(mc_version) = minecraft_version {
            let json_array = serde_json::to_string(&[mc_version])
                .map_err(|e| anyhow::anyhow!("Failed to encode Minecraft version: {}", e))?;
            let encoded = urlencoding::encode(&json_array);
            url = format!("{}?game_versions={}", url, encoded);
        }

        let versions: Vec<Version> = http::fetch_json(&url).await?;
        Ok(versions
            .iter()
            .filter_map(Self::normalize_version)
            .collect())
    }
}

#[async_trait]
impl PluginSource for ModrinthSource {
    fn name(&self) -> &'static str {
        "modrinth"
    }

    fn validate_plugin_id(&self, plugin_id: &str) -> anyhow::Result<()> {
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
        // Verify plugin exists by fetching project info
        let project_url = format!("https://api.modrinth.com/v2/project/{}", plugin_id);
        let _project: Project = http::fetch_json(&project_url)
            .await
            .map_err(|_| anyhow::anyhow!("Plugin '{}' not found in Modrinth", plugin_id))?;

        // Fetch versions (filtered if MC version provided)
        let mut versions = Self::fetch_versions(plugin_id, minecraft_version).await?;

        // If no versions with filter, try without for better error message
        let all_versions = if versions.is_empty() && minecraft_version.is_some() {
            Self::fetch_versions(plugin_id, None).await?
        } else {
            Vec::new()
        };

        // Combine for version selection
        let versions_for_selection = if versions.is_empty() && !all_versions.is_empty() {
            all_versions
        } else {
            std::mem::take(&mut versions)
        };

        let config = SelectionConfig::new(plugin_id);
        version_selector::select_version(
            versions_for_selection,
            requested_version,
            minecraft_version,
            &config,
        )
        .await
    }
}
