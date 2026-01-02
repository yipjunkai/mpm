// Constants module for shared string constants

pub const MANIFEST_FILE: &str = "plugins.toml";
pub const LOCKFILE_FILE: &str = "plugins.lock";
pub const PLUGINS_DIR: &str = "plugins";
pub const DEFAULT_MC_VERSION: &str = "1.21.11";
pub const DEFAULT_PLUGIN_SOURCE: &str = "modrinth";

/// Schema version for the doctor --json output format.
/// Increment only on breaking changes to ensure future integrations can safely evolve.
pub const SCHEMA_VERSION: u32 = 1;
