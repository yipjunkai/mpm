mod cli;
mod commands;
mod config;
mod constants;
mod lockfile;
mod manifest;
mod sources;

use clap::Parser;
use cli::Cli;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        cli::Commands::Init { version } => {
            commands::init::init(version)?;
        }
        cli::Commands::Add { spec } => {
            commands::add::add(spec)?;
        }
        cli::Commands::Remove { spec } => {
            commands::remove::remove(spec)?;
        }
        cli::Commands::Lock { dry_run } => match commands::lock::lock(dry_run).await {
            Ok(exit_code) => std::process::exit(exit_code),
            Err(e) => {
                eprintln!("Error: {}", e);
                std::process::exit(2);
            }
        },
        cli::Commands::Sync { dry_run } => match commands::sync::sync_plugins(dry_run).await {
            Ok(exit_code) => std::process::exit(exit_code),
            Err(e) => {
                eprintln!("Error: {}", e);
                std::process::exit(2);
            }
        },
        cli::Commands::Doctor { json } => match commands::doctor::check_health(json) {
            Ok(exit_code) => std::process::exit(exit_code),
            Err(e) => {
                eprintln!("Error: {}", e);
                std::process::exit(2);
            }
        },
        cli::Commands::Import => {
            commands::import::import_plugins()?;
        }
    }

    Ok(())
}
