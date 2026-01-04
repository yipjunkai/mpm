// Spigot source implementation (via Spiget API)

use crate::sources::source_trait::{PluginSource, ResolvedVersion};
use crate::sources::version_matcher;
use async_trait::async_trait;
use serde::Deserialize;
use sha2::{Digest, Sha256};

#[derive(Debug, Deserialize)]
struct Resource {
    #[allow(dead_code)] // Required for deserialization but not used
    id: i64,
    name: String,
    #[serde(rename = "testedVersions")]
    #[allow(dead_code)] // Used in filtering but may not always be present
    tested_versions: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct SearchResponse {
    data: Vec<Resource>,
    #[allow(dead_code)] // Required for deserialization but not used
    size: i64,
}

#[derive(Debug, Clone, Deserialize)]
struct Version {
    #[allow(dead_code)] // Required for deserialization but not used
    id: i64,
    name: String,
    #[serde(rename = "releaseDate")]
    release_date: i64,
    #[serde(rename = "testedVersions")]
    tested_versions: Option<Vec<String>>,
}

pub struct SpigotSource;

#[async_trait]
impl PluginSource for SpigotSource {
    fn name(&self) -> &'static str {
        "spigot"
    }

    fn validate_plugin_id(&self, plugin_id: &str) -> anyhow::Result<()> {
        // Spigot accepts numeric resource IDs or plugin names (for search)
        if plugin_id.is_empty() {
            anyhow::bail!("Spigot plugin ID cannot be empty");
        }
        // Allow both formats: numeric ID (e.g., "1234") or name (for search)
        Ok(())
    }

