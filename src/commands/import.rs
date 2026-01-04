// Import module for importing existing plugins from /plugins directory

use crate::config;
use crate::constants;
use crate::lockfile::{LockedPlugin, Lockfile};
use crate::manifest::{Manifest, MinecraftSpec, PluginSpec};
use crate::sources::REGISTRY;
use log::{debug, info, warn};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

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
    for (name, filename, version_option, hash) in &plugins {
        debug!(
            "Searching for plugin: name={}, filename={}, version={:?}",
            name, filename, version_option
        );

        // Try to find the plugin in sources using search functionality
        match find_plugin_source(name, version_option.as_deref(), minecraft_version).await {
            Some((source, plugin_id)) => {
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

                // Add to lockfile with local file info
                // The URL and hash will be updated when user runs 'lock' command
                // We use the local file hash for now to maintain integrity
                let source_clone = source.clone();
                lockfile_plugins.push(LockedPlugin {
                    name: name.clone(),
                    source,
                    version: version_option.clone().unwrap_or_else(|| filename.clone()),
                    file: filename.clone(),
                    url: format!("{}://{}", source_clone, plugin_id), // Placeholder, will be resolved during lock
                    hash: hash.clone(), // Local file hash, will be updated during lock
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
/// Returns Some((source_name, plugin_id)) if found, None otherwise
async fn find_plugin_source(
    plugin_name: &str,
    version: Option<&str>,
    minecraft_version: Option<&str>,
) -> Option<(String, String)> {
    let sources = REGISTRY.get_priority_order();
    let sources_count = sources.len();

    for source_impl in sources {
        let source_name = source_impl.name();

        debug!(
            "Trying source: plugin={}, source={}",
            plugin_name, source_name
        );

        // Try the plugin name as-is first (this will use search for Hangar/GitHub if needed)
        if source_impl.validate_plugin_id(plugin_name).is_ok() {
            // First try with the exact version from plugin.yml if provided
            let mut resolved = source_impl
                .resolve_version(plugin_name, version, minecraft_version)
                .await;

            // If exact version failed and we have both a version and minecraft_version,
            // try again without the version constraint to find the latest compatible version
            if resolved.is_err() && version.is_some() && minecraft_version.is_some() {
                debug!(
                    "Exact version not compatible, trying latest compatible version: plugin={}, source={}",
                    plugin_name, source_name
                );
                resolved = source_impl
                    .resolve_version(plugin_name, None, minecraft_version)
                    .await;
            }

            match resolved {
                Ok(_) => {
                    debug!(
                        "Plugin found in source: plugin={}, source={}",
                        plugin_name, source_name
                    );

                    // Found it!
                    return Some((source_name.to_string(), plugin_name.to_string()));
                }
                Err(e) => {
                    debug!(
                        "resolve_version failed: plugin={}, source={}, error={}",
                        plugin_name, source_name, e
                    );
                    // Continue searching
                }
            }
        }

        // For Modrinth, try lowercase version
        if source_name == "modrinth" {
            let lowercase_name = plugin_name.to_lowercase();
            if lowercase_name != plugin_name
                && source_impl.validate_plugin_id(&lowercase_name).is_ok()
            {
                debug!(
                    "Trying lowercase variant for Modrinth: plugin={}, lowercase={}",
                    plugin_name, lowercase_name
                );

                // First try with the exact version from plugin.yml if provided
                let mut resolved = source_impl
                    .resolve_version(&lowercase_name, version, minecraft_version)
                    .await;

                // If exact version failed and we have both a version and minecraft_version,
                // try again without the version constraint to find the latest compatible version
                if resolved.is_err() && version.is_some() && minecraft_version.is_some() {
                    debug!(
                        "Exact version not compatible (lowercase), trying latest compatible version: plugin={}, lowercase={}, source={}",
                        plugin_name, lowercase_name, source_name
                    );
                    resolved = source_impl
                        .resolve_version(&lowercase_name, None, minecraft_version)
                        .await;
                }

                match resolved {
                    Ok(_) => {
                        debug!(
                            "Plugin found (lowercase variant): plugin={}, lowercase={}, source={}",
                            plugin_name, lowercase_name, source_name
                        );
                        return Some((source_name.to_string(), lowercase_name));
                    }
                    Err(e) => {
                        debug!(
                            "resolve_version failed (lowercase): lowercase={}, source={}, error={}",
                            lowercase_name, source_name, e
                        );
                        // Continue searching
                    }
                }
            }
        }
    }

    // Not found in any source
    debug!(
        "Plugin not found in any source: plugin={}, sources_tried={}",
        plugin_name, sources_count
    );

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
