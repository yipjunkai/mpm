// Lockfile module for handling dependency lock files

use crate::config;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct Lockfile {
    pub plugin: Vec<LockedPlugin>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LockedPlugin {
    pub name: String,
    pub source: String,
    pub version: String,
    pub file: String,
    pub url: String,
    pub sha256: String,
}

impl Lockfile {
    pub fn load() -> anyhow::Result<Self> {
        let path = config::lockfile_path();
        let text = std::fs::read_to_string(&path)?;
        Ok(toml::from_str(&text)?)
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let dir = config::config_dir();
        std::fs::create_dir_all(&dir)?;
        let path = config::lockfile_path();
        let text = toml::to_string_pretty(self)?;
        std::fs::write(&path, text)?;
        Ok(())
    }

    pub fn new() -> Self {
        Self { plugin: Vec::new() }
    }

    pub fn add_plugin(&mut self, plugin: LockedPlugin) {
        self.plugin.push(plugin);
    }

    pub fn sort_by_name(&mut self) {
        self.plugin.sort_by(|a, b| a.name.cmp(&b.name));
    }
}
