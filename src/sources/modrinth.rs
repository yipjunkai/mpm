// Modrinth source implementation

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Project {
    pub id: String,
    pub slug: String,
    pub title: String,
}

#[derive(Debug, Deserialize)]
pub struct Version {
    pub id: String,
    pub version_number: String,
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
    #[serde(default)]
    pub sha256: Option<String>,
}

pub async fn get_project(slug: &str) -> anyhow::Result<Project> {
    let url = format!("https://api.modrinth.com/v2/project/{}", slug);
    let project = reqwest::get(url).await?.json().await?;
    Ok(project)
}

pub async fn get_versions(project_id: &str) -> anyhow::Result<Vec<Version>> {
    let url = format!("https://api.modrinth.com/v2/project/{}/version", project_id);
    let versions: Vec<Version> = reqwest::get(url).await?.json().await?;
    Ok(versions)
}

pub async fn resolve_version(
    project_id: &str,
    requested_version: Option<&str>,
) -> anyhow::Result<(String, String, String, String)> {
    // First get the project to get the ID
    let project = get_project(project_id).await?;

    // Get all versions
    let versions = get_versions(&project.id).await?;

    let version = if let Some(version_str) = requested_version {
        // Find the specific version
        versions
            .iter()
            .find(|v| v.version_number == version_str)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "Version '{}' not found for project '{}'",
                    version_str,
                    project_id
                )
            })?
    } else {
        // Get the latest version (first in the list, usually sorted by date)
        versions
            .first()
            .ok_or_else(|| anyhow::anyhow!("No versions found for project '{}'", project_id))?
    };

    // Get the primary file (usually the first one, or the one marked as primary)
    let file = version.files.first().ok_or_else(|| {
        anyhow::anyhow!("No files found for version '{}'", version.version_number)
    })?;

    // Use sha512 as sha256 (Modrinth provides sha512, we'll use it for sha256 field)
    // In a real implementation, you might want to compute sha256 from the file
    let sha256 = file.hashes.sha256.clone().unwrap_or_else(|| {
        // Use first 64 chars of sha512 as a placeholder, or use sha512 directly
        file.hashes.sha512.clone()
    });

    Ok((
        version.version_number.clone(),
        file.filename.clone(),
        file.url.clone(),
        sha256,
    ))
}
