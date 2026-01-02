// Config module for shared configuration utilities

pub fn config_dir() -> String {
    std::env::var("PM_DIR").unwrap_or_else(|_| "plugins".to_string())
}

pub fn manifest_path() -> String {
    format!("{}/plugins.toml", config_dir())
}

pub fn lockfile_path() -> String {
    format!("{}/plugins.lock", config_dir())
}
