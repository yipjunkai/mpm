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
        Some(cli::Commands::Init { version }) => {
            commands::init::init(version)?;
        }
        Some(cli::Commands::Add { spec, no_update }) => {
            commands::add::add(spec, no_update).await?;
        }
        Some(cli::Commands::Remove { spec, no_update }) => {
            commands::remove::remove(spec, no_update).await?;
        }
        Some(cli::Commands::Lock { dry_run }) => match commands::lock::lock(dry_run).await {
            Ok(exit_code) => std::process::exit(exit_code),
            Err(e) => {
                eprintln!("Error: {}", e);
                std::process::exit(2);
            }
        },
        Some(cli::Commands::Sync { dry_run }) => {
            match commands::sync::sync_plugins(dry_run).await {
                Ok(exit_code) => std::process::exit(exit_code),
                Err(e) => {
                    eprintln!("Error: {}", e);
                    std::process::exit(2);
                }
            }
        }
        Some(cli::Commands::Doctor { json }) => match commands::doctor::check_health(json) {
            Ok(exit_code) => std::process::exit(exit_code),
            Err(e) => {
                eprintln!("Error: {}", e);
                std::process::exit(2);
            }
        },
        Some(cli::Commands::Import) => {
            commands::import::import_plugins().await?;
        }
        None => {
            // This case should not be reached due to arg_required_else_help,
            // but handle it gracefully just in case
            unreachable!("No command provided, but clap should have shown help")
        }
    }

    Ok(())
}
