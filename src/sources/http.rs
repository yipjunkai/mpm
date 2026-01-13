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
    if let Some(header) = response
        .headers()
        .get("content-disposition")
        .and_then(|h| h.to_str().ok())
    {
        // Prefer filename*= (RFC 5987) over filename= to avoid MIME-encoded names
        // Example: filename*=UTF-8''Geyser-Spigot.jar
        if let Some(filename) = extract_filename_star(header) {
            return sanitize_filename(&filename);
        }

        // Fall back to basic filename=
        if let Some(filename) = extract_filename_basic(header) {
            return sanitize_filename(&filename);
        }
    }

    // Fall back to extracting from URL
    let filename = url
        .split('/')
        .next_back()
        .unwrap_or("download.jar")
        .split('?')
        .next()
        .unwrap_or("download.jar")
        .to_string();

    sanitize_filename(&filename)
}

/// Extract filename from RFC 5987 filename*= parameter
/// Format: filename*=UTF-8''encoded-filename or filename*=utf-8'en'encoded-filename
fn extract_filename_star(header: &str) -> Option<String> {
    // Find filename*= (case-insensitive)
    let lower = header.to_lowercase();
    let pos = lower.find("filename*=")?;
    let rest = &header[pos + 10..]; // Skip "filename*="

    // Take until semicolon or end
    let value = rest.split(';').next()?.trim();

    // Parse RFC 5987: charset'language'encoded-value
    // The encoded-value is percent-encoded
    let parts: Vec<&str> = value.splitn(3, '\'').collect();
    if parts.len() >= 3 {
        // Decode percent-encoding
        percent_decode(parts[2])
    } else if parts.len() == 1 {
        // No encoding specified, just use the value
        Some(value.trim_matches('"').to_string())
    } else {
        None
    }
}

/// Extract filename from basic filename= parameter
fn extract_filename_basic(header: &str) -> Option<String> {
    // Find filename= but not filename*=
    let lower = header.to_lowercase();

    // Find all occurrences of "filename=" and pick the one that's not "filename*="
    let mut search_start = 0;
    while let Some(pos) = lower[search_start..].find("filename=") {
        let actual_pos = search_start + pos;
        // Check if this is filename*= (preceded by *)
        if actual_pos > 0 && header.as_bytes()[actual_pos - 1] == b'*' {
            search_start = actual_pos + 9;
            continue;
        }

        let rest = &header[actual_pos + 9..]; // Skip "filename="
        let value = rest.split(';').next()?.trim().trim_matches('"');

        // Skip MIME-encoded values (=?...?=) as they may contain invalid chars
        if value.starts_with("=?") && value.ends_with("?=") {
            return None;
        }

        return Some(value.to_string());
    }

    None
}

/// Decode percent-encoded string (RFC 3986)
fn percent_decode(s: &str) -> Option<String> {
    let mut result = Vec::new();
    let bytes = s.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Ok(byte) =
                u8::from_str_radix(std::str::from_utf8(&bytes[i + 1..i + 3]).ok()?, 16)
        {
            result.push(byte);
            i += 3;
            continue;
        }
        result.push(bytes[i]);
        i += 1;
    }

    String::from_utf8(result).ok()
}

/// Sanitize filename for cross-platform compatibility (especially Windows)
fn sanitize_filename(filename: &str) -> String {
    // Windows invalid characters: < > : " / \ | ? *
    // Also handle control characters (0-31)
    filename
        .chars()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect()
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
