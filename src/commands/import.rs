// Import module for importing existing plugins from /plugins directory

use crate::config;
use crate::constants;
use crate::lockfile::{LockedPlugin, Lockfile};
use crate::manifest::{Manifest, MinecraftSpec, PluginSpec};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

#[derive(Debug, Deserialize, Serialize)]
struct PluginYml {
    name: Option<String>,
    version: Option<String>,
}

pub fn import_plugins() -> anyhow::Result<()> {
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

    // Create manifest
    let mut manifest_plugins = BTreeMap::new();
    for (name, _, version_option, _) in &plugins {
        manifest_plugins.insert(
            name.clone(),
            PluginSpec {
                source: "unknown".to_string(),
                id: name.clone(),
                version: version_option.clone(),
            },
        );
    }

    let manifest = Manifest {
        minecraft: MinecraftSpec {
            version: constants::DEFAULT_MC_VERSION.to_string(), // Default version, could be detected later
        },
        plugins: manifest_plugins,
    };

    // Create lockfile
    let mut lockfile = Lockfile::new();
    for (name, filename, version_option, hash) in &plugins {
        lockfile.add_plugin(LockedPlugin {
            name: name.clone(),
            source: "unknown".to_string(),
            version: version_option.clone().unwrap_or_else(|| filename.clone()),
            file: filename.clone(),
            url: "unknown://".to_string(),
            hash: hash.clone(),
        });
    }

    // Sort plugins by name
    lockfile.sort_by_name();

    // Save both files
    manifest.save()?;
    lockfile.save()?;

    println!("Imported {} plugin(s)", plugins.len());
    for (name, filename, _, _) in &plugins {
        println!("  → {} ({})", name, filename);
    }

    Ok(())
}

fn scan_plugins_dir(
    plugins_dir: &str,
) -> anyhow::Result<Vec<(String, String, Option<String>, String)>> {
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
