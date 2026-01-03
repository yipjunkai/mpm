// Lock command for generating or updating the lockfile

use crate::lockfile::{LockedPlugin, Lockfile};
use crate::manifest::Manifest;
use crate::sources::REGISTRY;
use toml;

pub async fn lock(dry_run: bool) -> anyhow::Result<i32> {
    // Load manifest
    let manifest = Manifest::load()
        .map_err(|_| anyhow::anyhow!("Manifest not found. Run 'pm init' first."))?;

    if dry_run {
        println!("[DRY RUN] Previewing lock changes...");
    }

    let mut lockfile = Lockfile::new();
    let minecraft_version = Some(manifest.minecraft.version.as_str());

    // For each plugin, resolve version
    for (name, plugin_spec) in manifest.plugins.iter() {
        println!("Resolving {}...", name);

        // Get the source implementation
        let source = REGISTRY.get_or_error(&plugin_spec.source)?;

        // Validate plugin ID format
        source.validate_plugin_id(&plugin_spec.id)?;

        // Resolve version using the trait
        let resolved = source
            .resolve_version(
                &plugin_spec.id,
                plugin_spec.version.as_deref(),
                minecraft_version,
            )
            .await?;

        lockfile.add_plugin(LockedPlugin {
            name: name.clone(),
            source: plugin_spec.source.clone(),
            version: resolved.version.clone(),
            file: resolved.filename.clone(),
            url: resolved.url.clone(),
            hash: resolved.hash.clone(),
        });

        println!("  → {} {}", name, resolved.version);
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
