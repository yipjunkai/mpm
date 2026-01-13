// Sync module for synchronizing plugins directory with lockfile

use crate::config;
use crate::lockfile::{LockedPlugin, Lockfile};
use crate::ui;
use log::debug;
use sha2::{Digest, Sha256, Sha512};
use std::fs;
use std::path::Path;

pub async fn sync_plugins(dry_run: bool) -> anyhow::Result<i32> {
    // Exit codes:
    // 0 = healthy, no issues
    // 1 = warnings only (changes detected in dry-run)
    // 2 = errors present

    // Load lockfile
    let lockfile = match Lockfile::load() {
        Ok(lockfile) => lockfile,
        Err(_) => {
            ui::error("Lockfile not found. Run 'mpm lock' first.");
            return Ok(2);
        }
    };

    // Check if there are any GitHub plugins and warn once about version compatibility
    let has_github_plugins = lockfile.plugin.iter().any(|p| p.source == "github");
    if has_github_plugins {
        ui::warning(
            "GitHub source does not support Minecraft version filtering. \
            Compatibility cannot be verified for GitHub plugins.",
        );
    }

    let plugins_dir = config::plugins_dir();

    if dry_run {
        ui::status("[DRY RUN]", "Previewing sync changes...");
    }

    let staging_dir = format!("{}/.plugins.staging", plugins_dir);
    let backup_dir = format!("{}/.plugins.backup", plugins_dir);

    // Clean up any leftover staging/backup directories
    if !dry_run && let Err(e) = cleanup_temp_dirs(&plugins_dir) {
        ui::error(&format!("Failed to cleanup temp directories: {}", e));
        return Ok(2);
    }

    // Create staging directory
    if !dry_run && let Err(e) = fs::create_dir_all(&staging_dir) {
        ui::error(&format!("Failed to create staging directory: {}", e));
        return Ok(2);
    }

    // Create backup of current plugins directory
    let _backup_created = if !dry_run {
        match create_backup(&plugins_dir, &backup_dir) {
            Ok(created) => created,
            Err(e) => {
                ui::error(&format!("Failed to create backup: {}", e));
                return Ok(2);
            }
        }
    } else {
        false
    };

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
                if let Ok(existing_hash) = verify_plugin_hash(&target_path, algorithm)
                    && existing_hash == plugin.hash
                {
                    debug!("  ✓ {} (already synced)", plugin.name);
                    continue;
                }
            }

            files_to_download.push(plugin);
        }

        // Track if there are changes (for exit code)
        let mut has_changes = !files_to_download.is_empty();

        // Download files that need updating
        for plugin in files_to_download {
            if dry_run {
                ui::action(&format!("Would download {}", plugin.name));
            } else {
                let staging_path = Path::new(&staging_dir).join(&plugin.file);
                download_and_verify_with_progress(plugin, &staging_path).await?;
            }
        }

        // Remove unmanaged .jar files
        if dry_run {
            // Just preview what would be removed
            let plugins_path = Path::new(&plugins_dir);
            if plugins_path.exists()
                && let Ok(entries) = fs::read_dir(plugins_path)
            {
                for entry in entries {
                    let entry = entry?;
                    let path = entry.path();
                    if path.is_file()
                        && let Some(filename) = path.file_name().and_then(|n| n.to_str())
                        && filename.ends_with(".jar")
                        && !managed_files.contains(filename)
                    {
                        ui::action(&format!("Would remove unmanaged file: {}", filename));
                        has_changes = true;
                    }
                }
            }
        } else {
            let unmanaged_removed = remove_unmanaged_files(&plugins_dir, &managed_files)?;
            has_changes = has_changes || unmanaged_removed;
        }

        // Atomically replace plugins
        if !dry_run {
            atomic_replace(&plugins_dir, &staging_dir, &backup_dir)?;
        }

        needs_restore = false;
        Ok::<bool, anyhow::Error>(has_changes)
    }
    .await;

    // Handle result and cleanup
    let has_changes = match result {
        Ok(changes) => changes,
        Err(e) => {
            // Error occurred - cleanup and return exit code 2
            ui::error(&e.to_string());

            // Cleanup and restore on error
            if !dry_run
                && needs_restore
                && let Err(restore_err) = restore_backup(&plugins_dir, &backup_dir)
            {
                ui::warning(&format!("Failed to restore backup: {}", restore_err));
            }

            // Clean up staging and backup directories
            if !dry_run {
                let _ = cleanup_temp_dirs(&plugins_dir);
            }

            return Ok(2);
        }
    };

    // Clean up staging and backup directories
    if !dry_run && let Err(e) = cleanup_temp_dirs(&plugins_dir) {
        ui::warning(&format!("Failed to cleanup temp directories: {}", e));
        // Don't fail on cleanup, but log it
    }

    if dry_run {
        ui::dim(&format!("Would sync {} plugin(s)", lockfile.plugin.len()));
        // Return exit code: 0 = no changes, 1 = changes detected
        Ok(if has_changes { 1 } else { 0 })
    } else {
        ui::success(&format!("Synced {} plugin(s)", lockfile.plugin.len()));
        Ok(0) // Success
    }
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

