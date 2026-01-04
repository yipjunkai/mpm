// Import module for importing existing plugins from /plugins directory

use crate::config;
use crate::constants;
use crate::lockfile::{LockedPlugin, Lockfile};
use crate::manifest::{Manifest, MinecraftSpec, PluginSpec};
use crate::sources::REGISTRY;
use futures::future::join_all;
use log::{debug, info, warn};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::timeout;

/// Plugin information scanned from the plugins directory
/// Tuple contains: (name, filename, version_option, hash)
type ScannedPlugin = (String, String, Option<String>, String);

#[derive(Debug, Deserialize, Serialize)]
struct PluginYml {
    name: Option<String>,
    version: Option<String>,
}

pub async fn import_plugins(version: Option<String>) -> anyhow::Result<()> {
    // Check if plugins.toml already exists
    if Manifest::load().is_ok() {
        anyhow::bail!(
            "{} already exists. Remove it first before importing.",
            constants::MANIFEST_FILE
        );
    }

    // Determine which version to use
    let final_version = if let Some(v) = version {
        // User provided version explicitly, use it
        v
    } else {
        // Try to detect from Paper JAR
        match detect_minecraft_version_from_paper_jar() {
            Some(detected_version) => {
                info!(
                    "Auto-detected Minecraft version {} from Paper JAR",
                    detected_version
                );
                detected_version
            }
            None => {
                warn!(
                    "Could not detect Minecraft version from Paper JAR, using default: {}",
                    constants::DEFAULT_MC_VERSION
                );
                constants::DEFAULT_MC_VERSION.to_string()
            }
        }
    };

    let plugins_dir = config::plugins_dir();
    let plugins_path = Path::new(&plugins_dir);

    // Check if plugins directory exists
    if !plugins_path.exists() {
        anyhow::bail!("Plugins directory '{}' does not exist", plugins_dir);
    }

    // Scan plugins directory for JAR files
    let plugins = scan_plugins_dir(&plugins_dir)?;

    debug!(
        "Scanned plugins directory: found {} plugin(s)",
        plugins.len()
    );

    if plugins.is_empty() {
        info!("No JAR files found in plugins directory");
        // Create empty manifest and lockfile
        let manifest = Manifest {
            minecraft: MinecraftSpec {
                version: final_version.clone(),
            },
            plugins: BTreeMap::new(),
        };
        manifest.save()?;

        let lockfile = Lockfile::new();
        lockfile.save()?;

        info!(
            "Created empty {} and {}",
            constants::MANIFEST_FILE,
            constants::LOCKFILE_FILE
        );
        return Ok(());
    }

    // Search for sources for each plugin
    let minecraft_version = Some(final_version.as_str());
    let mut manifest_plugins = BTreeMap::new();
    let mut lockfile_plugins = Vec::new();

    let mut skipped_plugins = Vec::new();
    for (name, filename, version_option, _hash) in &plugins {
        debug!(
            "Searching for plugin: name={}, filename={}, version={:?}",
            name, filename, version_option
        );

        // Try to find the plugin in sources using search functionality
        match find_plugin_source(name, version_option.as_deref(), minecraft_version).await {
            Some((source, plugin_id, resolved)) => {
                debug!(
                    "Plugin found in source: name={}, source={}, plugin_id={}",
                    name, source, plugin_id
                );

                manifest_plugins.insert(
                    name.clone(),
                    PluginSpec {
                        source: source.clone(),
                        id: plugin_id.clone(),
                        version: version_option.clone(),
                    },
                );

                // Use resolved URL and hash from the source, but keep local filename
                // since that's what the user actually has in their plugins directory
                lockfile_plugins.push(LockedPlugin {
                    name: name.clone(),
                    source,
                    version: resolved.version.clone(),
                    file: filename.clone(),      // Keep local filename
                    url: resolved.url.clone(),   // Use resolved URL
                    hash: resolved.hash.clone(), // Use resolved hash
                });
            }
            None => {
                debug!(
                    "Plugin not found in any source: name={}, filename={}",
                    name, filename
                );

                // Plugin not found in any source - skip it with a warning
                skipped_plugins.push((name.clone(), filename.clone()));
                warn!(
                    "Plugin '{}' ({}) not found in any source, skipping",
                    name, filename
                );
            }
        }
    }

    let imported_count = manifest_plugins.len();

    let manifest = Manifest {
        minecraft: MinecraftSpec {
            version: final_version.clone(),
        },
        plugins: manifest_plugins,
    };

    // Create lockfile
    let mut lockfile = Lockfile::new();
    for plugin in lockfile_plugins {
        lockfile.add_plugin(plugin);
    }

    // Sort plugins by name
    lockfile.sort_by_name();

    // Save both files
    manifest.save()?;
    lockfile.save()?;

    debug!(
        "Import complete: imported={}, skipped={}",
        imported_count,
        skipped_plugins.len()
    );

    info!("Imported {} plugin(s)", imported_count);
    if !skipped_plugins.is_empty() {
        info!(
            "Skipped {} plugin(s) not found in any source",
            skipped_plugins.len()
        );
    }
    for (name, filename, _, _) in &plugins {
        if let Some(spec) = manifest.plugins.get(name) {
            info!("  → {} ({}) - source: {}", name, filename, spec.source);
        }
    }

    Ok(())
}

