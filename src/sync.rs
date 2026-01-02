// Sync module for synchronizing plugins directory with lockfile

use crate::config;
use crate::lockfile::{LockedPlugin, Lockfile};
use sha2::{Digest, Sha256, Sha512};
use std::fs;
use std::path::Path;

pub async fn sync_plugins() -> anyhow::Result<()> {
    // Load lockfile
    let lockfile = Lockfile::load()
        .map_err(|_| anyhow::anyhow!("Lockfile not found. Run 'pm lock' first."))?;

    let plugins_dir = config::config_dir();
    let staging_dir = format!("{}/.plugins.staging", plugins_dir);
    let backup_dir = format!("{}/.plugins.backup", plugins_dir);

    // Clean up any leftover staging/backup directories
    cleanup_temp_dirs(&plugins_dir)?;

    // Create staging directory
    fs::create_dir_all(&staging_dir)?;

    // Create backup of current plugins directory
    let _backup_created = create_backup(&plugins_dir, &backup_dir)?;

    // Track if we need to restore on error
    let mut needs_restore = false;

    let result = async {
        needs_restore = true;

        // Get list of managed plugin filenames
        let managed_files: std::collections::HashSet<String> =
            lockfile.plugin.iter().map(|p| p.file.clone()).collect();

        // Track which files need to be downloaded
        let mut files_to_download = Vec::new();

        for plugin in &lockfile.plugin {
            let target_path = Path::new(&plugins_dir).join(&plugin.file);

            // Check if file already exists with correct hash and filename
            if target_path.exists() {
                // Parse hash to get algorithm
                let (algorithm, _) = plugin.parse_hash()?;
                if let Ok(existing_hash) = verify_plugin_hash(&target_path, algorithm) {
                    if existing_hash == plugin.hash {
                        println!("  ✓ {} (already synced)", plugin.name);
                        continue;
                    }
                }
            }

            files_to_download.push(plugin);
        }

        // Download files that need updating
        for plugin in files_to_download {
            let staging_path = Path::new(&staging_dir).join(&plugin.file);
            println!("  → Downloading {}...", plugin.name);
            download_and_verify(plugin, &staging_path).await?;
            println!("  ✓ {} verified", plugin.name);
        }

        // Remove unmanaged .jar files
        remove_unmanaged_files(&plugins_dir, &managed_files)?;

        // Atomically replace plugins
        atomic_replace(&plugins_dir, &staging_dir, &backup_dir)?;

        needs_restore = false;
        Ok::<(), anyhow::Error>(())
    }
    .await;

    // Cleanup and restore on error
    if result.is_err() && needs_restore {
        restore_backup(&plugins_dir, &backup_dir)?;
    }

    // Clean up staging and backup directories
    cleanup_temp_dirs(&plugins_dir)?;

    result?;

    println!("Synced {} plugin(s)", lockfile.plugin.len());
    Ok(())
}

pub fn verify_plugin_hash(file_path: &Path, algorithm: &str) -> anyhow::Result<String> {
    let data = fs::read(file_path)?;
    let hash_hex = match algorithm {
        "sha256" => {
            let mut hasher = Sha256::new();
            hasher.update(&data);
            hex::encode(hasher.finalize())
        }
        "sha512" => {
            let mut hasher = Sha512::new();
            hasher.update(&data);
            hex::encode(hasher.finalize())
        }
        _ => anyhow::bail!("Unsupported hash algorithm: {}", algorithm),
    };
    Ok(format!("{}:{}", algorithm, hash_hex))
}

async fn download_and_verify(plugin: &LockedPlugin, target_path: &Path) -> anyhow::Result<()> {
    // Download file
    let response = reqwest::get(&plugin.url).await?;
    let data = response.bytes().await?;

    // Parse hash to get algorithm and expected hash
    let (algorithm, expected_hash) = plugin.parse_hash()?;

    // Compute hash using the correct algorithm
    let computed_hash = match algorithm {
        "sha256" => {
            let mut hasher = Sha256::new();
            hasher.update(&data);
            hex::encode(hasher.finalize())
        }
        "sha512" => {
            let mut hasher = Sha512::new();
            hasher.update(&data);
            hex::encode(hasher.finalize())
        }
        _ => anyhow::bail!("Unsupported hash algorithm: {}", algorithm),
    };

    // Compare computed hash with expected hash
    if computed_hash != expected_hash {
        anyhow::bail!(
            "Hash mismatch for {}: expected {}:{}, got {}:{}",
            plugin.name,
            algorithm,
            expected_hash,
            algorithm,
            computed_hash
        );
    }

    // Write to staging
    if let Some(parent) = target_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(target_path, &data)?;

    Ok(())
}

