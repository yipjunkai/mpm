// CLI module for handling command-line interface

use crate::constants;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "pm")]
#[command(about = "Deterministic plugin manager for Minecraft servers")]
#[command(
    long_about = "mpm is a deterministic plugin manager for Minecraft servers that ensures \
    reproducible plugin installations through lockfiles and hash verification."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Initialize a new plugin manifest
    ///
    /// Creates a plugins.toml file in the current directory (or PM_DIR if set).
    /// This is the first step to start managing plugins with mpm.
    Init {
        /// Minecraft version (e.g., 1.20.2)
        #[arg(default_value = constants::DEFAULT_MC_VERSION)]
        version: String,
    },
    /// Add a plugin to the manifest
    ///
    /// Adds a plugin specification to plugins.toml. The spec format is:
    /// source:id, source:id@version, id, or id@version
    ///
    /// If no source is specified, defaults to DEFAULT_PLUGIN_SOURCE.
    ///
    /// Examples:
    ///   mpm add fabric-api
    ///   mpm add worldedit@7.3.0
    ///   mpm add modrinth:fabric-api
    ///   mpm add modrinth:worldedit@7.3.0
    Add {
        /// Plugin specification (id[@version] or source:id[@version])
        spec: String,
    },
    /// Remove a plugin from the manifest
    ///
    /// Removes a plugin from plugins.toml by its name (the key in the manifest).
    Remove {
        /// Plugin name to remove
        spec: String,
    },
    /// Generate or update the lockfile
    ///
    /// Resolves plugin versions and generates plugins.lock with exact versions,
    /// filenames, URLs, and hashes. This ensures reproducible installations.
    Lock {
        /// Preview changes without writing the lockfile
        #[arg(long)]
        dry_run: bool,
    },
    /// Synchronize plugins directory with lockfile
    ///
    /// Downloads missing plugins, verifies hashes, and removes unmanaged files.
    /// Ensures the plugins directory matches the lockfile exactly.
    Sync {
        /// Preview changes without modifying the plugins directory
        #[arg(long)]
        dry_run: bool,
    },
    /// Check plugin manager health
    ///
    /// Verifies that configuration files exist, plugin files are present,
    /// filenames match, and hashes are correct. Also detects unmanaged files.
    ///
    /// Exit codes:
    ///   0 = healthy (no errors, no warnings)
    ///   1 = drift (warnings present, e.g., unmanaged files)
    ///   2 = failure (errors present, e.g., missing files, hash mismatches)
    Doctor {
        /// Output results as JSON instead of human-readable format
        ///
        /// Useful for CI/CD pipelines and scripting. The JSON output includes
        /// status, summary counts, and detailed check results.
        #[arg(long)]
        json: bool,
    },
    /// Import existing plugins from /plugins directory
    ///
    /// Scans the plugins directory for JAR files, reads plugin.yml from each,
    /// computes SHA-256 hashes, and generates plugins.toml and plugins.lock.
    /// Plugins are marked with source "unknown" since they weren't installed via mpm.
    ///
    /// This command requires that plugins.toml does not already exist.
    Import,
}
