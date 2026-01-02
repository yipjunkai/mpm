// Config module for shared configuration utilities

pub fn config_dir() -> String {
    std::env::var("PM_DIR").unwrap_or_else(|_| ".".to_string())
}

pub fn plugins_dir() -> String {
    format!("{}/plugins", config_dir())
}

pub fn manifest_path() -> String {
    let dir = config_dir();
    if dir == "." {
        "plugins.toml".to_string()
    } else {
        format!("{}/plugins.toml", dir)
    }
}

pub fn lockfile_path() -> String {
    let dir = config_dir();
    if dir == "." {
        "plugins.lock".to_string()
    } else {
        format!("{}/plugins.lock", dir)
    }
}