fn create_backup(plugins_dir: &str, backup_dir: &str) -> anyhow::Result<bool> {
    let plugins_path = Path::new(plugins_dir);
    if !plugins_path.exists() {
        return Ok(false);
    }

    // Create backup of existing .jar files
    fs::create_dir_all(backup_dir)?;
    let mut backed_up = false;

    if let Ok(entries) = fs::read_dir(plugins_path) {
        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("jar") {
                let filename = path.file_name().unwrap();
                let backup_path = Path::new(backup_dir).join(filename);
                fs::copy(&path, &backup_path)?;
                backed_up = true;
            }
        }
    }

    Ok(backed_up)
}

fn restore_backup(plugins_dir: &str, backup_dir: &str) -> anyhow::Result<()> {
    let backup_path = Path::new(backup_dir);
    if !backup_path.exists() {
        return Ok(());
    }

    println!("Restoring from backup...");

    // Remove current .jar files
    let plugins_path = Path::new(plugins_dir);
    if plugins_path.exists() {
        if let Ok(entries) = fs::read_dir(plugins_path) {
            for entry in entries {
                let entry = entry?;
                let path = entry.path();
                if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("jar") {
                    fs::remove_file(&path)?;
                }
            }
        }
    }

    // Restore from backup
    if let Ok(entries) = fs::read_dir(backup_path) {
        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() {
                let filename = path.file_name().unwrap();
                let target_path = plugins_path.join(filename);
                fs::copy(&path, &target_path)?;
            }
        }
    }

    Ok(())
}

fn atomic_replace(plugins_dir: &str, staging_dir: &str, _backup_dir: &str) -> anyhow::Result<()> {
    let plugins_path = Path::new(plugins_dir);
    let staging_path = Path::new(staging_dir);

    // Get list of files in staging (these are the ones we downloaded)
    let mut staged_files = std::collections::HashSet::new();
    if staging_path.exists() {
        if let Ok(entries) = fs::read_dir(staging_path) {
            for entry in entries {
                let entry = entry?;
                let path = entry.path();
                if path.is_file() {
                    if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
                        staged_files.insert(filename.to_string());
                    }
                }
            }
        }
    }

    // Remove .jar files that are being replaced (exist in staging)
    if plugins_path.exists() {
        if let Ok(entries) = fs::read_dir(plugins_path) {
            for entry in entries {
                let entry = entry?;
                let path = entry.path();
                // Only remove .jar files that are being replaced, preserve plugins.toml and plugins.lock
                if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("jar") {
                    if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
                        if staged_files.contains(filename) {
                            fs::remove_file(&path)?;
                        }
                    }
                }
            }
        }
    }

    // Copy verified files from staging to plugins directory
    if staging_path.exists() {
        if let Ok(entries) = fs::read_dir(staging_path) {
            for entry in entries {
                let entry = entry?;
                let path = entry.path();
                if path.is_file() {
                    let filename = path.file_name().unwrap();
                    let target_path = plugins_path.join(filename);
                    fs::copy(&path, &target_path)?;
                }
            }
        }
    }

    Ok(())
}

fn remove_unmanaged_files(
    plugins_dir: &str,
    managed_files: &std::collections::HashSet<String>,
) -> anyhow::Result<()> {
    let plugins_path = Path::new(plugins_dir);
    if !plugins_path.exists() {
        return Ok(());
    }

    if let Ok(entries) = fs::read_dir(plugins_path) {
        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() {
                if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
                    // Only remove .jar files that aren't managed
                    if filename.ends_with(".jar") && !managed_files.contains(filename) {
                        println!("  → Removing unmanaged file: {}", filename);
                        fs::remove_file(&path)?;
                    }
                }
            }
        }
    }

    Ok(())
}

fn cleanup_temp_dirs(plugins_dir: &str) -> anyhow::Result<()> {
    let staging_dir = format!("{}/.plugins.staging", plugins_dir);
    let backup_dir = format!("{}/.plugins.backup", plugins_dir);

    if Path::new(&staging_dir).exists() {
        fs::remove_dir_all(&staging_dir)?;
    }

    if Path::new(&backup_dir).exists() {
        fs::remove_dir_all(&backup_dir)?;
    }

    Ok(())
}
