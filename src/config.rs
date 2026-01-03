// Config module for shared configuration utilities

use crate::constants;

pub fn config_dir() -> String {
    std::env::var("PM_DIR").unwrap_or_else(|_| ".".to_string())
}

pub fn plugins_dir() -> String {
    std::env::var("PM_PLUGINS_DIR")
        .unwrap_or_else(|_| format!("{}/{}", config_dir(), constants::PLUGINS_DIR))
}

pub fn manifest_path() -> String {
    let dir = config_dir();
    if dir == "." {
        constants::MANIFEST_FILE.to_string()
    } else {
        format!("{}/{}", dir, constants::MANIFEST_FILE)
    }
}

pub fn lockfile_path() -> String {
    let dir = config_dir();
    if dir == "." {
        constants::LOCKFILE_FILE.to_string()
    } else {
        format!("{}/{}", dir, constants::LOCKFILE_FILE)
    }
}
