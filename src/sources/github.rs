// GitHub Releases source implementation

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

pub async fn resolve_version(
    project_id: &str,
    requested_version: Option<&str>,
) -> anyhow::Result<(String, String, String, String)> {
    // Parse project_id as owner/repo
    let parts: Vec<&str> = project_id.split('/').collect();
    if parts.len() != 2 {
        anyhow::bail!(
            "Invalid GitHub repository format. Expected 'owner/repo', got '{}'",
            project_id
        );
    }
    let owner = parts[0];
    let repo = parts[1];

    let release = if let Some(version_str) = requested_version {
        // Get specific release by tag
        let url = format!(
            "https://api.github.com/repos/{}/{}/releases/tags/{}",
            owner, repo, version_str
        );
        reqwest::get(&url)
            .await?
            .json::<Release>()
            .await
            .map_err(|e| {
                anyhow::anyhow!(
                    "Failed to fetch GitHub release '{}' for '{}/{}': {}",
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
        reqwest::get(&url)
            .await?
            .json::<Release>()
            .await
            .map_err(|e| {
                anyhow::anyhow!(
                    "Failed to fetch latest GitHub release for '{}/{}': {}",
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

    Ok((
        release.tag_name.clone(),
        jar_asset.name.clone(),
        jar_asset.browser_download_url.clone(),
        hash,
    ))
}
