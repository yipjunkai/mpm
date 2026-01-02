// Remove command for removing a plugin from the manifest

use crate::manifest::Manifest;

pub fn remove(spec: String) -> anyhow::Result<()> {
    // Load existing manifest
    let mut manifest = Manifest::load()
        .map_err(|_| anyhow::anyhow!("Manifest not found. Run 'pm init' first."))?;

    // Remove plugin from manifest
    if manifest.plugins.remove(&spec).is_some() {
        manifest.save()?;
        println!("Removed plugin '{}'", spec);
    } else {
        anyhow::bail!("Plugin '{}' not found in manifest", spec);
    }
    Ok(())
}

