use crate::paths;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub const DEFAULT_COSMETICS_URL: &str =
    "https://raw.githubusercontent.com/Vamp1rexe/Atlas/main/cosmetics";
pub const DEFAULT_COSMETICS_MOD_URL: &str =
    "https://github.com/Vamp1rexe/Atlas/releases/latest/download/atlas-cosmetics.jar";

/// Per-cosmetic customization, applied by the in-game mod.
#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CosmeticOption {
    pub scale: f32,
    /// "#rrggbb"; empty means the cosmetic's own default color
    pub color: String,
}

impl Default for CosmeticOption {
    fn default() -> Self {
        Self {
            scale: 1.0,
            color: String::new(),
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub username: String,
    pub ram_mb: u32,
    pub java_path: String,
    /// Tuned JVM flags for smoother frame times
    pub perf_flags: bool,
    pub theme_accent: String,
    pub theme_bg: String,
    pub cosmetics_url: String,
    pub cosmetics_mod_url: String,
    pub equipped: Vec<String>,
    pub cosmetic_options: HashMap<String, CosmeticOption>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            username: "Player".into(),
            ram_mb: 4096,
            java_path: String::new(),
            perf_flags: true,
            theme_accent: "#1bd96a".into(),
            theme_bg: "#16181c".into(),
            cosmetics_url: DEFAULT_COSMETICS_URL.into(),
            cosmetics_mod_url: DEFAULT_COSMETICS_MOD_URL.into(),
            equipped: vec![],
            cosmetic_options: HashMap::new(),
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

/// "#rrggbb" only; keeps junk out of settings and out of the JVM arguments.
pub fn valid_hex(s: &str) -> bool {
    s.len() == 7 && s.starts_with('#') && s[1..].chars().all(|c| c.is_ascii_hexdigit())
}
