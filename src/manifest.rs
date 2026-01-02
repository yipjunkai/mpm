// Manifest module for handling package manifests

use crate::config;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Serialize, Deserialize)]
pub struct Manifest {
    pub minecraft: Minecraft,
    pub plugins: BTreeMap<String, PluginSpec>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Minecraft {
    pub version: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PluginSpec {
    pub source: String,
    pub id: String,
    pub version: Option<String>,
}

impl Manifest {
    pub fn load() -> anyhow::Result<Self> {
        let path = config::manifest_path();
        let text = std::fs::read_to_string(&path)?;
        Ok(toml::from_str(&text)?)
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let dir = config::config_dir();
        std::fs::create_dir_all(&dir)?;
        let path = config::manifest_path();
        let text = toml::to_string_pretty(self)?;
        std::fs::write(&path, text)?;
        Ok(())
    }
}
