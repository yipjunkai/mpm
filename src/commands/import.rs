// Import module for importing existing plugins from /plugins directory

use crate::config;
use crate::constants;
use crate::lockfile::{LockedPlugin, Lockfile};
use crate::manifest::{Manifest, MinecraftSpec, PluginSpec};
use crate::sources::REGISTRY;
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

pub async fn import_plugins() -> anyhow::Result<()> {
    // Check if plugins.toml already exists
    if Manifest::load().is_ok() {
        anyhow::bail!(
            "{} already exists. Remove it first before importing.",
            constants::MANIFEST_FILE
        );
    }

    let plugins_dir = config::plugins_dir();
    let plugins_path = Path::new(&plugins_dir);

    // Check if plugins directory exists
    if !plugins_path.exists() {
        anyhow::bail!("Plugins directory '{}' does not exist", plugins_dir);
    }

    // Scan plugins directory for JAR files
    let plugins = scan_plugins_dir(&plugins_dir)?;

    if plugins.is_empty() {
        println!("No JAR files found in plugins directory");
        // Create empty manifest and lockfile
        let manifest = Manifest {
            minecraft: MinecraftSpec {
                version: constants::DEFAULT_MC_VERSION.to_string(), // Default version
            },
            plugins: BTreeMap::new(),
        };
        manifest.save()?;

        let lockfile = Lockfile::new();
        lockfile.save()?;

        println!(
            "Created empty {} and {}",
            constants::MANIFEST_FILE,
            constants::LOCKFILE_FILE
        );
        return Ok(());
    }

    // Search for sources for each plugin
    let minecraft_version = Some(constants::DEFAULT_MC_VERSION);
    let mut manifest_plugins = BTreeMap::new();
    let mut lockfile_plugins = Vec::new();

    for (name, filename, version_option, hash) in &plugins {
        // Try to find the plugin in sources
        let (source, plugin_id) =
            find_plugin_source(name, version_option.as_deref(), minecraft_version).await;

        manifest_plugins.insert(
            name.clone(),
            PluginSpec {
                source: source.clone(),
                id: plugin_id.clone(),
                version: version_option.clone(),
            },
        );

        lockfile_plugins.push(LockedPlugin {
            name: name.clone(),
            source,
            version: version_option.clone().unwrap_or_else(|| filename.clone()),
            file: filename.clone(),
            url: "unknown://".to_string(),
            hash: hash.clone(),
        });
    }

    let manifest = Manifest {
        minecraft: MinecraftSpec {
            version: constants::DEFAULT_MC_VERSION.to_string(), // Default version, could be detected later
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

    println!("Imported {} plugin(s)", plugins.len());
    for (name, filename, _, _) in &plugins {
        let source = manifest
            .plugins
            .get(name)
            .map(|spec| spec.source.as_str())
            .unwrap_or("unknown");
        if source == "unknown" {
            println!("  → {} ({}) - source: unknown", name, filename);
        } else {
            println!("  → {} ({}) - source: {}", name, filename, source);
        }
    }

    Ok(())
}

/// Search for a plugin across all sources in priority order
/// Returns (source_name, plugin_id)
async fn find_plugin_source(
    plugin_name: &str,
    version: Option<&str>,
    minecraft_version: Option<&str>,
) -> (String, String) {
    let sources = REGISTRY.get_priority_order();

    for source_impl in sources {
        let source_name = source_impl.name();

        // Try the plugin name as-is first
        if source_impl.validate_plugin_id(plugin_name).is_ok() {
            match source_impl
                .resolve_version(plugin_name, version, minecraft_version)
                .await
            {
                Ok(_) => {
                    // Found it!
                    return (source_name.to_string(), plugin_name.to_string());
                }
                Err(_) => {
                    // Continue searching
                }
            }
        }

        // For Hangar and GitHub, try some common transformations
        // Hangar format: author/slug, so if name doesn't have /, skip
        // GitHub format: owner/repo, so if name doesn't have /, skip
        // Modrinth: try lowercase version
        if source_name == "modrinth" {
            let lowercase_name = plugin_name.to_lowercase();
            if lowercase_name != plugin_name
                && source_impl.validate_plugin_id(&lowercase_name).is_ok()
            {
                match source_impl
                    .resolve_version(&lowercase_name, version, minecraft_version)
                    .await
                {
                    Ok(_) => {
                        return (source_name.to_string(), lowercase_name);
                    }
                    Err(_) => {
                        // Continue searching
                    }
                }
            }
        }
    }

    // Not found in any source
    ("unknown".to_string(), plugin_name.to_string())
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
                    eprintln!(
                        "Warning: Could not read plugin.yml from {}: {}",
                        filename, e
                    );
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
                    eprintln!("Warning: Could not compute hash for {}: {}", filename, e);
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
