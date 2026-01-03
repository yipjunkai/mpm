// Lock command for generating or updating the lockfile

use crate::lockfile::{LockedPlugin, Lockfile};
use crate::manifest::Manifest;
use crate::sources::{github, hangar, modrinth};
use toml;

pub async fn lock(dry_run: bool) -> anyhow::Result<i32> {
    // Load manifest
    let manifest = Manifest::load()
        .map_err(|_| anyhow::anyhow!("Manifest not found. Run 'pm init' first."))?;

    if dry_run {
        println!("[DRY RUN] Previewing lock changes...");
    }

    let mut lockfile = Lockfile::new();

    // For each plugin, resolve version
    for (name, plugin_spec) in manifest.plugins.iter() {
        println!("Resolving {}...", name);

        let (version, filename, url, hash) = match plugin_spec.source.as_str() {
            "modrinth" => {
                modrinth::resolve_version(&plugin_spec.id, plugin_spec.version.as_deref()).await?
            }
            "hangar" => {
                hangar::resolve_version(&plugin_spec.id, plugin_spec.version.as_deref()).await?
            }
            "github" => {
                github::resolve_version(&plugin_spec.id, plugin_spec.version.as_deref()).await?
            }
            _ => {
                anyhow::bail!("Unsupported source: {}", plugin_spec.source);
            }
        };

        lockfile.add_plugin(LockedPlugin {
            name: name.clone(),
            source: plugin_spec.source.clone(),
            version: version.clone(),
            file: filename.clone(),
            url: url.clone(),
            hash: hash.clone(),
        });

        println!("  → {} {}", name, version);
    }

    // Sort plugins by name
    lockfile.sort_by_name();

    // Exit codes:
    // 0 = healthy, no issues
    // 1 = warnings only (changes detected in dry-run)
    // 2 = errors present
    if dry_run {
        println!("[DRY RUN] Would lock {} plugin(s)", lockfile.plugin.len());

        // Check if lockfile would change by comparing with existing lockfile
        let exit_code = match Lockfile::load() {
            Ok(existing_lockfile) => {
                // Compare lockfiles by serializing them
                let new_content = toml::to_string_pretty(&lockfile)?;
                let existing_content = toml::to_string_pretty(&existing_lockfile)?;
                if new_content == existing_content {
                    0 // No changes needed
                } else {
                    1 // Changes detected
                }
            }
            Err(_) => {
                // No existing lockfile, so it would be created (change)
                1
            }
        };
        Ok(exit_code)
    } else {
        lockfile.save()?;
        println!("Locked {} plugin(s)", lockfile.plugin.len());
        Ok(0) // Success
    }
}
