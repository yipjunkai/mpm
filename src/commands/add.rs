// Add command for adding a plugin to the manifest

use crate::constants;
use crate::manifest::{Manifest, PluginSpec};

pub fn add(spec: String) -> anyhow::Result<()> {
    // Parse spec format:
    // - source:id or source:id@version (e.g., modrinth:fabric-api)
    // - id or id@version (defaults to modrinth source)
    let (source, id_version) = if let Some(colon_pos) = spec.find(':') {
        let source = &spec[..colon_pos];
        let id_version = &spec[colon_pos + 1..];
        (source, id_version)
    } else {
        // No colon found, default to modrinth
        (constants::DEFAULT_PLUGIN_SOURCE, spec.as_str())
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

    // Add plugin to manifest
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
    println!("Added plugin '{}' from source '{}'", plugin_name, source);
    Ok(())
}
