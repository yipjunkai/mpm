// Spigot source implementation (via Spiget API)

use crate::sources::hash::{self, HashAlgorithm};
use crate::sources::http;
use crate::sources::search::{self, Searchable};
use crate::sources::source_trait::{PluginSource, ResolvedVersion};
use crate::sources::version_data::{DownloadInfo, NormalizedVersion};
use crate::sources::version_selector::{self, SelectionConfig};
use async_trait::async_trait;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct ResourceFile {
    #[serde(rename = "externalUrl")]
    external_url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Resource {
    id: i64,
    name: String,
    file: Option<ResourceFile>,
}

#[derive(Debug, Clone, Deserialize)]
struct Version {
    id: i64,
    name: String,
    #[serde(rename = "releaseDate")]
    release_date: i64,
    #[serde(rename = "testedVersions")]
    tested_versions: Option<Vec<String>>,
}

// Implement Searchable for Resource
impl Searchable for Resource {
    fn search_name(&self) -> &str {
        &self.name
    }
}

pub struct SpigotSource;

impl SpigotSource {
    /// Normalize a Spiget API version to our common format
    fn normalize_version(v: &Version, resource_id: i64) -> NormalizedVersion {
        let download_url = format!(
            "https://api.spiget.org/v2/resources/{}/versions/{}/download",
            resource_id, v.id
        );

        NormalizedVersion {
            version: v.name.clone(),
            published_at: v.release_date.to_string(),
            mc_versions: v.tested_versions.clone().unwrap_or_default(),
            download: DownloadInfo::without_hash(&download_url, None),
        }
    }

    /// Fetch versions from the Spiget API
    async fn fetch_versions(resource_id: i64) -> anyhow::Result<Vec<NormalizedVersion>> {
        let url = format!(
            "https://api.spiget.org/v2/resources/{}/versions?size=1000",
            resource_id
        );

        let versions: Vec<Version> = http::fetch_json(&url).await?;
        Ok(versions
            .iter()
            .map(|v| Self::normalize_version(v, resource_id))
            .collect())
    }

    /// Search for a resource by name with hyphen variations
    async fn search_resource(&self, search_name: &str) -> anyhow::Result<Resource> {
        // Try the original search name first
        let mut search_terms = vec![search_name.to_string()];

        // If the search name contains hyphens, also try with spaces instead
        if search_name.contains('-') {
            search_terms.push(search_name.replace('-', " "));
        }

        // Try each search term variation
        for search_term in &search_terms {
            let search_url = format!(
                "https://api.spiget.org/v2/search/resources/{}?size=100",
                urlencoding::encode(search_term)
            );

            let response = http::client().get(&search_url).send().await?;
            if !response.status().is_success() {
                continue;
            }

            let results: Vec<Resource> = response.json().await?;
            if !results.is_empty() {
                return self.process_search_results(results, search_name);
            }
        }

        anyhow::bail!("No resources found matching '{}' in Spigot", search_name);
    }

    /// Process search results and return the best match
    fn process_search_results(
        &self,
        mut results: Vec<Resource>,
        search_name: &str,
    ) -> anyhow::Result<Resource> {
        search::rank_search_results(&mut results, search_name);
        Ok(results.into_iter().next().unwrap())
    }

    /// Parse plugin ID and resolve to resource ID
    async fn resolve_resource_id(&self, plugin_id: &str) -> anyhow::Result<(i64, Option<String>)> {
        if plugin_id.chars().all(|c| c.is_ascii_digit()) {
            // Numeric ID format
            let id = plugin_id.parse::<i64>().map_err(|_| {
                anyhow::anyhow!("Invalid Spigot resource ID format: '{}'", plugin_id)
            })?;
            Ok((id, None))
        } else {
            // Name format - search for it
            let resource = self.search_resource(plugin_id).await?;
            let external_url = resource.file.and_then(|f| f.external_url);
            Ok((resource.id, external_url))
        }
    }

    /// Download and hash the resource, handling external URL fallback
    async fn download_with_hash(
        resource_id: i64,
        version: &NormalizedVersion,
        external_url: Option<&str>,
    ) -> anyhow::Result<ResolvedVersion> {
        let download_url = &version.download.url;

        // Try the Spiget download endpoint first
        let mut response = http::download_with_response(download_url).await?;

        // If the download failed, try external URL as fallback
        let mut final_url = download_url.clone();
        if !response.status().is_success() {
            if let Some(ext_url) = external_url {
                let external_response = http::download_with_response(ext_url).await?;

                if !external_response.status().is_success() {
                    anyhow::bail!(
                        "Failed to download resource '{}' version '{}' from external URL '{}': HTTP {}",
                        resource_id,
                        version.version,
                        ext_url,
                        external_response.status()
                    );
                }

                // Check if the response is actually a JAR file
                let content_type = http::get_content_type(&external_response).unwrap_or_default();
                let is_jar_file = content_type.starts_with("application/java-archive")
                    || content_type.starts_with("application/x-java-archive")
                    || ext_url.ends_with(".jar");

                if !is_jar_file {
                    anyhow::bail!(
                        "External URL '{}' for resource '{}' version '{}' does not point to a JAR file (Content-Type: {}). \
                        Please ensure the external URL points directly to a .jar file download.",
                        ext_url,
                        resource_id,
                        version.version,
                        if content_type.is_empty() {
                            "not specified"
                        } else {
                            &content_type
                        }
                    );
                }

                response = external_response;
                final_url = ext_url.to_string();
            } else {
                // Check if it's a 403 and no external URL was available
                if response.status() == reqwest::StatusCode::FORBIDDEN {
                    anyhow::bail!(
                        "SpigotMC uses Cloudflare protection that blocks automated downloads, and this resource doesn't have an external download URL. \
                        Please download the plugin manually from https://www.spigotmc.org/resources/{}/ and add it to your server.",
                        resource_id
                    );
                }
                anyhow::bail!(
                    "Failed to download resource '{}' version '{}': HTTP {}",
                    resource_id,
                    version.version,
                    response.status()
                );
            }
        }

        // Extract filename and compute hash
        let filename = http::extract_filename(&response, &final_url);
        let filename = if filename.is_empty() || filename == "download.jar" {
            format!("{}.jar", version.version)
        } else {
            filename
        };

        let data = response.bytes().await?;
        let hash_str = hash::compute_hash(&data, HashAlgorithm::Sha256);

        Ok(ResolvedVersion {
            version: version.version.clone(),
            filename,
            url: final_url,
            hash: hash_str,
        })
    }
}

#[async_trait]
impl PluginSource for SpigotSource {
    fn name(&self) -> &'static str {
        "spigot"
    }

    fn validate_plugin_id(&self, plugin_id: &str) -> anyhow::Result<()> {
        if plugin_id.is_empty() {
            anyhow::bail!("Spigot plugin ID cannot be empty");
        }
        Ok(())
    }

    async fn resolve_version(
        &self,
        plugin_id: &str,
        requested_version: Option<&str>,
        minecraft_version: Option<&str>,
    ) -> anyhow::Result<ResolvedVersion> {
        // Resolve plugin ID to resource ID
        let (resource_id, external_url) = self.resolve_resource_id(plugin_id).await?;

        // Verify resource exists
        let resource_url = format!("https://api.spiget.org/v2/resources/{}", resource_id);
        let resource: Resource = http::fetch_json(&resource_url)
            .await
            .map_err(|_| anyhow::anyhow!("Resource '{}' not found in Spigot", resource_id))?;

        // Get external URL from resource if not already have it
        let external_url = external_url.or_else(|| resource.file.and_then(|f| f.external_url));

        // Fetch all versions
        let versions = Self::fetch_versions(resource_id).await?;

        if versions.is_empty() {
            anyhow::bail!("No versions found for resource '{}'", resource_id);
        }

        // Use version selector with treat_empty_as_compatible for Spigot
        // (many Spigot plugins don't properly fill tested_versions)
        let config = SelectionConfig::new(resource_id.to_string()).treat_empty_as_compatible();

        // Find the selected version first
        let selected = version_selector::select_version(
            versions.clone(),
            requested_version,
            minecraft_version,
            &config,
        )
        .await?;

        // Find the matching normalized version for download
        let normalized_version = versions
            .iter()
            .find(|v| v.version == selected.version)
            .ok_or_else(|| anyhow::anyhow!("Version not found after selection"))?;

        // Now download with hash computation
        Self::download_with_hash(resource_id, normalized_version, external_url.as_deref()).await
    }
}
