use crate::paths;
use anyhow::Result;
use serde::{Deserialize, Serialize};

pub const DEFAULT_COSMETICS_URL: &str =
    "https://raw.githubusercontent.com/Vamp1rexe/Atlas/main/cosmetics";
pub const DEFAULT_COSMETICS_MOD_URL: &str =
    "https://github.com/Vamp1rexe/Atlas/releases/latest/download/atlas-cosmetics.jar";

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub username: String,
    pub ram_mb: u32,
    pub java_path: String,
    pub cosmetics_url: String,
    pub cosmetics_mod_url: String,
    pub equipped: Vec<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            username: "Player".into(),
            ram_mb: 4096,
            java_path: String::new(),
            cosmetics_url: DEFAULT_COSMETICS_URL.into(),
            cosmetics_mod_url: DEFAULT_COSMETICS_MOD_URL.into(),
            equipped: vec![],
        }
    }
}

fn file() -> std::path::PathBuf {
    paths::root().join("settings.json")
}

pub fn load() -> Settings {
    std::fs::read_to_string(file())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save(s: &Settings) -> Result<()> {
    std::fs::write(file(), serde_json::to_string_pretty(s)?)?;
    Ok(())
}
