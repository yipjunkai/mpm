// Modrinth source implementation

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Project {
    pub id: String,
    pub slug: String,
    pub title: String,
}

pub async fn get_project(slug: &str) -> anyhow::Result<Project> {
    let url = format!("https://api.modrinth.com/v2/project/{}", slug);

    let project = reqwest::get(url).await?.json().await?;
    Ok(project)
}
