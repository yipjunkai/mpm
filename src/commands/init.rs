// Init command for initializing a new plugin manifest

use crate::constants;
use crate::manifest::{Manifest, MinecraftSpec};

pub fn init(version: String) -> anyhow::Result<()> {
    // Check if manifest already exists
    if Manifest::load().is_ok() {
        println!("Manifest detected. Skipping initialization.");
        return Ok(());
    }

    let manifest = Manifest {
        minecraft: MinecraftSpec {
            version: version.clone(),
        },
        plugins: Default::default(),
    };

    manifest.save()?;
    println!(
        "Initialized {} with Minecraft version {}",
        constants::MANIFEST_FILE,
        version
    );
    Ok(())
}
