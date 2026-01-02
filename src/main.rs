mod cli;
mod error;
mod lockfile;
mod manifest;
mod sources;

use clap::Parser;
use cli::Cli;

use manifest::{Manifest, Minecraft};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        cli::Commands::Init => {
            let manifest = Manifest {
                minecraft: Minecraft {
                    version: "1.20.2".into(),
                },
                plugins: Default::default(),
            };

            manifest.save()?;
            println!("Created plugins.toml");

            let loaded_manifest = Manifest::load()?;
            println!("Loaded plugins.toml: {:?}", loaded_manifest);
        }
        _ => {}
    }

    Ok(())
}
