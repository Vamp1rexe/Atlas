//! Cosmetics catalog. The launcher manages files, custom capes and the equipped set;
//! the in-game Atlas Cosmetics mod renders them (see cosmetics/README.md).

use crate::instances::{self, Instance};
use crate::modrinth;
use crate::net::{self, DlItem, ProgressFn};
use crate::paths;
use crate::settings::CosmeticOption;
use anyhow::{bail, Result};
use base64::Engine;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
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
    /// path relative to the cosmetics files folder
    pub model: String,
    pub texture: String,
    #[serde(default)]
    pub preview: Option<String>,
    /// "#rrggbb" the texture is tinted with by default
    #[serde(default)]
    pub default_color: String,
    /// created by the user (cape from an image)
    #[serde(default)]
    pub custom: bool,
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

/// Merged catalog (remote + custom). This is the file the in-game mod reads.
pub fn catalog_file() -> PathBuf {
    paths::cosmetics().join("catalog.json")
}

pub fn options_file() -> PathBuf {
    paths::cosmetics().join("options.json")
}

fn remote_file() -> PathBuf {
    paths::cosmetics().join("remote.json")
}

fn custom_file() -> PathBuf {
    paths::cosmetics().join("custom.json")
}

fn read_catalog(path: &PathBuf) -> Option<Catalog> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
}

fn empty() -> Catalog {
    Catalog {
        format: 1,
        cosmetics: vec![],
    }
}

fn load_remote() -> Catalog {
    read_catalog(&remote_file())
        .or_else(|| serde_json::from_str(BUNDLED).ok())
        .unwrap_or_else(empty)
}

fn load_custom() -> Catalog {
    read_catalog(&custom_file()).unwrap_or_else(empty)
}

fn load_catalog() -> Catalog {
    let mut c = load_remote();
    c.cosmetics.extend(load_custom().cosmetics);
    c
}

/// Writes the merged catalog the mod reads.
pub fn write_merged() -> Result<()> {
    std::fs::create_dir_all(paths::cosmetics())?;
    std::fs::write(catalog_file(), serde_json::to_string_pretty(&load_catalog())?)?;
    Ok(())
}

