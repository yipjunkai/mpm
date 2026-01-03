// Version matching utility for Minecraft version compatibility

/// Normalize a Minecraft version string for comparison
///
/// Strips build metadata and handles common version formats.
/// Examples:
/// - "1.20.1-R0.1-SNAPSHOT" -> "1.20.1"
/// - "1.20" -> "1.20"
pub fn normalize_mc_version(version: &str) -> String {
    // Remove build metadata (e.g., -R0.1-SNAPSHOT)
    version
        .split('-')
        .next()
        .unwrap_or(version)
        .trim()
        .to_string()
}

/// Check if a Minecraft version matches the target version
///
/// Supports:
/// - Exact match: "1.20.1" == "1.20.1"
/// - Prefix match: "1.20.1" matches "1.20" (for 1.20.x compatibility)
///
/// # Examples
/// ```
/// assert!(matches_mc_version("1.20.1", "1.20.1")); // Exact match
/// assert!(matches_mc_version("1.20.1", "1.20")); // Prefix match
/// assert!(matches_mc_version("1.20-R0.1-SNAPSHOT", "1.20")); // With metadata
/// ```
pub fn matches_mc_version(version: &str, target: &str) -> bool {
    let normalized_version = normalize_mc_version(version);
    let normalized_target = normalize_mc_version(target);

    // Exact match
    if normalized_version == normalized_target {
        return true;
    }

    // Prefix match: version starts with target (e.g., "1.20.1" matches "1.20")
    if normalized_version.starts_with(&normalized_target) {
        // Ensure we're matching at a version boundary (not "1.20" matching "1.2")
        let next_char = normalized_version
            .chars()
            .nth(normalized_target.len())
            .unwrap_or(' ');
        if next_char == '.' || next_char == '-' || next_char == ' ' {
            return true;
        }
    }

    // Reverse prefix match: target starts with version (e.g., "1.20" matches "1.20.1")
    if normalized_target.starts_with(&normalized_version) {
        let next_char = normalized_target
            .chars()
            .nth(normalized_version.len())
            .unwrap_or(' ');
        if next_char == '.' || next_char == '-' || next_char == ' ' {
            return true;
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_mc_version() {
        assert_eq!(normalize_mc_version("1.20.1"), "1.20.1");
        assert_eq!(normalize_mc_version("1.20.1-R0.1-SNAPSHOT"), "1.20.1");
        assert_eq!(normalize_mc_version("1.20"), "1.20");
    }

    #[test]
    fn test_matches_mc_version_exact() {
        assert!(matches_mc_version("1.20.1", "1.20.1"));
        assert!(matches_mc_version("1.20", "1.20"));
    }

    #[test]
    fn test_matches_mc_version_prefix() {
        assert!(matches_mc_version("1.20.1", "1.20"));
        assert!(matches_mc_version("1.20.2", "1.20"));
        assert!(matches_mc_version("1.20", "1.20.1"));
    }

    #[test]
    fn test_matches_mc_version_with_metadata() {
        assert!(matches_mc_version("1.20.1-R0.1-SNAPSHOT", "1.20.1"));
        assert!(matches_mc_version("1.20-R0.1-SNAPSHOT", "1.20"));
        assert!(matches_mc_version("1.20.1", "1.20.1-R0.1-SNAPSHOT"));
    }

    #[test]
    fn test_matches_mc_version_no_match() {
        assert!(!matches_mc_version("1.20.1", "1.21"));
        assert!(!matches_mc_version("1.20", "1.21"));
        assert!(!matches_mc_version("1.2", "1.20")); // Should not match "1.2" with "1.20"
    }
}
