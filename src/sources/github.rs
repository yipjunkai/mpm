// GitHub Releases source implementation

use crate::sources::hash::{self, HashAlgorithm};
use crate::sources::http;
use crate::sources::search::{self, ParsedId, Searchable};
use crate::sources::source_trait::{PluginSource, ResolvedVersion};
use async_trait::async_trait;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Release {
    #[serde(rename = "tag_name")]
    tag_name: String,
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
}

// Implement Searchable for Repository
impl Searchable for Repository {
    fn search_name(&self) -> &str {
        &self.name
    }
}

pub struct GitHubSource;

impl GitHubSource {
    /// Search for a repository by name
    async fn search_repository(&self, search_name: &str) -> anyhow::Result<(String, String)> {
        let search_query = format!("{} in:name", urlencoding::encode(search_name));
        let search_url = format!(
            "https://api.github.com/search/repositories?q={}&sort=stars&order=desc&per_page=100",
            urlencoding::encode(&search_query)
        );

        let search_result: SearchResponse = http::fetch_json(&search_url).await?;

        if search_result.items.is_empty() {
            anyhow::bail!("No repositories found matching '{}' on GitHub", search_name);
        }

        let mut results = search_result.items;
        search::rank_search_results_stable(&mut results, search_name);

        let repo = results.into_iter().next().unwrap();
        Ok((repo.owner.login, repo.name))
    }

    /// Parse plugin ID and resolve to owner/repo
    async fn resolve_repo_id(&self, plugin_id: &str) -> anyhow::Result<(String, String)> {
        match search::parse_owner_name_id(plugin_id) {
            ParsedId::Full { owner, name } => Ok((owner, name)),
            ParsedId::SearchTerm(_) => self.search_repository(plugin_id).await,
        }
    }

    /// Fetch release by tag or latest
    async fn fetch_release(
        owner: &str,
        repo: &str,
        requested_version: Option<&str>,
    ) -> anyhow::Result<Release> {
        let url = if let Some(version) = requested_version {
            format!(
                "https://api.github.com/repos/{}/{}/releases/tags/{}",
                owner, repo, version
            )
        } else {
            format!(
                "https://api.github.com/repos/{}/{}/releases/latest",
                owner, repo
            )
        };

        http::fetch_json(&url).await.map_err(|_| {
            if let Some(version) = requested_version {
                anyhow::anyhow!(
                    "Release '{}' not found for repository '{}/{}'",
                    version,
                    owner,
                    repo
                )
            } else {
                anyhow::anyhow!("No releases found for repository '{}/{}'", owner, repo)
            }
        })
    }
}

#[async_trait]
impl PluginSource for GitHubSource {
    fn name(&self) -> &'static str {
        "github"
    }

    fn validate_plugin_id(&self, plugin_id: &str) -> anyhow::Result<()> {
        if plugin_id.is_empty() {
            anyhow::bail!("GitHub repository name cannot be empty");
        }
        Ok(())
    }

    async fn resolve_version(
        &self,
        plugin_id: &str,
        requested_version: Option<&str>,
        _minecraft_version: Option<&str>,
    ) -> anyhow::Result<ResolvedVersion> {
        // GitHub Releases don't have built-in Minecraft version metadata
        // Resolve plugin ID to owner/repo
        let (owner, repo) = self.resolve_repo_id(plugin_id).await?;

        // Verify repository exists
        let repo_url = format!("https://api.github.com/repos/{}/{}", owner, repo);
        http::fetch_json::<Repository>(&repo_url)
            .await
            .map_err(|_| anyhow::anyhow!("Repository '{}/{}' not found on GitHub", owner, repo))?;

        // Fetch release
        let release = Self::fetch_release(&owner, &repo, requested_version).await?;

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

        // Download the file to compute hash
        let response = http::download_with_response(&jar_asset.browser_download_url).await?;
        let data = response.bytes().await?;
        let hash_str = hash::compute_hash(&data, HashAlgorithm::Sha256);

        Ok(ResolvedVersion {
            version: release.tag_name,
            filename: jar_asset.name.clone(),
            url: jar_asset.browser_download_url.clone(),
            hash: hash_str,
        })
    }
}
