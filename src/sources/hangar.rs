// Hangar source implementation (PaperMC plugin repository)

use crate::sources::source_trait::{PluginSource, ResolvedVersion};
use crate::sources::version_matcher;
use async_trait::async_trait;
use hex;
use serde::Deserialize;
use sha2::{Digest, Sha256};

#[derive(Debug, Deserialize)]
struct Project {
    #[allow(dead_code)] // Required for deserialization but not used
    id: i64,
    name: String,
    namespace: Namespace,
}

#[derive(Debug, Deserialize)]
struct SearchResponse {
    result: Vec<Project>,
    #[allow(dead_code)] // Required for deserialization but not used
    pagination: serde_json::Value,
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

pub struct HangarSource;

#[async_trait]
impl PluginSource for HangarSource {
    fn name(&self) -> &'static str {
        "hangar"
    }

    fn validate_plugin_id(&self, plugin_id: &str) -> anyhow::Result<()> {
        // Hangar accepts author/slug format or single word (for search)
        if plugin_id.is_empty() {
            anyhow::bail!("Hangar plugin ID cannot be empty");
        }
        // Allow both formats: "author/slug" or just "name" (for search)
        Ok(())
    }

    async fn resolve_version(
        &self,
        plugin_id: &str,
        requested_version: Option<&str>,
        minecraft_version: Option<&str>,
    ) -> anyhow::Result<ResolvedVersion> {
        // Parse plugin_id - could be author/slug or just name (for search)
        let parts: Vec<&str> = plugin_id.split('/').collect();
        let (author, slug) = if parts.len() == 2 && !parts[0].is_empty() && !parts[1].is_empty() {
            // Full format: author/slug
            (parts[0].to_string(), parts[1].to_string())
        } else if parts.len() == 1 && !parts[0].is_empty() {
            // Single word - search for it
            let search_name = parts[0];
            let found_project = self.search_project(search_name).await?;
            (found_project.namespace.owner, found_project.namespace.slug)
        } else {
            anyhow::bail!(
                "Invalid Hangar plugin ID format. Expected 'author/slug' or plugin name, got '{}'",
                plugin_id
            );
        };

        let author = author.as_str();
        let slug = slug.as_str();

        // Get plugin info to verify it exists
        let plugin_url = format!(
            "https://hangar.papermc.io/api/v1/projects/{}/{}",
            author, slug
        );
        let response = reqwest::get(&plugin_url).await?;

        if response.status() == reqwest::StatusCode::NOT_FOUND {
            anyhow::bail!("Plugin '{}/{}' not found in Hangar", author, slug);
        }

        if !response.status().is_success() {
            anyhow::bail!(
                "Failed to fetch Hangar plugin '{}/{}': HTTP {}",
                author,
                slug,
                response.status()
            );
        }

        let _plugin: Project = response
            .json()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to parse Hangar plugin response: {}", e))?;

        // Get all versions
        let versions_url = format!(
            "https://hangar.papermc.io/api/v1/projects/{}/{}/versions",
            author, slug
        );

        #[derive(Debug, Deserialize)]
        struct VersionsResponse {
            result: Vec<Version>,
        }

        let response: VersionsResponse = reqwest::get(&versions_url)
            .await?
            .json()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to fetch Hangar versions: {}", e))?;

        let all_versions = response.result;

        // Filter by Minecraft version if provided
        let mut versions = if let Some(mc_version) = minecraft_version {
            all_versions
                .iter()
                .filter(|v| {
                    // Check if any platform's dependencies match the Minecraft version
                    v.platform_dependencies
                        .values()
                        .flatten()
                        .any(|dep_version| {
                            version_matcher::matches_mc_version(dep_version, mc_version)
                        })
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
                        let is_compatible =
                            v.platform_dependencies
                                .values()
                                .flatten()
                                .any(|dep_version| {
                                    version_matcher::matches_mc_version(dep_version, mc_version)
                                });
                        if !is_compatible {
                            let compatible_versions: Vec<String> = v
                                .platform_dependencies
                                .values()
                                .flatten()
                                .cloned()
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
                    if let Some(mc_version) = minecraft_version
                        && let Some(incompatible_version) =
                            all_versions.iter().find(|v| v.name == version_str)
                    {
                        let compatible_versions: Vec<String> = incompatible_version
                            .platform_dependencies
                            .values()
                            .flatten()
                            .cloned()
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
                                    .values()
                                    .flatten()
                                    .cloned()
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

        // Get the primary download - prefer PAPER platform, fallback to first available
        // Filter out downloads with null fileInfo, downloadUrl, or fileInfo fields
        // Find a valid download - prefer PAPER platform, fallback to first available
        // Valid download has either download_url or external_url
        let download = version
            .downloads
            .get("PAPER")
            .filter(|d| d.download_url.is_some() || d.external_url.is_some())
            .or_else(|| {
                version
                    .downloads
                    .values()
                    .find(|d| d.download_url.is_some() || d.external_url.is_some())
            })
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "No downloads with download URL or external URL found for version '{}' of plugin '{}/{}'",
                    version.name,
                    author,
                    slug
                )
            })?;

        // Prefer download_url, fallback to external_url
        let download_url = download
            .download_url
            .as_ref()
            .or(download.external_url.as_ref())
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "Download URL is null for version '{}' of plugin '{}/{}'",
                    version.name,
                    author,
                    slug
                )
            })?;

