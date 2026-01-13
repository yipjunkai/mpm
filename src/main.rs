mod cli;
mod commands;
mod config;
mod constants;
mod lockfile;
mod manifest;
mod sources;
mod ui;

use clap::Parser;
use cli::Cli;
use env_logger::Builder;
use log::{LevelFilter, error};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // Initialize logger based on debug flag
    // If RUST_LOG is set, it takes precedence; otherwise use --debug flag
    let mut builder = Builder::from_default_env();
    if std::env::var("RUST_LOG").is_err() {
        // Only set default level if RUST_LOG is not explicitly set
        // Default to INFO so user-facing messages are visible
        // Use DEBUG when --debug flag is set for detailed diagnostics
        builder.filter_level(if cli.debug {
            LevelFilter::Debug
        } else {
            LevelFilter::Info
        });
    }
    builder.init();

    match cli.command {
        Some(cli::Commands::Init { version }) => {
            commands::init::init(version)?;
        }
        Some(cli::Commands::Add {
            spec,
            no_update,
            skip_compatibility,
        }) => {
            commands::add::add(spec, no_update, skip_compatibility).await?;
        }
        Some(cli::Commands::Remove { spec, no_update }) => {
            commands::remove::remove(spec, no_update).await?;
        }
        Some(cli::Commands::Lock { dry_run }) => match commands::lock::lock(dry_run).await {
            Ok(exit_code) => std::process::exit(exit_code),
            Err(e) => {
                error!("{}", e);
                std::process::exit(2);
            }
        },
        Some(cli::Commands::Sync { dry_run }) => {
            match commands::sync::sync_plugins(dry_run).await {
                Ok(exit_code) => std::process::exit(exit_code),
                Err(e) => {
                    error!("{}", e);
                    std::process::exit(2);
                }
            }
        }
        Some(cli::Commands::Doctor { json }) => match commands::doctor::check_health(json) {
            Ok(exit_code) => std::process::exit(exit_code),
            Err(e) => {
                error!("{}", e);
                std::process::exit(2);
            }
        },
        Some(cli::Commands::Import { version }) => {
            commands::import::import_plugins(version).await?;
        }
        None => {
            // This case should not be reached due to arg_required_else_help,
            // but handle it gracefully just in case
            unreachable!("No command provided, but clap should have shown help")
        }
    }

    Ok(())
}
