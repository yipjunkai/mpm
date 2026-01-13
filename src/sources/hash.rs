// Hash computation utilities

use anyhow::Result;
use sha2::{Digest, Sha256, Sha512};

/// Hash algorithm types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HashAlgorithm {
    Sha256,
    Sha512,
}

impl HashAlgorithm {
    /// Get the algorithm prefix for formatted output
    pub fn prefix(&self) -> &'static str {
        match self {
            HashAlgorithm::Sha256 => "sha256",
            HashAlgorithm::Sha512 => "sha512",
        }
    }
}

/// Compute hash of data and return formatted string (e.g., "sha256:abc123...")
pub fn compute_hash(data: &[u8], algorithm: HashAlgorithm) -> String {
    let hash_hex = match algorithm {
        HashAlgorithm::Sha256 => {
            let mut hasher = Sha256::new();
            hasher.update(data);
            hex::encode(hasher.finalize())
        }
        HashAlgorithm::Sha512 => {
            let mut hasher = Sha512::new();
            hasher.update(data);
            hex::encode(hasher.finalize())
        }
    };

    format!("{}:{}", algorithm.prefix(), hash_hex)
}

/// Format an existing hash with algorithm prefix
pub fn format_hash(hash: &str, algorithm: HashAlgorithm) -> String {
    format!("{}:{}", algorithm.prefix(), hash)
}

/// Download file and compute hash
/// Returns (formatted_hash, filename, bytes)
#[allow(dead_code)]
pub async fn download_and_hash(url: &str) -> Result<(String, String, Vec<u8>)> {
    let (bytes, filename) = super::http::download_file(url).await?;
    let hash = compute_hash(&bytes, HashAlgorithm::Sha256);
    Ok((hash, filename, bytes))
}

/// Download file with custom response handling and compute hash
/// Returns (formatted_hash, filename, bytes)
#[allow(dead_code)]
pub async fn download_and_hash_with_fallback(
    url: &str,
    default_filename: &str,
) -> Result<(String, String, Vec<u8>)> {
    let response = super::http::download_with_response(url).await?;

    if !response.status().is_success() {
        anyhow::bail!("Download failed: {} ({})", url, response.status());
    }

    let filename = super::http::extract_filename(&response, url);
    let filename = if filename.is_empty() || filename == "download.jar" {
        default_filename.to_string()
    } else {
        filename
    };

    let bytes = response.bytes().await?.to_vec();
    let hash = compute_hash(&bytes, HashAlgorithm::Sha256);

    Ok((hash, filename, bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_sha256() {
        let data = b"hello world";
        let hash = compute_hash(data, HashAlgorithm::Sha256);
        assert!(hash.starts_with("sha256:"));
        assert_eq!(hash.len(), 7 + 64); // "sha256:" + 64 hex chars
    }

    #[test]
    fn test_compute_sha512() {
        let data = b"hello world";
        let hash = compute_hash(data, HashAlgorithm::Sha512);
        assert!(hash.starts_with("sha512:"));
        assert_eq!(hash.len(), 7 + 128); // "sha512:" + 128 hex chars
    }

    #[test]
    fn test_format_hash() {
        let hash = format_hash("abc123", HashAlgorithm::Sha256);
        assert_eq!(hash, "sha256:abc123");
    }
}
