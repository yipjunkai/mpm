// Normalized version model for unified version handling

/// Normalized version information from any source
#[derive(Debug, Clone)]
pub struct NormalizedVersion {
    /// Version string (e.g., "1.2.3")
    pub version: String,

    /// Published date/time for sorting (ISO8601 string or unix timestamp as string)
    pub published_at: String,

    /// Compatible Minecraft versions (empty = unknown compatibility)
    pub mc_versions: Vec<String>,

    /// Download information
    pub download: DownloadInfo,
}

/// Download information for a version
#[derive(Debug, Clone)]
pub struct DownloadInfo {
    /// Download URL
    pub url: String,

    /// Filename (if known from API)
    pub filename: Option<String>,

    /// Hash (if provided by API, None = compute after download)
    /// Format: "algorithm:hash" (e.g., "sha256:abc123..." or "sha512:...")
    pub hash: Option<String>,
}

impl NormalizedVersion {
    /// Create a new NormalizedVersion
    #[allow(dead_code)]
    pub fn new(
        version: impl Into<String>,
        published_at: impl Into<String>,
        mc_versions: Vec<String>,
        download: DownloadInfo,
    ) -> Self {
        Self {
            version: version.into(),
            published_at: published_at.into(),
            mc_versions,
            download,
        }
    }

    /// Check if this version has any known MC version compatibility info
    #[allow(dead_code)]
    pub fn has_mc_version_info(&self) -> bool {
        !self.mc_versions.is_empty()
    }
}

impl DownloadInfo {
    /// Create download info with full details (hash provided by API)
    pub fn with_hash(
        url: impl Into<String>,
        filename: impl Into<String>,
        hash: impl Into<String>,
    ) -> Self {
        Self {
            url: url.into(),
            filename: Some(filename.into()),
            hash: Some(hash.into()),
        }
    }

    /// Create download info without hash (needs to be computed)
    pub fn without_hash(url: impl Into<String>, filename: Option<String>) -> Self {
        Self {
            url: url.into(),
            filename,
            hash: None,
        }
    }

    /// Create download info with just a URL (filename and hash need to be determined)
    pub fn url_only(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            filename: None,
            hash: None,
        }
    }
}
