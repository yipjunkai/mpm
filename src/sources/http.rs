// Shared HTTP client utilities

use anyhow::Result;
use reqwest::{Client, Response, StatusCode};
use serde::de::DeserializeOwned;

/// User-Agent string for all HTTP requests
const USER_AGENT: &str = concat!("mpm/", env!("CARGO_PKG_VERSION"));

lazy_static::lazy_static! {
    /// Shared HTTP client with proper User-Agent
    static ref CLIENT: Client = Client::builder()
        .user_agent(USER_AGENT)
        .build()
        .expect("Failed to create HTTP client");
}

/// Get a reference to the shared HTTP client
pub fn client() -> &'static Client {
    &CLIENT
}

/// Fetch JSON from a URL and deserialize it
pub async fn fetch_json<T: DeserializeOwned>(url: &str) -> Result<T> {
    let response: Response = CLIENT.get(url).send().await?;

    if response.status() == StatusCode::NOT_FOUND {
        anyhow::bail!("Resource not found: {}", url);
    }

    if !response.status().is_success() {
        anyhow::bail!("HTTP request failed: {} ({})", url, response.status());
    }

    let result = response.json().await?;
    Ok(result)
}

/// Fetch JSON from a URL, returning None for 404 errors
#[allow(dead_code)]
pub async fn fetch_json_optional<T: DeserializeOwned>(url: &str) -> Result<Option<T>> {
    let response: Response = CLIENT.get(url).send().await?;

    if response.status() == StatusCode::NOT_FOUND {
        return Ok(None);
    }

    if !response.status().is_success() {
        anyhow::bail!("HTTP request failed: {} ({})", url, response.status());
    }

    let result = response.json().await?;
    Ok(Some(result))
}

/// Fetch raw bytes from a URL
#[allow(dead_code)]
pub async fn fetch_bytes(url: &str) -> Result<Vec<u8>> {
    let response: Response = CLIENT.get(url).send().await?;

    if !response.status().is_success() {
        anyhow::bail!("HTTP request failed: {} ({})", url, response.status());
    }

    let bytes = response.bytes().await?;
    Ok(bytes.to_vec())
}

/// Download a file and return (bytes, filename)
/// Extracts filename from Content-Disposition header or URL
#[allow(dead_code)]
pub async fn download_file(url: &str) -> Result<(Vec<u8>, String)> {
    let response: Response = CLIENT.get(url).send().await?;

    if !response.status().is_success() {
        anyhow::bail!("Download failed: {} ({})", url, response.status());
    }

    let filename = extract_filename(&response, url);
    let bytes = response.bytes().await?;

    Ok((bytes.to_vec(), filename))
}

/// Download a file with full response access for custom handling
pub async fn download_with_response(url: &str) -> Result<Response> {
    let response: Response = CLIENT.get(url).send().await?;
    Ok(response)
}

/// Extract filename from Content-Disposition header or URL
pub fn extract_filename(response: &Response, url: &str) -> String {
    // Try Content-Disposition header first
    if let Some(filename) = response
        .headers()
        .get("content-disposition")
        .and_then(|h| h.to_str().ok())
        .and_then(|s| {
            s.split("filename=")
                .nth(1)
                .and_then(|f| f.trim_matches('"').split(';').next())
                .map(|f| f.trim_matches('"').to_string())
        })
    {
        return filename;
    }

    // Fall back to extracting from URL
    url.split('/')
        .next_back()
        .unwrap_or("download.jar")
        .split('?')
        .next()
        .unwrap_or("download.jar")
        .to_string()
}

/// Check if a response has a specific content type
#[allow(dead_code)]
pub fn has_content_type(response: &Response, expected: &str) -> bool {
    response
        .headers()
        .get("content-type")
        .and_then(|h| h.to_str().ok())
        .map(|ct| ct.to_lowercase().contains(expected))
        .unwrap_or(false)
}

/// Get content type from response
pub fn get_content_type(response: &Response) -> Option<String> {
    response
        .headers()
        .get("content-type")
        .and_then(|h| h.to_str().ok())
        .map(|s| s.to_lowercase())
}
