// Remove command for removing a plugin from the manifest

use crate::commands::lock;
use crate::manifest::Manifest;
use crate::ui;

pub async fn remove(spec: String, no_update: bool) -> anyhow::Result<()> {
    // Load existing manifest
    let mut manifest = Manifest::load()
        .map_err(|_| anyhow::anyhow!("Manifest not found. Run 'mpm init' first."))?;

    // Remove plugin from manifest
    if manifest.plugins.remove(&spec).is_some() {
        manifest.save()?;
        ui::success(&format!("Removed {}", spec));

        // Automatically lock after removing unless --no-update is specified
        if !no_update {
            lock::lock(false).await?;
        }
    } else {
        anyhow::bail!("Plugin '{}' not found in manifest", spec);
    }
    Ok(())
}
