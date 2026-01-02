mod cli;
mod config;
mod constants;
mod doctor;
mod error;
mod import;
mod lockfile;
mod manifest;
mod sources;
mod sync;

use clap::Parser;
use cli::Cli;

use lockfile::{LockedPlugin, Lockfile};
use manifest::{Manifest, Minecraft, PluginSpec};
use sources::modrinth;
use toml;

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
                "Initialized {} with Minecraft version {}",
                constants::MANIFEST_FILE,
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
        cli::Commands::Lock { dry_run } => {
            // Load manifest
            let manifest = Manifest::load()
                .map_err(|_| anyhow::anyhow!("Manifest not found. Run 'pm init' first."))?;

            if dry_run {
                println!("[DRY RUN] Previewing lock changes...");
            }

            let mut lockfile = Lockfile::new();

            // For each plugin, resolve version
            for (name, plugin_spec) in manifest.plugins.iter() {
                println!("Resolving {}...", name);

                let (version, filename, url, hash) = match plugin_spec.source.as_str() {
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
                    hash: hash.clone(),
                });

                println!("  → {} {}", name, version);
            }

            // Sort plugins by name
            lockfile.sort_by_name();

            // Save lockfile
            if dry_run {
                println!("[DRY RUN] Would lock {} plugin(s)", lockfile.plugin.len());

                // Check if lockfile would change by comparing with existing lockfile
                let exit_code = match Lockfile::load() {
                    Ok(existing_lockfile) => {
                        // Compare lockfiles by serializing them
                        let new_content = toml::to_string_pretty(&lockfile)?;
                        let existing_content = toml::to_string_pretty(&existing_lockfile)?;
                        if new_content == existing_content {
                            0 // Lockfile already matches
                        } else {
                            1 // Lockfile would change
                        }
                    }
                    Err(_) => {
                        // No existing lockfile, so it would be created (change)
                        1
                    }
                };
                std::process::exit(exit_code);
            } else {
                lockfile.save()?;
                println!("Locked {} plugin(s)", lockfile.plugin.len());
            }
        }
        cli::Commands::Sync { dry_run } => {
            let exit_code = sync::sync_plugins(dry_run).await?;
            if dry_run {
                std::process::exit(exit_code);
            }
        }
        cli::Commands::Doctor { json } => {
            let exit_code = doctor::check_health(json)?;
            std::process::exit(exit_code);
        }
        cli::Commands::Import => {
            import::import_plugins()?;
        }
    }

    Ok(())
}