/// Search for a plugin across all sources in priority order
/// Returns Some((source_name, plugin_id, resolved_version)) if found, None otherwise
async fn find_plugin_source(
    plugin_name: &str,
    version: Option<&str>,
    minecraft_version: Option<&str>,
) -> Option<(String, String, crate::sources::ResolvedVersion)> {
    let sources = REGISTRY.get_priority_order();
    let timeout_duration = Duration::from_secs(180); // 3 minutes

    // Helper function to create a search future
    async fn search_source(
        source_impl: std::sync::Arc<dyn crate::sources::PluginSource>,
        source_name: &'static str,
        search_id: String,
        version: Option<String>,
        minecraft_version: Option<String>,
        timeout_duration: Duration,
        priority: usize,
    ) -> Result<(String, String, crate::sources::ResolvedVersion, usize), (String, String, usize)>
    {
        debug!(
            "Searching source '{}' for plugin '{}'",
            source_name, search_id
        );

        // First try with the exact version from plugin.yml if provided
        let minecraft_version_ref: Option<&str> = minecraft_version.as_deref();
        let resolved_future =
            source_impl.resolve_version(&search_id, version.as_deref(), minecraft_version_ref);

        let result = timeout(timeout_duration, resolved_future).await;

        match result {
            Ok(Ok(resolved)) => Ok((
                source_name.to_string(),
                search_id.clone(),
                resolved,
                priority,
            )),
            Ok(Err(e)) => {
                // If exact version failed and we have both a version and minecraft_version,
                // try again without the version constraint to find the latest compatible version
                if version.is_some() && minecraft_version.is_some() {
                    debug!(
                        "Exact version not compatible, trying latest compatible version: plugin={}, source={}",
                        search_id, source_name
                    );
                    let minecraft_version_ref: Option<&str> = minecraft_version.as_deref();
                    let retry_future =
                        source_impl.resolve_version(&search_id, None, minecraft_version_ref);
                    let retry_result = timeout(timeout_duration, retry_future).await;
                    match retry_result {
                        Ok(Ok(resolved)) => Ok((
                            source_name.to_string(),
                            search_id.clone(),
                            resolved,
                            priority,
                        )),
                        Ok(Err(_)) | Err(_) => {
                            Err((source_name.to_string(), search_id.clone(), priority))
                        }
                    }
                } else {
                    debug!(
                        "resolve_version failed: plugin={}, source={}, error={}",
                        search_id, source_name, e
                    );
                    Err((source_name.to_string(), search_id.clone(), priority))
                }
            }
            Err(_) => {
                debug!(
                    "Source '{}' timed out for plugin '{}'",
                    source_name, search_id
                );
                Err((source_name.to_string(), search_id.clone(), priority))
            }
        }
    }

    // Create futures for all sources
    let mut futures = Vec::new();
    for (idx, source_impl) in sources.iter().enumerate() {
        let source_name = source_impl.name();
        let plugin_name_str = plugin_name.to_string();
        let source_impl_clone = Arc::clone(source_impl);
        let version_clone = version.map(|s| s.to_string());
        let minecraft_version_clone = minecraft_version.map(|s| s.to_string());
        let timeout_duration_clone = timeout_duration;

        // Add regular search future
        let search_id = plugin_name_str.clone();
        let priority = idx * 2; // Use even numbers for regular searches
        futures.push(search_source(
            source_impl_clone.clone(),
            source_name,
            search_id,
            version_clone,
            minecraft_version_clone,
            timeout_duration_clone,
            priority,
        ));

        // For Modrinth, also try lowercase version
        if source_name == "modrinth" {
            let lowercase_name = plugin_name.to_lowercase();
            if lowercase_name != plugin_name {
                let source_impl_clone_lower = Arc::clone(source_impl);
                let version_clone_lower = version.map(|s| s.to_string());
                let minecraft_version_clone_lower = minecraft_version.map(|s| s.to_string());
                let timeout_duration_clone_lower = timeout_duration;
                let search_id_lower = lowercase_name.clone();
                let priority_lower = idx * 2 + 1; // Use odd numbers for lowercase searches

                futures.push(search_source(
                    source_impl_clone_lower.clone(),
                    source_name,
                    search_id_lower,
                    version_clone_lower,
                    minecraft_version_clone_lower,
                    timeout_duration_clone_lower,
                    priority_lower,
                ));
            }
        }
    }

    // Wait for all searches to complete/timeout
    let results = join_all(futures).await;

    // Find first successful result in priority order (lower priority number = higher priority)
    let mut successful_results: Vec<_> = results
        .into_iter()
        .filter_map(|result| match result {
            Ok((source_name, plugin_id, resolved, priority)) => {
                Some((source_name, plugin_id, resolved, priority))
            }
            Err(_) => None,
        })
        .collect();

    // Sort by priority (lower number = higher priority)
    successful_results.sort_by_key(|(_, _, _, priority)| *priority);

    // Return the first successful result
    if let Some((source_name, plugin_id, resolved, _)) = successful_results.first() {
        debug!(
            "Plugin found in source: plugin={}, source={}, plugin_id={}",
            plugin_name, source_name, plugin_id
        );
        return Some((source_name.to_string(), plugin_id.clone(), resolved.clone()));
    }

    // Not found in any source
    debug!("Plugin not found in any source: plugin={}", plugin_name);

    None
}

