// Add command for adding a plugin to the manifest

use crate::commands::lock;
use crate::manifest::{Manifest, PluginSpec};
use crate::sources::REGISTRY;
use futures::future::join_all;
use log::{debug, info};
use std::time::Duration;
use tokio::time::timeout;

pub async fn add(spec: String, no_update: bool, skip_compatibility: bool) -> anyhow::Result<()> {
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

    let minecraft_version = if skip_compatibility {
        None
    } else {
        Some(manifest.minecraft.version.as_str())
    };

    // If source is specified, use it directly
    // Otherwise, search through all sources in priority order
    let (source_name, source_impl) = if let Some(source_str) = source {
        let source_impl = REGISTRY.get_or_error(source_str)?;
        (source_str, source_impl)
    } else {
        // Search through all sources in parallel with timeout
        let sources = REGISTRY.get_priority_order();
        let timeout_duration = Duration::from_secs(180); // 3 minutes

        // Create futures for all sources with timeout
        let futures: Vec<_> = sources
            .iter()
            .map(|source_impl| {
                let source_name = source_impl.name();
                let id = id.to_string();
                let version_clone = version.clone();
                let minecraft_version_clone: Option<String> =
                    minecraft_version.map(|s| s.to_string());

                async move {
                    debug!("Searching source '{}' for plugin '{}'", source_name, id);
                    let minecraft_version_ref: Option<&str> = minecraft_version_clone.as_deref();
                    let result = timeout(
                        timeout_duration,
                        source_impl.resolve_version(
                            &id,
                            version_clone.as_deref(),
                            minecraft_version_ref,
                        ),
                    )
                    .await;

                    match result {
                        Ok(Ok(_)) => Ok((source_name, id)),
                        Ok(Err(e)) => {
                            debug!("Source '{}' failed for plugin '{}': {}", source_name, id, e);
                            Err((source_name, e))
                        }
                        Err(_) => {
                            debug!("Source '{}' timed out for plugin '{}'", source_name, id);
                            Err((
                                source_name,
                                anyhow::anyhow!("Search timed out after 3 minutes"),
                            ))
                        }
                    }
                }
            })
            .collect();

        // Wait for all searches to complete/timeout
        let results = join_all(futures).await;

        // Find first successful result in priority order
        let mut errors = Vec::new();
        for result in results {
            match result {
                Ok((source_name, plugin_id)) => {
                    debug!("Found plugin '{}' in source '{}'", plugin_id, source_name);
                    return add_plugin_to_manifest(
                        &mut manifest,
                        source_name,
                        &plugin_id,
                        version,
                        no_update,
                    )
                    .await;
                }
                Err((source_name, err)) => {
                    errors.push((source_name, err));
                }
            }
        }

        // If we get here, plugin wasn't found in any source
        let error_msg = if let Some((last_source, last_err)) = errors.first() {
            format!(
                "Plugin '{}' not found in any source. Last attempted source '{}': {}",
                id, last_source, last_err
            )
        } else {
            format!("Plugin '{}' not found in any source.", id)
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