async fn download_and_verify_with_progress(
    plugin: &LockedPlugin,
    target_path: &Path,
) -> anyhow::Result<()> {
    // Create spinner for download
    let pb = ui::spinner(&format!("Downloading {}...", plugin.name));

    // Download file
    let response = reqwest::get(&plugin.url).await?;

    // Get content length for progress (if available)
    let total_size = response.content_length();

    // Update progress bar if we have size info
    if let Some(size) = total_size {
        pb.set_length(size);
        pb.set_style(
            indicatif::ProgressStyle::default_bar()
                .template("{spinner:.cyan} {msg} [{bar:25.cyan/dim}] {bytes}/{total_bytes}")
                .unwrap()
                .progress_chars("━━╺"),
        );
    }

    let data = response.bytes().await?;
    pb.set_position(data.len() as u64);

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
        _ => {
            ui::finish_spinner_error(&pb, &format!("{}: unsupported hash algorithm", plugin.name));
            anyhow::bail!("Unsupported hash algorithm: {}", algorithm);
        }
    };

    // Compare computed hash with expected hash
    if computed_hash != expected_hash {
        ui::finish_spinner_error(&pb, &format!("{}: hash mismatch", plugin.name));
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

    ui::finish_download_success(&pb, &plugin.name);

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

    ui::action("Restoring from backup...");

    // Remove current .jar files
    let plugins_path = Path::new(plugins_dir);
    if plugins_path.exists()
        && let Ok(entries) = fs::read_dir(plugins_path)
    {
        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("jar") {
                fs::remove_file(&path)?;
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
    if staging_path.exists()
        && let Ok(entries) = fs::read_dir(staging_path)
    {
        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            if path.is_file()
                && let Some(filename) = path.file_name().and_then(|n| n.to_str())
            {
                staged_files.insert(filename.to_string());
            }
        }
    }

    // Remove .jar files that are being replaced (exist in staging)
    if plugins_path.exists()
        && let Ok(entries) = fs::read_dir(plugins_path)
    {
        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            // Only remove .jar files that are being replaced, preserve manifest and lockfile
            if path.is_file()
                && path.extension().and_then(|s| s.to_str()) == Some("jar")
                && let Some(filename) = path.file_name().and_then(|n| n.to_str())
                && staged_files.contains(filename)
            {
                fs::remove_file(&path)?;
            }
        }
    }

    // Copy verified files from staging to plugins directory
    if staging_path.exists()
        && let Ok(entries) = fs::read_dir(staging_path)
    {
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

fn remove_unmanaged_files(
    plugins_dir: &str,
    managed_files: &std::collections::HashSet<String>,
) -> anyhow::Result<bool> {
    let plugins_path = Path::new(plugins_dir);
    if !plugins_path.exists() {
        return Ok(false);
    }

    let mut removed_any = false;
    if let Ok(entries) = fs::read_dir(plugins_path) {
        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            if path.is_file()
                && let Some(filename) = path.file_name().and_then(|n| n.to_str())
            {
                // Only remove .jar files that aren't managed
                if filename.ends_with(".jar") && !managed_files.contains(filename) {
                    ui::action(&format!("Removing unmanaged file: {}", filename));
                    fs::remove_file(&path)?;
                    removed_any = true;
                }
            }
        }
    }

    Ok(removed_any)
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
