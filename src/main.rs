mod cli;
mod error;
mod lockfile;
mod manifest;
mod sources;

use clap::Parser;
use cli::Cli;

use lockfile::{LockedPlugin, Lockfile};
use manifest::{Manifest, Minecraft, PluginSpec};
use sources::modrinth;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        cli::Commands::Init { version } => {
            // Check if manifest already exists
            if Manifest::load().is_ok() {
                println!("Manifest detected. Skipping initialization.");
                return Ok(());
            }

            let manifest = Manifest {
                minecraft: Minecraft {
                    version: version.clone(),
                },
                plugins: Default::default(),
            };

            manifest.save()?;
            println!(
                "Initialized plugins.toml with Minecraft version {}",
                version
            );
        }
        cli::Commands::Add { spec } => {
            // Parse spec format: source:id or source:id@version
            // Example: modrinth:fabric-api or modrinth:worldedit@7.3.0
            let parts: Vec<&str> = spec.split(':').collect();
            if parts.len() != 2 {
                anyhow::bail!("Invalid spec format. Expected: source:id or source:id@version");
            }

            let source = parts[0];
            let id_version = parts[1];

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
        }
        cli::Commands::Remove { spec } => {
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
        }
        cli::Commands::Lock => {
            // Load manifest
            let manifest = Manifest::load()
                .map_err(|_| anyhow::anyhow!("Manifest not found. Run 'pm init' first."))?;

            let mut lockfile = Lockfile::new();

            // For each plugin, resolve version
            for (name, plugin_spec) in manifest.plugins.iter() {
                println!("Resolving {}...", name);

                let (version, filename, url, sha256) = match plugin_spec.source.as_str() {
                    "modrinth" => {
                        modrinth::resolve_version(&plugin_spec.id, plugin_spec.version.as_deref())
                            .await?
                    }
                    _ => {
                        anyhow::bail!("Unsupported source: {}", plugin_spec.source);
                    }
                };

                lockfile.add_plugin(LockedPlugin {
                    name: name.clone(),
                    source: plugin_spec.source.clone(),
                    version: version.clone(),
                    file: filename.clone(),
                    url: url.clone(),
                    sha256: sha256.clone(),
                });

                println!("  → {} {}", name, version);
            }

            // Sort plugins by name
            lockfile.sort_by_name();

            // Save lockfile
            lockfile.save()?;
            println!("Locked {} plugin(s)", lockfile.plugin.len());
        }
        cli::Commands::Sync => {
            println!("Sync command not implemented yet");
        }
    }

    Ok(())
}