        // Handle filename and hash - prefer from fileInfo if available
        let (filename, hash) = if let Some(file_info) = &download.file_info {
            if let (Some(name), Some(sha256_hash)) = (&file_info.name, &file_info.sha256_hash) {
                // Use fileInfo if available
                (name.clone(), format!("sha256:{}", sha256_hash))
            } else {
                // fileInfo exists but is missing fields
                anyhow::bail!(
                    "Download file info incomplete for version '{}' of plugin '{}/{}'",
                    version.name,
                    author,
                    slug
                );
            }
        } else {
            // fileInfo is null - download file to compute hash (similar to Spigot/GitHub sources)
            let response = reqwest::get(download_url).await?;
            if !response.status().is_success() {
                anyhow::bail!(
                    "Failed to download plugin '{}/{}' version '{}': HTTP {}",
                    author,
                    slug,
                    version.name,
                    response.status()
                );
            }

            // Extract filename from URL or Content-Disposition header
            let filename_from_url = response
                .headers()
                .get("content-disposition")
                .and_then(|h| h.to_str().ok())
                .and_then(|s| {
                    s.split("filename=")
                        .nth(1)
                        .and_then(|f| f.trim_matches('"').split(';').next())
                        .map(|f| f.trim_matches('"').to_string())
                })
                .unwrap_or_else(|| {
                    download_url
                        .split('/')
                        .next_back()
                        .unwrap_or(&format!("{}.jar", version.name))
                        .split('?')
                        .next()
                        .unwrap_or(&format!("{}.jar", version.name))
                        .to_string()
                });

            let data = response.bytes().await?;

            // Compute SHA-256 hash
            let mut hasher = Sha256::new();
            hasher.update(&data);
            let hash_hex = hex::encode(hasher.finalize());
            let hash = format!("sha256:{}", hash_hex);

            (filename_from_url, hash)
        };

        Ok(ResolvedVersion {
            version: version.name.clone(),
            filename,
            url: download_url.clone(),
            hash,
        })
    }
}

impl HangarSource {
    /// Search for a project by name and return the best match (exact name match, case-insensitive)
    async fn search_project(&self, search_name: &str) -> anyhow::Result<Project> {
        let search_url = format!(
            "https://hangar.papermc.io/api/v1/projects?q={}",
            urlencoding::encode(search_name)
        );
        let response = reqwest::get(&search_url).await?;

        if !response.status().is_success() {
            anyhow::bail!(
                "Failed to search Hangar projects: HTTP {}",
                response.status()
            );
        }

        let search_result: SearchResponse = response.json().await?;

        if search_result.result.is_empty() {
            anyhow::bail!("No projects found matching '{}' in Hangar", search_name);
        }

        // Sort by exact name match (case-insensitive)
        // Exact matches first, then partial matches
        let mut results = search_result.result;
        results.sort_by(|a, b| {
            let a_name_lower = a.name.to_lowercase();
            let b_name_lower = b.name.to_lowercase();
            let search_lower = search_name.to_lowercase();

            // Exact match gets highest priority
            let a_exact = a_name_lower == search_lower;
            let b_exact = b_name_lower == search_lower;

            match (a_exact, b_exact) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => {
                    // If both or neither are exact, sort by name
                    a_name_lower.cmp(&b_name_lower)
                }
            }
        });

        // Return the first (best) match
        Ok(results.into_iter().next().unwrap())
    }
}
