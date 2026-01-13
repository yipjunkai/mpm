// Lock command for generating or updating the lockfile

use crate::lockfile::{LockedPlugin, Lockfile};
use crate::manifest::Manifest;
use crate::sources::REGISTRY;
use crate::ui;
use toml;

pub async fn lock(dry_run: bool) -> anyhow::Result<i32> {
    // Load manifest
    let manifest = Manifest::load()
        .map_err(|_| anyhow::anyhow!("Manifest not found. Run 'mpm init' first."))?;

    if dry_run {
        ui::status("[DRY RUN]", "Previewing lock changes...");
    }

    let mut lockfile = Lockfile::new();
    let minecraft_version = Some(manifest.minecraft.version.as_str());

    // Check if there are any GitHub plugins and warn once about version compatibility
    let has_github_plugins = manifest
        .plugins
        .values()
        .any(|spec| spec.source == "github");
    if has_github_plugins && minecraft_version.is_some() {
        ui::warning(
            "GitHub source does not support Minecraft version filtering. \
            Compatibility cannot be verified for GitHub plugins.",
        );
    }

    // For each plugin, resolve version
    for (name, plugin_spec) in manifest.plugins.iter() {
        let spinner = ui::spinner(&format!("Resolving {}...", name));

        // Get the source implementation
        let source = match REGISTRY.get_or_error(&plugin_spec.source) {
            Ok(s) => s,
            Err(e) => {
                ui::finish_spinner_error(&spinner, &format!("{}: {}", name, e));
                return Err(e);
            }
        };

        // Validate plugin ID format
        if let Err(e) = source.validate_plugin_id(&plugin_spec.id) {
            ui::finish_spinner_error(&spinner, &format!("{}: {}", name, e));
            return Err(e);
        }

        // Resolve version using the trait
        let resolved = match source
            .resolve_version(
                &plugin_spec.id,
                plugin_spec.version.as_deref(),
                minecraft_version,
            )
            .await
        {
            Ok(r) => r,
            Err(e) => {
                ui::finish_spinner_error(&spinner, &format!("{}: {}", name, e));
                return Err(e);
            }
        };

        lockfile.add_plugin(LockedPlugin {
            name: name.clone(),
            source: plugin_spec.source.clone(),
            version: resolved.version.clone(),
            file: resolved.filename.clone(),
            url: resolved.url.clone(),
            hash: resolved.hash.clone(),
        });

        ui::finish_spinner_resolved(&spinner, name, &resolved.version);
    }

    // Sort plugins by name
    lockfile.sort_by_name();

    // Exit codes:
    // 0 = healthy, no issues
    // 1 = warnings only (changes detected in dry-run)
    // 2 = errors present
    if dry_run {
        ui::dim(&format!("Would lock {} plugin(s)", lockfile.plugin.len()));

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
        ui::success(&format!("Locked {} plugin(s)", lockfile.plugin.len()));
        Ok(0) // Success
    }
}
