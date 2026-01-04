// Remove command for removing a plugin from the manifest

use crate::commands::lock;
use crate::manifest::Manifest;
use log::info;

pub async fn remove(spec: String, no_update: bool) -> anyhow::Result<()> {
    // Load existing manifest
    let mut manifest = Manifest::load()
        .map_err(|_| anyhow::anyhow!("Manifest not found. Run 'pm init' first."))?;

    // Remove plugin from manifest
    if manifest.plugins.remove(&spec).is_some() {
        manifest.save()?;
        info!("Removed plugin '{}'", spec);

        // Automatically lock after removing unless --no-update is specified
        if !no_update {
            lock::lock(false).await?;
        }
    } else {
        anyhow::bail!("Plugin '{}' not found in manifest", spec);
    }
    Ok(())
}
