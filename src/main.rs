mod cli;
mod error;
mod lockfile;
mod manifest;
mod sources;

use clap::Parser;
use cli::Cli;

use manifest::{Manifest, Minecraft, PluginSpec};
use sources::modrinth;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        cli::Commands::Init => {
            let mut manifest = Manifest {
                minecraft: Minecraft {
                    version: "1.20.2".into(),
                },
                plugins: Default::default(),
            };

            // Test adding a plugin to the manifest
            println!("Testing adding plugin to manifest...");
            manifest.plugins.insert(
                "fabric-api".to_string(),
                PluginSpec {
                    source: "modrinth".to_string(),
                    id: "fabric-api".to_string(),
                    version: None,
                },
            );

            manifest.save()?;
            println!("Created plugins.toml with test plugin");

            let loaded_manifest = Manifest::load()?;
            println!("Loaded plugins.toml: {:?}", loaded_manifest);

            // Test Modrinth API
            println!("\nTesting Modrinth API...");
            let project = modrinth::get_project("fabric-api").await?;
            println!("Fetched project: {project:?}");
        }
        _ => {}
    }

    Ok(())
}
