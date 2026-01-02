// CLI module for handling command-line interface

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "pm")]
#[command(about = "Deterministic plugin manager for Minecraft servers")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    Init {
        /// Minecraft version (e.g., 1.20.2)
        #[arg(default_value = "1.21.11")]
        version: String,
    },
    Add {
        spec: String,
    },
    Remove {
        spec: String,
    },
    Lock,
    Sync,
}