pub fn write_options(options: &HashMap<String, CosmeticOption>) -> Result<()> {
    std::fs::create_dir_all(paths::cosmetics())?;
    std::fs::write(options_file(), serde_json::to_string_pretty(options)?)?;
    Ok(())
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
    std::fs::write(remote_file(), serde_json::to_string_pretty(&catalog)?)?;
    write_merged()?;
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

const CAPE_ROWS: u32 = 8;

fn png_size(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.len() < 24 || &bytes[..8] != b"\x89PNG\r\n\x1a\n" {
        return None;
    }
    let w = u32::from_be_bytes(bytes[16..20].try_into().ok()?);
    let h = u32::from_be_bytes(bytes[20..24].try_into().ok()?);
    Some((w, h))
}

/// Cape model: a chain of 8 horizontal strips that bend and wave independently.
fn cape_model(w: u32, h: u32) -> serde_json::Value {
    let row_h = h / CAPE_ROWS;
    let mut bones = vec![];
    for i in 0..CAPE_ROWS {
        let y = 2.0 * i as f32;
        let uv = [0, row_h * i, w, row_h * (i + 1)];
        bones.push(serde_json::json!({
            "name": format!("row{i}"),
            "parent": if i == 0 { serde_json::Value::Null } else { format!("row{}", i - 1).into() },
            "pivot": [0.0, y, 2.8],
            "rotation": {
                "base": [1.6, 0.0, 0.0],
                "amp": [2.0, 0.0, 0.8],
                "move": [5.0, 0.0, 0.0],
                "speed": 1.8,
                "phase": -(i as f32) * 0.55
            },
            "cubes": [{
                "from": [-5.0, y, 2.8],
                "to": [5.0, y + 2.15, 3.4],
                "faces": { "south": uv, "north": [uv[2], uv[1], uv[0], uv[3]] }
            }]
        }));
    }
    serde_json::json!({
        "format": 2,
        "texture_size": [w, h],
        "attach": "body",
        "offset": [0, 0, 0],
        "scale": 1.0,
        "bones": bones
    })
}

fn slug(name: &str) -> String {
    let s: String = name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let s = s.trim_matches('-').to_string();
    if s.is_empty() {
        "cape".into()
    } else {
        s.chars().take(24).collect()
    }
}

/// Turns a cropped image (PNG, height divisible by 8) into a wavy cape in the library.
pub fn add_cape(name: &str, png_base64: &str) -> Result<Cosmetic> {
    let bytes = base64::engine::general_purpose::STANDARD.decode(png_base64.trim())?;
    let Some((w, h)) = png_size(&bytes) else {
        bail!("Das Bild ist kein PNG");
    };
    if w < 10 || w > 256 || h < 16 || h > 512 || h % CAPE_ROWS != 0 {
        bail!("Ungültige Bildgröße {w}×{h}");
    }
    let name = name.trim();
    let name = if name.is_empty() { "Eigenes Cape" } else { name };
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let id = format!("cape-{}-{stamp}", slug(name));

    let rel_dir = format!("custom/{id}");
    let dir = files_dir().join(&rel_dir);
    std::fs::create_dir_all(&dir)?;
    std::fs::write(dir.join("cape.png"), &bytes)?;
    std::fs::write(dir.join("model.json"), serde_json::to_string(&cape_model(w, h))?)?;

    let entry = Cosmetic {
        id,
        name: name.to_string(),
        slot: "cape".into(),
        rarity: "custom".into(),
        description: "Eigenes Cape aus einem Bild".into(),
        model: format!("{rel_dir}/model.json"),
        texture: format!("{rel_dir}/cape.png"),
        preview: Some(format!("{rel_dir}/cape.png")),
        default_color: "#ffffff".into(),
        custom: true,
        preview_data: None,
    };
    let mut custom = load_custom();
    custom.cosmetics.push(entry.clone());
    std::fs::create_dir_all(paths::cosmetics())?;
    std::fs::write(custom_file(), serde_json::to_string_pretty(&custom)?)?;
    write_merged()?;
    Ok(entry)
}

/// Removes a custom cosmetic and its files. Built-in cosmetics cannot be deleted.
pub fn delete_custom(id: &str) -> Result<()> {
    let mut custom = load_custom();
    let Some(pos) = custom.cosmetics.iter().position(|c| c.id == id) else {
        bail!("Nur eigene Cosmetics lassen sich löschen");
    };
    custom.cosmetics.remove(pos);
    std::fs::write(custom_file(), serde_json::to_string_pretty(&custom)?)?;
    if net::safe_join(&files_dir(), &format!("custom/{id}")).is_some() && !id.contains('/') {
        let _ = std::fs::remove_dir_all(files_dir().join("custom").join(id));
    }
    write_merged()?;
    Ok(())
}

/// Minecraft versions the Atlas Cosmetics mod is built for.
pub fn mod_supports(mc_version: &str) -> bool {
    mc_version == "26.2" || mc_version.starts_with("26.2.")
}

/// Puts the Atlas Cosmetics mod (and the Fabric API it needs) into an instance.
pub async fn ensure_mod(inst: &Instance, mod_url: &str, progress: &ProgressFn) -> Result<()> {
    let mods = instances::dir(&inst.id).join("mods");
    std::fs::create_dir_all(&mods)?;
    let target = mods.join("atlas-cosmetics.jar");

    if !target.exists() {
        // A jar placed in %APPDATA%\Atlas\mods wins over the download (handy for local builds).
        let local = paths::root().join("mods").join("atlas-cosmetics.jar");
        if local.exists() {
            std::fs::copy(&local, &target)?;
        } else {
            net::download(mod_url, &target, None).await?;
        }
    }

    let has_api = std::fs::read_dir(&mods)?
        .filter_map(|e| e.ok())
        .any(|e| e.file_name().to_string_lossy().starts_with("fabric-api"));
    if !has_api {
        modrinth::install_project("fabric-api", "mod", inst, progress).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn png_header_is_read() {
        let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        png.extend_from_slice(&30u32.to_be_bytes());
        png.extend_from_slice(&48u32.to_be_bytes());
        assert_eq!(png_size(&png), Some((30, 48)));
        assert_eq!(png_size(b"not a png at all, just text"), None);
    }

    #[test]
    fn cape_has_eight_chained_rows_covering_the_image() {
        let m = cape_model(30, 48);
        let bones = m["bones"].as_array().unwrap();
        assert_eq!(bones.len(), 8);
        assert!(bones[0]["parent"].is_null());
        assert_eq!(bones[3]["parent"], "row2");
        assert_eq!(bones[7]["cubes"][0]["faces"]["south"], serde_json::json!([0, 42, 30, 48]));
    }
}