fn scan_plugins_dir(plugins_dir: &str) -> anyhow::Result<Vec<ScannedPlugin>> {
    let plugins_path = Path::new(plugins_dir);
    let mut plugins = Vec::new();

    let entries = fs::read_dir(plugins_path)?;
    for entry in entries {
        let entry = entry?;
        let path = entry.path();

        // Only process .jar files
        if path.is_file()
            && let Some(ext) = path.extension()
            && ext == "jar"
        {
            let filename = path
                .file_name()
                .and_then(|n| n.to_str())
                .ok_or_else(|| anyhow::anyhow!("Invalid filename"))?
                .to_string();

            // Try to read plugin.yml from JAR
            let (name, version) = match read_plugin_yml_from_jar(&path) {
                Ok((n, v)) => (n, v),
                Err(e) => {
                    warn!("Could not read plugin.yml from {}: {}", filename, e);
                    // Fallback to filename without .jar extension
                    let fallback_name = filename
                        .strip_suffix(".jar")
                        .unwrap_or(&filename)
                        .to_string();
                    (fallback_name, None)
                }
            };

            // Compute SHA-256 hash
            let hash = match compute_sha256(&path) {
                Ok(h) => h,
                Err(e) => {
                    warn!("Could not compute hash for {}: {}", filename, e);
                    continue; // Skip this plugin if hash computation fails
                }
            };

            plugins.push((name, filename, version, hash));
        }
    }

    Ok(plugins)
}

fn read_plugin_yml_from_jar(jar_path: &Path) -> anyhow::Result<(String, Option<String>)> {
    use std::io::Read;

    // Open JAR file as ZIP archive
    let file = fs::File::open(jar_path)?;
    let mut archive = zip::ZipArchive::new(file)?;

    // Look for plugin.yml in the root of the JAR
    // Try plugin.yml first, then bungee.yml for BungeeCord plugins
    let yml_name = {
        let _test = archive.by_name("plugin.yml");
        if _test.is_ok() {
            "plugin.yml"
        } else {
            "bungee.yml"
        }
    };
    let mut plugin_yml = archive.by_name(yml_name)?;

    // Read the contents
    let mut contents = String::new();
    plugin_yml.read_to_string(&mut contents)?;

    // Parse YAML
    let plugin_data: PluginYml = serde_yaml::from_str(&contents)
        .map_err(|e| anyhow::anyhow!("Failed to parse plugin.yml: {}", e))?;

    let name = plugin_data
        .name
        .ok_or_else(|| anyhow::anyhow!("plugin.yml missing 'name' field"))?;

    let version = plugin_data.version;

    Ok((name, version))
}

fn compute_sha256(file_path: &Path) -> anyhow::Result<String> {
    let data = fs::read(file_path)?;
    let mut hasher = Sha256::new();
    hasher.update(&data);
    let hash_hex = hex::encode(hasher.finalize());
    Ok(format!("sha256:{}", hash_hex))
}

