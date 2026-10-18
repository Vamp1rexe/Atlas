use crate::instances::{self, Instance};
use crate::net::{self, DlItem, ProgressFn};
use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;

const API: &str = "https://api.modrinth.com/v2";

#[derive(Serialize, Deserialize, Clone)]
pub struct Hit {
    pub project_id: String,
    pub slug: String,
    pub title: String,
    pub description: String,
    pub icon_url: Option<String>,
    pub downloads: u64,
    pub author: String,
    pub project_type: String,
}

#[derive(Deserialize)]
struct SearchResp {
    hits: Vec<Hit>,
    total_hits: u64,
}

#[derive(Serialize)]
pub struct SearchResult {
    pub hits: Vec<Hit>,
    pub total: u64,
}

/// `project_type`: mod | modpack | resourcepack | shader
pub async fn search(
    query: &str,
    project_type: &str,
    mc_version: Option<&str>,
    loader: Option<&str>,
    offset: u32,
) -> Result<SearchResult> {
    let mut facets: Vec<Vec<String>> = vec![vec![format!("project_type:{project_type}")]];
    if let Some(v) = mc_version.filter(|v| !v.is_empty()) {
        facets.push(vec![format!("versions:{v}")]);
    }
    if project_type == "mod" {
        if let Some(l) = loader.filter(|l| !l.is_empty() && *l != "vanilla") {
            facets.push(vec![format!("categories:{l}")]);
        }
    }
    let resp = net::http()
        .get(format!("{API}/search"))
        .query(&[
            ("query", query.to_string()),
            ("facets", serde_json::to_string(&facets)?),
            ("limit", "20".to_string()),
            ("offset", offset.to_string()),
        ])
        .send()
        .await?
        .error_for_status()?;
    let r: SearchResp = resp.json().await?;
    Ok(SearchResult {
        hits: r.hits,
        total: r.total_hits,
    })
}

async fn project_versions(
    project_id: &str,
    mc: Option<&str>,
    loader: Option<&str>,
) -> Result<Vec<Value>> {
    let mut q: Vec<(&str, String)> = vec![];
    if let Some(mc) = mc {
        q.push(("game_versions", serde_json::to_string(&[mc])?));
    }
    if let Some(l) = loader {
        q.push(("loaders", serde_json::to_string(&[l])?));
    }
    let resp = net::http()
        .get(format!("{API}/project/{project_id}/version"))
        .query(&q)
        .send()
        .await?
        .error_for_status()?;
    Ok(resp.json().await?)
}

/// Newest release version, or the newest version of any kind if there is no release.
fn pick(versions: &[Value]) -> Option<&Value> {
    versions
        .iter()
        .find(|v| v["version_type"] == "release")
        .or_else(|| versions.first())
}

fn primary_file(version: &Value) -> Option<&Value> {
    let files = version["files"].as_array()?;
    files.iter().find(|f| f["primary"] == true).or_else(|| files.first())
}

fn folder_for(project_type: &str) -> Result<&'static str> {
    Ok(match project_type {
        "mod" => "mods",
        "resourcepack" => "resourcepacks",
        "shader" => "shaderpacks",
        other => bail!("{other} lässt sich nicht direkt installieren"),
    })
}

/// Installs a project into an instance, including required dependencies for mods.
/// Returns the file names that were installed.
pub async fn install_project(
    project_id: &str,
    project_type: &str,
    inst: &Instance,
    progress: &ProgressFn,
) -> Result<Vec<String>> {
    let folder = folder_for(project_type)?;
    let game_dir = instances::dir(&inst.id);
    let loader = (project_type == "mod").then(|| inst.loader.as_str());
    if loader == Some("vanilla") {
        bail!("Mods brauchen einen Loader – leg die Instanz mit Fabric an.");
    }

    let mut installed = vec![];
    let mut visited = HashSet::new();
    let mut queue = vec![(project_id.to_string(), true)];

    while let Some((pid, is_main)) = queue.pop() {
        if !visited.insert(pid.clone()) {
            continue;
        }
        let versions = project_versions(&pid, Some(&inst.mc_version), loader).await?;
        let Some(v) = pick(&versions) else {
            if is_main {
                bail!("Keine passende Version für Minecraft {} gefunden.", inst.mc_version);
            }
            continue;
        };
        let file = primary_file(v).ok_or_else(|| anyhow!("Version ohne Datei"))?;
        let filename = file["filename"].as_str().unwrap_or("download.jar");
        let dest = net::safe_join(&game_dir.join(folder), filename)
            .ok_or_else(|| anyhow!("Ungültiger Dateiname {filename}"))?;
        net::download_many(
            vec![DlItem {
                url: file["url"].as_str().context("Download-URL fehlt")?.into(),
                dest,
                sha1: file["hashes"]["sha1"].as_str().map(String::from),
            }],
            &format!("Installiere {filename}"),
            progress,
        )
        .await?;
        installed.push(filename.to_string());

        if project_type == "mod" {
            for dep in v["dependencies"].as_array().cloned().unwrap_or_default() {
                if dep["dependency_type"] != "required" {
                    continue;
                }
                let dep_pid = match (dep["project_id"].as_str(), dep["version_id"].as_str()) {
                    (Some(p), _) => Some(p.to_string()),
                    (None, Some(vid)) => net::get_json::<Value>(&format!("{API}/version/{vid}"))
                        .await
                        .ok()
                        .and_then(|v| v["project_id"].as_str().map(String::from)),
                    _ => None,
                };
                if let Some(p) = dep_pid {
                    queue.push((p, false));
                }
            }
        }
    }
    Ok(installed)
}

