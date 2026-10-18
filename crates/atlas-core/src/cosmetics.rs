//! Cosmetics catalog. The launcher only manages files and the equipped set;
//! the in-game Atlas Cosmetics mod renders them (see cosmetics/README.md).

use crate::net::{self, DlItem, ProgressFn};
use crate::paths;
use anyhow::Result;
use base64::Engine;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Catalog shipped inside the launcher, used when the remote one cannot be reached.
const BUNDLED: &str = include_str!("../../../cosmetics/catalog.json");

#[derive(Serialize, Deserialize, Clone)]
pub struct Cosmetic {
    pub id: String,
    pub name: String,
    /// hat | back | cape | wings | face | pet ...
    pub slot: String,
    pub rarity: String,
    #[serde(default)]
    pub description: String,
    /// path relative to the cosmetics folder
    pub model: String,
    pub texture: String,
    #[serde(default)]
    pub preview: Option<String>,
    /// filled by `list()`, not stored
    #[serde(default, skip_deserializing)]
    pub preview_data: Option<String>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Catalog {
    pub format: u32,
    pub cosmetics: Vec<Cosmetic>,
}

pub fn files_dir() -> PathBuf {
    paths::cosmetics().join("files")
}

pub fn catalog_file() -> PathBuf {
    paths::cosmetics().join("catalog.json")
}

fn load_catalog() -> Catalog {
    std::fs::read_to_string(catalog_file())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .or_else(|| serde_json::from_str(BUNDLED).ok())
        .unwrap_or(Catalog {
            format: 1,
            cosmetics: vec![],
        })
}

/// Catalog with preview images inlined as data URIs (the webview cannot read local paths).
pub fn list() -> Catalog {
    let mut c = load_catalog();
    for item in &mut c.cosmetics {
        if let Some(p) = &item.preview {
            if let Ok(bytes) = std::fs::read(files_dir().join(p)) {
                item.preview_data = Some(format!(
                    "data:image/png;base64,{}",
                    base64::engine::general_purpose::STANDARD.encode(bytes)
                ));
            }
        }
    }
    c
}

/// Pulls the newest catalog and all referenced files from `base_url`.
pub async fn sync(base_url: &str, progress: &ProgressFn) -> Result<Catalog> {
    let base = base_url.trim_end_matches('/');
    let catalog: Catalog = match net::get_json(&format!("{base}/catalog.json")).await {
        Ok(c) => c,
        Err(_) => serde_json::from_str(BUNDLED)?,
    };

    let mut items = vec![];
    for c in &catalog.cosmetics {
        let mut files = vec![c.model.clone(), c.texture.clone()];
        files.extend(c.preview.clone());
        for rel in files {
            if let Some(dest) = net::safe_join(&files_dir(), &rel) {
                items.push(DlItem {
                    url: format!("{base}/{rel}"),
                    dest,
                    sha1: None,
                });
            }
        }
    }
    // Files may change under the same name, so always refetch.
    for it in &items {
        let _ = std::fs::remove_file(&it.dest);
    }
    net::download_many(items, "Cosmetics", progress).await?;

    std::fs::create_dir_all(paths::cosmetics())?;
    std::fs::write(catalog_file(), serde_json::to_string_pretty(&catalog)?)?;
    Ok(catalog)
}

/// One cosmetic per slot: equipping replaces whatever sat in that slot.
pub fn equip(equipped: &[String], id: &str) -> Vec<String> {
    let catalog = load_catalog();
    let Some(new) = catalog.cosmetics.iter().find(|c| c.id == id) else {
        return equipped.to_vec();
    };
    let mut out: Vec<String> = equipped
        .iter()
        .filter(|e| {
            catalog
                .cosmetics
                .iter()
                .find(|c| &c.id == *e)
                .map(|c| c.slot != new.slot)
                .unwrap_or(false)
        })
        .cloned()
        .collect();
    out.push(id.to_string());
    out
}
