// Hangar source implementation (PaperMC plugin repository)

use crate::sources::hash::{self, HashAlgorithm};
use crate::sources::http;
use crate::sources::search::{self, ParsedId, Searchable};
use crate::sources::source_trait::{PluginSource, ResolvedVersion};
use crate::sources::version_data::{DownloadInfo, NormalizedVersion};
use crate::sources::version_selector::{self, SelectionConfig};
use async_trait::async_trait;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Project {
    name: String,
    namespace: Namespace,
}

#[derive(Debug, Deserialize)]
struct SearchResponse {
    result: Vec<Project>,
}

#[derive(Debug, Deserialize)]
struct Namespace {
    owner: String,
    slug: String,
}

#[derive(Debug, Clone, Deserialize)]
struct Version {
    name: String,
    #[serde(rename = "createdAt")]
    created_at: String,
    #[serde(rename = "platformDependencies")]
    platform_dependencies: std::collections::HashMap<String, Vec<String>>,
    downloads: std::collections::HashMap<String, Download>,
}

#[derive(Debug, Clone, Deserialize)]
struct Download {
    #[serde(rename = "fileInfo")]
    file_info: Option<FileInfo>,
    #[serde(rename = "downloadUrl")]
    download_url: Option<String>,
    #[serde(rename = "externalUrl")]
    external_url: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct FileInfo {
    name: Option<String>,
    #[serde(rename = "sha256Hash")]
    sha256_hash: Option<String>,
}

#[derive(Debug, Deserialize)]
struct VersionsResponse {
    result: Vec<Version>,
}

// Implement Searchable for Project
impl Searchable for Project {
    fn search_name(&self) -> &str {
        &self.name
    }
}

pub struct HangarSource;

impl HangarSource {
    /// Normalize a Hangar API version to our common format
    fn normalize_version(v: &Version) -> Option<NormalizedVersion> {
        // Get download - prefer PAPER platform, fallback to first available
        let download = v
            .downloads
            .get("PAPER")
            .filter(|d| d.download_url.is_some() || d.external_url.is_some())
            .or_else(|| {
                v.downloads
                    .values()
                    .find(|d| d.download_url.is_some() || d.external_url.is_some())
            })?;

        let url = download
            .download_url
            .as_ref()
            .or(download.external_url.as_ref())?;

        // Get MC versions from platform dependencies
        let mc_versions: Vec<String> = v
            .platform_dependencies
            .values()
            .flatten()
            .cloned()
            .collect();

        // Build download info based on available file info
        let download_info = if let Some(file_info) = &download.file_info {
            if let (Some(name), Some(hash)) = (&file_info.name, &file_info.sha256_hash) {
                DownloadInfo::with_hash(url, name, hash::format_hash(hash, HashAlgorithm::Sha256))
            } else {
                // fileInfo exists but incomplete - need to compute hash
                DownloadInfo::without_hash(url, file_info.name.clone())
            }
        } else {
            // No fileInfo - need to compute hash
            DownloadInfo::url_only(url)
        };

        Some(NormalizedVersion {
            version: v.name.clone(),
            published_at: v.created_at.clone(),
            mc_versions,
            download: download_info,
        })
    }

    /// Fetch versions from the Hangar API
    async fn fetch_versions(author: &str, slug: &str) -> anyhow::Result<Vec<NormalizedVersion>> {
        let url = format!(
            "https://hangar.papermc.io/api/v1/projects/{}/{}/versions",
            author, slug
        );

        let response: VersionsResponse = http::fetch_json(&url).await?;
        Ok(response
            .result
            .iter()
            .filter_map(Self::normalize_version)
            .collect())
    }

    /// Search for a project by name and return the best match
    async fn search_project(&self, search_name: &str) -> anyhow::Result<(String, String)> {
        let search_url = format!(
            "https://hangar.papermc.io/api/v1/projects?q={}",
            urlencoding::encode(search_name)
        );
        let search_result: SearchResponse = http::fetch_json(&search_url).await?;

        if search_result.result.is_empty() {
            anyhow::bail!("No projects found matching '{}' in Hangar", search_name);
        }

        let mut results = search_result.result;
        search::rank_search_results(&mut results, search_name);

        let project = results.into_iter().next().unwrap();
        Ok((project.namespace.owner, project.namespace.slug))
    }

    /// Parse plugin ID and resolve to owner/slug
    async fn resolve_plugin_id(&self, plugin_id: &str) -> anyhow::Result<(String, String)> {
        match search::parse_owner_name_id(plugin_id) {
            ParsedId::Full { owner, name } => Ok((owner, name)),
            ParsedId::SearchTerm(_) => self.search_project(plugin_id).await,
        }
    }
}

#[async_trait]
impl PluginSource for HangarSource {
    fn name(&self) -> &'static str {
        "hangar"
    }

    fn validate_plugin_id(&self, plugin_id: &str) -> anyhow::Result<()> {
        if plugin_id.is_empty() {
            anyhow::bail!("Hangar plugin ID cannot be empty");
        }
        Ok(())
    }

    async fn resolve_version(
        &self,
        plugin_id: &str,
        requested_version: Option<&str>,
        minecraft_version: Option<&str>,
    ) -> anyhow::Result<ResolvedVersion> {
        // Resolve plugin ID to owner/slug
        let (author, slug) = self.resolve_plugin_id(plugin_id).await?;

        // Verify project exists
        let project_url = format!(
            "https://hangar.papermc.io/api/v1/projects/{}/{}",
            author, slug
        );
        http::fetch_json::<Project>(&project_url)
            .await
            .map_err(|_| anyhow::anyhow!("Plugin '{}/{}' not found in Hangar", author, slug))?;

        // Fetch all versions
        let versions = Self::fetch_versions(&author, &slug).await?;

        // Use version selector with plugin ID for error messages
        let display_id = format!("{}/{}", author, slug);
        let config = SelectionConfig::new(&display_id);

        version_selector::select_version(versions, requested_version, minecraft_version, &config)
            .await
    }
}
