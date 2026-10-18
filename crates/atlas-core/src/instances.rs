use crate::paths;
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Serialize, Deserialize)]
pub struct Instance {
    pub id: String,
    pub name: String,
    pub mc_version: String,
    /// "vanilla" or "fabric"
    pub loader: String,
    pub loader_version: Option<String>,
    pub ram_mb: Option<u32>,
}

pub fn dir(id: &str) -> PathBuf {
    paths::instances().join(id)
}

fn slug(name: &str) -> String {
    let s: String = name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let s = s.trim_matches('-').to_string();
    if s.is_empty() {
        "instance".into()
    } else {
        s
    }
}

pub fn list() -> Result<Vec<Instance>> {
    let mut out = vec![];
    let root = paths::instances();
    if !root.exists() {
        return Ok(out);
    }
    for entry in std::fs::read_dir(root)? {
        let p = entry?.path().join("instance.json");
        if let Ok(s) = std::fs::read_to_string(&p) {
            if let Ok(i) = serde_json::from_str::<Instance>(&s) {
                out.push(i);
            }
        }
    }
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(out)
}

pub fn get(id: &str) -> Result<Instance> {
    let s = std::fs::read_to_string(dir(id).join("instance.json"))
        .map_err(|_| anyhow::anyhow!("Instanz {id} nicht gefunden"))?;
    Ok(serde_json::from_str(&s)?)
}

pub fn save(i: &Instance) -> Result<()> {
    let d = dir(&i.id);
    std::fs::create_dir_all(&d)?;
    std::fs::write(d.join("instance.json"), serde_json::to_string_pretty(i)?)?;
    Ok(())
}

pub fn create(
    name: &str,
    mc_version: &str,
    loader: &str,
    loader_version: Option<String>,
) -> Result<Instance> {
    let name = name.trim();
    if name.is_empty() {
        bail!("Name fehlt");
    }
    if loader != "vanilla" && loader != "fabric" {
        bail!("Loader {loader} wird noch nicht unterstützt");
    }
    let base = slug(name);
    let mut id = base.clone();
    let mut n = 2;
    while dir(&id).exists() {
        id = format!("{base}-{n}");
        n += 1;
    }
    let inst = Instance {
        id,
        name: name.to_string(),
        mc_version: mc_version.to_string(),
        loader: loader.to_string(),
        loader_version,
        ram_mb: None,
    };
    save(&inst)?;
    Ok(inst)
}

pub fn delete(id: &str) -> Result<()> {
    let d = dir(id);
    // `id` always comes from our own listing, but never delete outside the instances dir.
    if !d.starts_with(paths::instances()) || id.contains("..") || id.contains('/') || id.contains('\\') {
        bail!("Ungültige Instanz-ID");
    }
    if d.exists() {
        std::fs::remove_dir_all(d)?;
    }
    Ok(())
}