/// Detect Minecraft version from Paper JAR file in the configuration directory
/// Returns None if no Paper JAR is found or version cannot be extracted
pub fn detect_minecraft_version_from_paper_jar() -> Option<String> {
    let config_dir = config::config_dir();
    let config_path = Path::new(&config_dir);

    if !config_path.exists() {
        debug!("Config directory does not exist: {}", config_dir);
        return None;
    }

    // Search for Paper JAR files (paper-*.jar or papermc-*.jar)
    let entries = match fs::read_dir(config_path) {
        Ok(entries) => entries,
        Err(e) => {
            debug!("Failed to read config directory {}: {}", config_dir, e);
            return None;
        }
    };

    for entry in entries {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };

        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        let filename = match path.file_name().and_then(|n| n.to_str()) {
            Some(name) => name,
            None => continue,
        };

        // Check if it's a Paper JAR file
        if !filename.ends_with(".jar") {
            continue;
        }

        let filename_lower = filename.to_lowercase();
        if !filename_lower.starts_with("paper") {
            continue;
        }

        debug!("Found potential Paper JAR: {}", filename);

        // Try to extract version from filename first (e.g., paper-1.20.6-150.jar -> 1.20.6)
        if let Some(version) = extract_version_from_filename(filename) {
            debug!("Extracted version from filename: {}", version);
            return Some(version);
        }

        // Try to read from MANIFEST.MF
        if let Some(version) = extract_version_from_manifest(&path) {
            debug!("Extracted version from MANIFEST.MF: {}", version);
            return Some(version);
        }
    }

    debug!("No Paper JAR found or version could not be extracted");
    None
}

/// Extract Minecraft version from Paper JAR filename
/// Patterns:
///   - paper-{version}-{build}.jar (e.g., paper-1.20.6-150.jar -> 1.20.6)
///   - paper-{version}.jar (e.g., paper-1.20.6.jar -> 1.20.6)
fn extract_version_from_filename(filename: &str) -> Option<String> {
    // Remove .jar extension
    let name = filename.strip_suffix(".jar")?;

    // Pattern: paper-{version}-{build} or papermc-{version}-{build} or paper-{version}
    // We want to extract the version part
    let parts: Vec<&str> = name.split('-').collect();

    // Need at least 2 parts: paper, version (build number is optional)
    if parts.len() < 2 {
        return None;
    }

    // If we have 3+ parts, assume the last part is a build number
    // Otherwise, everything after "paper" is the version
    let version_parts = if parts.len() >= 3 {
        // Skip the first part (paper/papermc) and last part (build number)
        // Join the middle parts as version (e.g., "1.20.6")
        &parts[1..parts.len() - 1]
    } else {
        // No build number, everything after "paper" is the version
        &parts[1..]
    };

    let version = version_parts.join(".");

    // Validate it looks like a version
    if version.is_empty() {
        return None;
    }

    // Basic validation: should start with a digit
    if !version.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        return None;
    }

    Some(version)
}

/// Extract Minecraft version from JAR MANIFEST.MF file
fn extract_version_from_manifest(jar_path: &Path) -> Option<String> {
    use std::io::Read;

    let file = match fs::File::open(jar_path) {
        Ok(f) => f,
        Err(e) => {
            debug!("Failed to open JAR file {:?}: {}", jar_path, e);
            return None;
        }
    };

    let mut archive = match zip::ZipArchive::new(file) {
        Ok(a) => a,
        Err(e) => {
            debug!("Failed to open JAR archive {:?}: {}", jar_path, e);
            return None;
        }
    };

    let mut manifest = match archive.by_name("META-INF/MANIFEST.MF") {
        Ok(m) => m,
        Err(e) => {
            debug!("Failed to read MANIFEST.MF from {:?}: {}", jar_path, e);
            return None;
        }
    };

    let mut contents = String::new();
    if manifest.read_to_string(&mut contents).is_err() {
        debug!("Failed to read MANIFEST.MF contents from {:?}", jar_path);
        return None;
    }

    // Parse manifest file (simple key-value format)
    // Look for Implementation-Version or Specification-Version
    for line in contents.lines() {
        let line = line.trim();

        // Skip continuation lines (start with space)
        if line.starts_with(' ') {
            continue;
        }

        // Look for version-related keys
        if let Some(version) = line.strip_prefix("Implementation-Version:") {
            let version = version.trim();
            if !version.is_empty() {
                // Try to normalize version (remove build metadata like -R0.1-SNAPSHOT)
                let normalized = version
                    .split('-')
                    .next()
                    .unwrap_or(version)
                    .trim()
                    .to_string();
                if !normalized.is_empty() {
                    return Some(normalized);
                }
            }
        } else if let Some(version) = line.strip_prefix("Specification-Version:") {
            let version = version.trim();
            if !version.is_empty() {
                let normalized = version
                    .split('-')
                    .next()
                    .unwrap_or(version)
                    .trim()
                    .to_string();
                if !normalized.is_empty() {
                    return Some(normalized);
                }
            }
        }
    }

    None
}
