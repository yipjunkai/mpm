// Init command for initializing a new plugin manifest

use crate::commands::import::detect_minecraft_version_from_paper_jar;
use crate::constants;
use crate::manifest::{Manifest, MinecraftSpec};
use log::{info, warn};

pub fn init(version: Option<String>) -> anyhow::Result<()> {
    // Check if manifest already exists
    if Manifest::load().is_ok() {
        info!("Manifest detected. Skipping initialization.");
        return Ok(());
    }

    // Determine which version to use
    let final_version = if let Some(v) = version {
        // User provided version explicitly, use it
        v
    } else {
        // Try to detect from Paper JAR
        match detect_minecraft_version_from_paper_jar() {
            Some(detected_version) => {
                info!(
                    "Auto-detected Minecraft version {} from Paper JAR",
                    detected_version
                );
                detected_version
            }
            None => {
                warn!(
                    "Could not detect Minecraft version from Paper JAR, using default: {}",
                    constants::DEFAULT_MC_VERSION
                );
                constants::DEFAULT_MC_VERSION.to_string()
            }
        }
    };

    let manifest = Manifest {
        minecraft: MinecraftSpec {
            version: final_version.clone(),
        },
        plugins: Default::default(),
    };

    manifest.save()?;
    info!(
        "Initialized {} with Minecraft version {}",
        constants::MANIFEST_FILE,
        final_version
    );
    Ok(())
}
