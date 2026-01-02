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
    Init,
    Add { spec: String },
    Remove { spec: String },
    Lock,
    Sync,
}
