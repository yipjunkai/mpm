// GitHub Releases source implementation

use crate::sources::source_trait::{PluginSource, ResolvedVersion};
use async_trait::async_trait;
use log::warn;
use serde::Deserialize;
use sha2::{Digest, Sha256};

#[derive(Debug, Deserialize)]
struct Release {
    #[serde(rename = "tag_name")]
    tag_name: String,
    #[serde(rename = "published_at")]
    #[allow(dead_code)] // Required for deserialization but not used
    published_at: String,
    assets: Vec<Asset>,
}

#[derive(Debug, Deserialize)]
struct Asset {
    name: String,
    #[serde(rename = "browser_download_url")]
    browser_download_url: String,
}

#[derive(Debug, Deserialize)]
struct Repository {
    #[allow(dead_code)] // Required for deserialization but not used
    full_name: String,
    name: String,
    owner: RepositoryOwner,
}

#[derive(Debug, Deserialize)]
struct RepositoryOwner {
    login: String,
}

#[derive(Debug, Deserialize)]
struct SearchResponse {
    items: Vec<Repository>,
    #[allow(dead_code)] // Required for deserialization but not used
    total_count: u64,
}

pub struct GitHubSource;

#[async_trait]
impl PluginSource for GitHubSource {
    fn name(&self) -> &'static str {
        "github"
    }

    fn validate_plugin_id(&self, plugin_id: &str) -> anyhow::Result<()> {
        // GitHub accepts owner/repo format or single word (for search)
        if plugin_id.is_empty() {
            anyhow::bail!("GitHub repository name cannot be empty");
        }
        // Allow both formats: "owner/repo" or just "name" (for search)
        Ok(())
    }

    async fn resolve_version(
        &self,
        plugin_id: &str,
        requested_version: Option<&str>,
        minecraft_version: Option<&str>,
    ) -> anyhow::Result<ResolvedVersion> {
        // GitHub Releases don't have built-in Minecraft version metadata
        // Note: Warning about Minecraft version compatibility is logged once in lock/sync commands
        // Parse plugin_id - could be owner/repo or just name (for search)
        let parts: Vec<&str> = plugin_id.split('/').collect();
        let (owner, repo) = if parts.len() == 2 && !parts[0].is_empty() && !parts[1].is_empty() {
            // Full format: owner/repo
            (parts[0].to_string(), parts[1].to_string())
        } else if parts.len() == 1 && !parts[0].is_empty() {
            // Single word - search for it
            let search_name = parts[0];
            let found_repo = self.search_repository(search_name).await?;
            (found_repo.owner.login, found_repo.name)
        } else {
            anyhow::bail!(
                "Invalid GitHub repository format. Expected 'owner/repo' or repository name, got '{}'",
                plugin_id
            );
        };

        let owner = owner.as_str();
        let repo = repo.as_str();

        // First verify the repository exists
        let repo_url = format!("https://api.github.com/repos/{}/{}", owner, repo);
        let repo_response = reqwest::get(&repo_url).await?;

        if repo_response.status() == reqwest::StatusCode::NOT_FOUND {
            anyhow::bail!("Repository '{}/{}' not found on GitHub", owner, repo);
        }

        if !repo_response.status().is_success() {
            anyhow::bail!(
                "Failed to fetch GitHub repository '{}/{}': HTTP {}",
                owner,
                repo,
                repo_response.status()
            );
        }

        let release = if let Some(version_str) = requested_version {
            // Get specific release by tag
            let url = format!(
                "https://api.github.com/repos/{}/{}/releases/tags/{}",
                owner, repo, version_str
            );
            let response = reqwest::get(&url).await?;

            if response.status() == reqwest::StatusCode::NOT_FOUND {
                anyhow::bail!(
                    "Release '{}' not found for repository '{}/{}'",
                    version_str,
                    owner,
                    repo
                );
            }

            if !response.status().is_success() {
                anyhow::bail!(
                    "Failed to fetch GitHub release '{}' for '{}/{}': HTTP {}",
                    version_str,
                    owner,
                    repo,
                    response.status()
                );
            }

            response.json::<Release>().await.map_err(|e| {
                anyhow::anyhow!(
                    "Failed to parse GitHub release '{}' for '{}/{}': {}",
                    version_str,
                    owner,
                    repo,
                    e
                )
            })?
        } else {
            // Get latest release
            let url = format!(
                "https://api.github.com/repos/{}/{}/releases/latest",
                owner, repo
            );
            let response = reqwest::get(&url).await?;

            if response.status() == reqwest::StatusCode::NOT_FOUND {
                anyhow::bail!("No releases found for repository '{}/{}'", owner, repo);
            }

            if !response.status().is_success() {
                anyhow::bail!(
                    "Failed to fetch latest GitHub release for '{}/{}': HTTP {}",
                    owner,
                    repo,
                    response.status()
                );
            }

            response.json::<Release>().await.map_err(|e| {
                anyhow::anyhow!(
                    "Failed to parse latest GitHub release for '{}/{}': {}",
                    owner,
                    repo,
                    e
                )
            })?
        };

        // Find the first .jar file in assets
        let jar_asset = release
            .assets
            .iter()
            .find(|a| a.name.ends_with(".jar"))
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "No .jar file found in release '{}' for '{}/{}'",
                    release.tag_name,
                    owner,
                    repo
                )
            })?;

        // Download the file to compute hash (GitHub API doesn't provide checksums)
        let response = reqwest::get(&jar_asset.browser_download_url).await?;
        let data = response.bytes().await?;

        // Compute SHA-256 hash
        let mut hasher = Sha256::new();
        hasher.update(&data);
        let hash_hex = hex::encode(hasher.finalize());
        let hash = format!("sha256:{}", hash_hex);

        Ok(ResolvedVersion {
            version: release.tag_name.clone(),
            filename: jar_asset.name.clone(),
            url: jar_asset.browser_download_url.clone(),
            hash,
        })
    }
}

impl GitHubSource {
    /// Search for a repository by name and return the best match (exact name match, case-insensitive)
    async fn search_repository(&self, search_name: &str) -> anyhow::Result<Repository> {
        // Search for repositories with the name, prioritizing exact matches
        // Use GitHub search API: search for repos with name matching
        let search_query = format!("{} in:name", urlencoding::encode(search_name));
        let search_url = format!(
            "https://api.github.com/search/repositories?q={}&sort=stars&order=desc&per_page=100",
            urlencoding::encode(&search_query)
        );

        let response = reqwest::get(&search_url).await?;

        if !response.status().is_success() {
            anyhow::bail!(
                "Failed to search GitHub repositories: HTTP {}",
                response.status()
            );
        }

        let search_result: SearchResponse = response.json().await?;

        if search_result.items.is_empty() {
            anyhow::bail!("No repositories found matching '{}' on GitHub", search_name);
        }

        // Sort by exact name match (case-insensitive)
        // Exact matches first, then by stars (already sorted by API, but we re-sort for exact matches)
        let mut results = search_result.items;
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
                    // If both or neither are exact, maintain original order (by stars)
                    std::cmp::Ordering::Equal
                }
            }
        });

        // Return the first (best) match
        Ok(results.into_iter().next().unwrap())
    }
}
