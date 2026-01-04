// Add command for adding a plugin to the manifest

use crate::commands::lock;
use crate::manifest::{Manifest, PluginSpec};
use crate::sources::REGISTRY;
use log::{debug, info};

pub async fn add(spec: String, no_update: bool) -> anyhow::Result<()> {
    // Parse spec format:
    // - source:id or source:id@version (e.g., modrinth:fabric-api)
    // - id or id@version (searches through all sources in priority order)
    let (source, id_version) = if let Some(colon_pos) = spec.find(':') {
        let source = &spec[..colon_pos];
        let id_version = &spec[colon_pos + 1..];
        (Some(source), id_version)
    } else {
        // No colon found, will search through all sources
        (None, spec.as_str())
    };

    let (id, version) = if let Some(at_pos) = id_version.find('@') {
        let id = &id_version[..at_pos];
        let version = Some(id_version[at_pos + 1..].to_string());
        (id, version)
    } else {
        (id_version, None)
    };

    // Load existing manifest
    let mut manifest = Manifest::load()
        .map_err(|_| anyhow::anyhow!("Manifest not found. Run 'pm init' first."))?;

    let minecraft_version = Some(manifest.minecraft.version.as_str());

    // If source is specified, use it directly
    // Otherwise, search through all sources in priority order
    let (source_name, source_impl) = if let Some(source_str) = source {
        let source_impl = REGISTRY.get_or_error(source_str)?;
        (source_str, source_impl)
    } else {
        // Search through sources in priority order
        let sources = REGISTRY.get_priority_order();
        let mut last_error = None;

        for source_impl in sources {
            let source_name = source_impl.name();

            // First, try to validate the plugin ID format (fast check)
            if source_impl.validate_plugin_id(id).is_err() {
                continue; // Skip this source if ID format doesn't match
            }

            // Then try to resolve the version (confirms existence)
            match source_impl
                .resolve_version(id, version.as_deref(), minecraft_version)
                .await
            {
                Ok(_) => {
                    // Found it! Use this source
                    debug!("Found plugin '{}' in source '{}'", id, source_name);
                    return add_plugin_to_manifest(
                        &mut manifest,
                        source_name,
                        id,
                        version,
                        no_update,
                    )
                    .await;
                }
                Err(e) => {
                    // Store error but continue searching
                    last_error = Some((source_name, e));
                }
            }
        }

        // If we get here, plugin wasn't found in any source
        let error_msg = if let Some((last_source, last_err)) = last_error {
            format!(
                "Plugin '{}' not found in any source. Last attempted source '{}': {}",
                id, last_source, last_err
            )
        } else {
            format!(
                "Plugin '{}' not found in any source. No source accepted the plugin ID format.",
                id
            )
        };
        anyhow::bail!(error_msg);
    };

    // Source was explicitly specified, validate and add
    source_impl.validate_plugin_id(id)?;

    // Check compatibility with Minecraft version
    let _resolved = source_impl
        .resolve_version(id, version.as_deref(), minecraft_version)
        .await
        .map_err(|e| {
            anyhow::anyhow!(
                "Failed to resolve plugin '{}' from source '{}': {}",
                id,
                source_name,
                e
            )
        })?;

    add_plugin_to_manifest(&mut manifest, source_name, id, version, no_update).await
}

async fn add_plugin_to_manifest(
    manifest: &mut Manifest,
    source: &str,
    id: &str,
    version: Option<String>,
    no_update: bool,
) -> anyhow::Result<()> {
    // Add plugin to manifest (compatibility check passed)
    let plugin_name = id.to_string();
    manifest.plugins.insert(
        plugin_name.clone(),
        PluginSpec {
            source: source.to_string(),
            id: id.to_string(),
            version,
        },
    );

    manifest.save()?;
    info!("Added plugin '{}' from source '{}'", plugin_name, source);

    // Automatically lock after adding unless --no-update is specified
    if !no_update {
        lock::lock(false).await?;
    }

    Ok(())
}