/// Downloads a .mrpack, creates a new instance for it and fetches all pack files.
pub async fn install_modpack(project_id: &str, progress: &ProgressFn) -> Result<Instance> {
    let project: Value = net::get_json(&format!("{API}/project/{project_id}")).await?;
    let title = project["title"].as_str().unwrap_or("Modpack").to_string();

    let versions = project_versions(project_id, None, None).await?;
    let v = pick(&versions).ok_or_else(|| anyhow!("Modpack hat keine Version"))?;
    let file = v["files"]
        .as_array()
        .and_then(|f| f.iter().find(|f| f["filename"].as_str().map(|n| n.ends_with(".mrpack")).unwrap_or(false)))
        .or_else(|| primary_file(v))
        .ok_or_else(|| anyhow!("Modpack-Datei fehlt"))?;
    let bytes = net::get_bytes(file["url"].as_str().context("Download-URL fehlt")?).await?;

    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes))?;
    let index: Value = {
        let mut f = zip.by_name("modrinth.index.json").context("modrinth.index.json fehlt")?;
        let mut s = String::new();
        std::io::Read::read_to_string(&mut f, &mut s)?;
        serde_json::from_str(&s)?
    };

    let deps = &index["dependencies"];
    let mc = deps["minecraft"]
        .as_str()
        .context("Modpack nennt keine Minecraft-Version")?;
    let (loader, loader_version) = if let Some(v) = deps["fabric-loader"].as_str() {
        ("fabric", Some(v.to_string()))
    } else if deps.get("forge").is_some() || deps.get("neoforge").is_some() || deps.get("quilt-loader").is_some() {
        bail!("Dieses Modpack nutzt Forge/NeoForge/Quilt – Atlas unterstützt bisher nur Fabric und Vanilla.");
    } else {
        ("vanilla", None)
    };

    let inst = instances::create(&title, mc, loader, loader_version)?;
    let game_dir = instances::dir(&inst.id);

    let mut items = vec![];
    for f in index["files"].as_array().cloned().unwrap_or_default() {
        if f["env"]["client"] == "unsupported" {
            continue;
        }
        let rel = f["path"].as_str().unwrap_or_default();
        let Some(dest) = net::safe_join(&game_dir, rel) else { continue };
        let Some(url) = f["downloads"].get(0).and_then(|u| u.as_str()) else { continue };
        items.push(DlItem {
            url: url.into(),
            dest,
            sha1: f["hashes"]["sha1"].as_str().map(String::from),
        });
    }
    net::download_many(items, "Modpack-Dateien", progress).await?;

    // overrides/ and client-overrides/ are copied over the instance as-is
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i)?;
        if entry.is_dir() {
            continue;
        }
        let name = entry.name().replace('\\', "/");
        let rel = name
            .strip_prefix("overrides/")
            .or_else(|| name.strip_prefix("client-overrides/"));
        let Some(rel) = rel else { continue };
        let Some(dest) = net::safe_join(&game_dir, rel) else { continue };
        if let Some(p) = dest.parent() {
            std::fs::create_dir_all(p)?;
        }
        let mut out = std::fs::File::create(dest)?;
        std::io::copy(&mut entry, &mut out)?;
    }

    Ok(inst)
}