    async fn resolve_version(
        &self,
        plugin_id: &str,
        requested_version: Option<&str>,
        minecraft_version: Option<&str>,
    ) -> anyhow::Result<ResolvedVersion> {
        // Parse plugin_id - could be numeric ID or name (for search)
        let resource_id = if plugin_id.chars().all(|c| c.is_ascii_digit()) {
            // Numeric ID format
            plugin_id.parse::<i64>().map_err(|_| {
                anyhow::anyhow!("Invalid Spigot resource ID format: '{}'", plugin_id)
            })?
        } else {
            // Name format - search for it
            let search_name = plugin_id;
            let found_resource = self.search_resource(search_name).await?;
            found_resource.id
        };

        // Get resource info to verify it exists
        let resource_url = format!("https://api.spiget.org/v2/resources/{}", resource_id);
        let response = reqwest::get(&resource_url).await?;

        if response.status() == reqwest::StatusCode::NOT_FOUND {
            anyhow::bail!("Resource '{}' not found in Spigot", resource_id);
        }

        if !response.status().is_success() {
            anyhow::bail!(
                "Failed to fetch Spigot resource '{}': HTTP {}",
                resource_id,
                response.status()
            );
        }

        let _resource: Resource = response
            .json()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to parse Spigot resource response: {}", e))?;

        // Get all versions
        let versions_url = format!(
            "https://api.spiget.org/v2/resources/{}/versions?size=1000",
            resource_id
        );
        let all_versions: Vec<Version> = reqwest::get(&versions_url)
            .await?
            .json()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to fetch Spigot versions: {}", e))?;

        if all_versions.is_empty() {
            anyhow::bail!("No versions found for resource '{}'", resource_id);
        }

        // Filter by Minecraft version if provided
        let mut versions = if let Some(mc_version) = minecraft_version {
            all_versions
                .iter()
                .filter(|v| {
                    v.tested_versions
                        .as_ref()
                        .map(|tvs| {
                            tvs.iter()
                                .any(|tv| version_matcher::matches_mc_version(tv, mc_version))
                        })
                        .unwrap_or(false)
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
                        let is_compatible = v
                            .tested_versions
                            .as_ref()
                            .map(|tvs| {
                                tvs.iter()
                                    .any(|tv| version_matcher::matches_mc_version(tv, mc_version))
                            })
                            .unwrap_or(false);
                        if !is_compatible {
                            let compatible_versions = v
                                .tested_versions
                                .as_ref()
                                .map(|tvs| tvs.join(", "))
                                .unwrap_or_else(|| "unknown".to_string());
                            anyhow::bail!(
                                "Resource '{}' version '{}' is not compatible with Minecraft {}. Compatible versions: {}",
                                resource_id,
                                version_str,
                                mc_version,
                                compatible_versions
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
                        let compatible_versions = incompatible_version
                            .tested_versions
                            .as_ref()
                            .map(|tvs| tvs.join(", "))
                            .unwrap_or_else(|| "unknown".to_string());
                        anyhow::bail!(
                            "Resource '{}' version '{}' is not compatible with Minecraft {}. Compatible versions: {}",
                            resource_id,
                            version_str,
                            mc_version,
                            compatible_versions
                        );
                    }
                    anyhow::bail!(
                        "Version '{}' not found for resource '{}'",
                        version_str,
                        resource_id
                    )
                }
            }
        } else {
            // Get the latest compatible version
            if versions.is_empty() {
                if let Some(mc_version) = minecraft_version {
                    let latest_compatible = all_versions
                        .first()
                        .and_then(|v| v.tested_versions.as_ref().map(|tvs| tvs.join(", ")))
                        .unwrap_or_else(|| "unknown".to_string());
                    anyhow::bail!(
                        "No versions of resource '{}' are compatible with Minecraft {}. Latest version supports: {}",
                        resource_id,
                        mc_version,
                        latest_compatible
                    );
                } else {
                    anyhow::bail!("No versions found for resource '{}'", resource_id);
                }
            }

            // Sort by release_date descending to ensure determinism
            versions.sort_by(|a, b| {
                // Sort by release_date descending (newest first)
                b.release_date.cmp(&a.release_date)
            });
            versions.first().unwrap()
        };

        // Build download URL: /resources/{resourceId}/versions/{versionId}/download
        let download_url = format!(
            "https://api.spiget.org/v2/resources/{}/versions/{}/download",
            resource_id, version.id
        );

        // Spiget API doesn't provide hashes, so we need to download and compute SHA-256
        let response = reqwest::get(&download_url).await?;
        if !response.status().is_success() {
            anyhow::bail!(
                "Failed to download resource '{}' version '{}': HTTP {}",
                resource_id,
                version.name,
                response.status()
            );
        }

        // Get filename from Content-Disposition header before consuming response
        let filename = response
            .headers()
            .get("content-disposition")
            .and_then(|h| h.to_str().ok())
            .and_then(|s| {
                s.split("filename=")
                    .nth(1)
                    .and_then(|f| f.trim_matches('"').split(';').next())
                    .map(|f| f.trim_matches('"').to_string())
            })
            .unwrap_or_else(|| format!("{}.jar", version.name));

        let data = response.bytes().await?;

        // Compute SHA-256 hash
        let mut hasher = Sha256::new();
        hasher.update(&data);
        let hash_hex = hex::encode(hasher.finalize());
        let hash = format!("sha256:{}", hash_hex);

        Ok(ResolvedVersion {
            version: version.name.clone(),
            filename,
            url: download_url,
            hash,
        })
    }
}

impl SpigotSource {
    /// Search for a resource by name and return the best match (exact name match, case-insensitive)
    async fn search_resource(&self, search_name: &str) -> anyhow::Result<Resource> {
        let search_url = format!(
            "https://api.spiget.org/v2/search/resources/{}?size=100",
            urlencoding::encode(search_name)
        );
        let response = reqwest::get(&search_url).await?;

        if !response.status().is_success() {
            anyhow::bail!(
                "Failed to search Spigot resources: HTTP {}",
                response.status()
            );
        }

        let search_result: SearchResponse = response.json().await?;

        if search_result.data.is_empty() {
            anyhow::bail!("No resources found matching '{}' in Spigot", search_name);
        }

        // Sort by exact name match (case-insensitive)
        // Exact matches first, then partial matches
        let mut results = search_result.data;
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
