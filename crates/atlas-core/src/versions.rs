use crate::instances::Instance;
use crate::net::{self, DlItem, ProgressFn};
use crate::paths;
use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;
use std::path::PathBuf;

const MANIFEST: &str = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";
const FABRIC_META: &str = "https://meta.fabricmc.net/v2";
const ASSET_BASE: &str = "https://resources.download.minecraft.net";

#[derive(Serialize, Deserialize, Clone)]
pub struct VersionEntry {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub url: String,
}

#[derive(Deserialize)]
struct Manifest {
    versions: Vec<VersionEntry>,
}

pub async fn list_versions(include_snapshots: bool) -> Result<Vec<VersionEntry>> {
    let m: Manifest = net::get_json(MANIFEST).await?;
    Ok(m.versions
        .into_iter()
        .filter(|v| v.kind == "release" || (include_snapshots && v.kind == "snapshot"))
        .collect())
}

/// Latest stable Fabric loader for a Minecraft version.
pub async fn latest_fabric_loader(mc: &str) -> Result<String> {
    let v: Vec<Value> = net::get_json(&format!("{FABRIC_META}/versions/loader/{mc}")).await?;
    v.iter()
        .find(|e| e["loader"]["stable"].as_bool().unwrap_or(false))
        .or_else(|| v.first())
        .and_then(|e| e["loader"]["version"].as_str())
        .map(String::from)
        .ok_or_else(|| anyhow!("Kein Fabric-Loader für {mc} verfügbar"))
}

pub fn os_name() -> &'static str {
    if cfg!(windows) {
        "windows"
    } else if cfg!(target_os = "macos") {
        "osx"
    } else {
        "linux"
    }
}

pub fn rules_allow(rules: Option<&Value>) -> bool {
    let Some(rules) = rules.and_then(|r| r.as_array()) else {
        return true;
    };
    let mut allowed = false;
    for r in rules {
        let mut applies = true;
        if let Some(os) = r.get("os") {
            if let Some(n) = os.get("name").and_then(|v| v.as_str()) {
                if n != os_name() {
                    applies = false;
                }
            }
            if let Some(a) = os.get("arch").and_then(|v| v.as_str()) {
                let arch = std::env::consts::ARCH;
                if !((a == "x86" && arch == "x86") || (a == "x86_64" && arch == "x86_64")) {
                    applies = false;
                }
            }
        }
        // Rules gated on launcher features (demo mode, custom resolution, ...) never apply.
        if r.get("features").is_some() {
            applies = false;
        }
        if applies {
            allowed = r["action"] == "allow";
        }
    }
    allowed
}

fn maven_path(name: &str) -> Result<String> {
    let name = name.split('@').next().unwrap_or(name);
    let parts: Vec<&str> = name.split(':').collect();
    if parts.len() < 3 {
        bail!("Ungültiger Maven-Name {name}");
    }
    let (group, artifact, version) = (parts[0].replace('.', "/"), parts[1], parts[2]);
    let classifier = parts.get(3).map(|c| format!("-{c}")).unwrap_or_default();
    Ok(format!(
        "{group}/{artifact}/{version}/{artifact}-{version}{classifier}.jar"
    ))
}

fn merge(parent: Value, child: Value) -> Value {
    let mut out = parent.clone();
    let o = out.as_object_mut().expect("version json is an object");
    if let Some(c) = child.as_object() {
        for (k, v) in c {
            match k.as_str() {
                "libraries" => {
                    let mut libs = v.as_array().cloned().unwrap_or_default();
                    libs.extend(parent["libraries"].as_array().cloned().unwrap_or_default());
                    o.insert(k.clone(), Value::Array(libs));
                }
                "arguments" => {
                    let mut args = parent
                        .get("arguments")
                        .cloned()
                        .unwrap_or_else(|| serde_json::json!({}));
                    for key in ["jvm", "game"] {
                        let mut list = args
                            .get(key)
                            .and_then(|x| x.as_array())
                            .cloned()
                            .unwrap_or_default();
                        list.extend(v.get(key).and_then(|x| x.as_array()).cloned().unwrap_or_default());
                        args[key] = Value::Array(list);
                    }
                    o.insert(k.clone(), args);
                }
                _ => {
                    o.insert(k.clone(), v.clone());
                }
            }
        }
    }
    out
}

pub struct Prepared {
    pub version: Value,
    pub jar: PathBuf,
    pub classpath: Vec<PathBuf>,
    pub natives_dir: PathBuf,
    pub asset_index: String,
    pub java_major: u32,
    pub main_class: String,
    pub version_name: String,
}

/// Downloads everything an instance needs (client, libraries, assets, loader)
/// and returns what the launcher needs to build the command line.
pub async fn prepare(inst: &Instance, progress: &ProgressFn) -> Result<Prepared> {
    let mc = inst.mc_version.as_str();

    // 1. vanilla version json
    let vjson_path = paths::versions().join(mc).join(format!("{mc}.json"));
    let vanilla: Value = if vjson_path.exists() {
        serde_json::from_str(&tokio::fs::read_to_string(&vjson_path).await?)?
    } else {
        let manifest: Manifest = net::get_json(MANIFEST).await?;
        let entry = manifest
            .versions
            .iter()
            .find(|v| v.id == mc)
            .ok_or_else(|| anyhow!("Minecraft {mc} existiert nicht"))?;
        let v: Value = net::get_json(&entry.url).await?;
        tokio::fs::create_dir_all(vjson_path.parent().unwrap()).await?;
        tokio::fs::write(&vjson_path, serde_json::to_string(&v)?).await?;
        v
    };

    // 2. loader profile
    let (version, version_name) = if inst.loader == "fabric" {
        let loader = match &inst.loader_version {
            Some(l) => l.clone(),
            None => latest_fabric_loader(mc).await?,
        };
        let profile_id = format!("fabric-loader-{loader}-{mc}");
        let p = paths::versions().join(&profile_id).join(format!("{profile_id}.json"));
        let profile: Value = if p.exists() {
            serde_json::from_str(&tokio::fs::read_to_string(&p).await?)?
        } else {
            let v: Value = net::get_json(&format!(
                "{FABRIC_META}/versions/loader/{mc}/{loader}/profile/json"
            ))
            .await?;
            tokio::fs::create_dir_all(p.parent().unwrap()).await?;
            tokio::fs::write(&p, serde_json::to_string(&v)?).await?;
            v
        };
        (merge(vanilla, profile), profile_id)
    } else {
        (vanilla, mc.to_string())
    };

    // 3. client jar
    let jar = paths::versions().join(mc).join(format!("{mc}.jar"));
    let client = &version["downloads"]["client"];
    net::download_many(
        vec![DlItem {
            url: client["url"].as_str().context("client url fehlt")?.to_string(),
            dest: jar.clone(),
            sha1: client["sha1"].as_str().map(String::from),
        }],
        "Minecraft",
        progress,
    )
    .await?;

    // 4. libraries
    let natives_dir = paths::versions().join(mc).join("natives");
    tokio::fs::create_dir_all(&natives_dir).await?;
    let mut items = vec![];
    let mut classpath = vec![];
    let mut native_jars = vec![];
    let mut seen = HashSet::new();
    for lib in version["libraries"].as_array().cloned().unwrap_or_default() {
        if !rules_allow(lib.get("rules")) {
            continue;
        }
        let name = lib["name"].as_str().unwrap_or_default().to_string();

        // legacy natives: classifier jar, extracted, not on the classpath
        if let Some(natives) = lib.get("natives") {
            if let Some(classifier) = natives.get(os_name()).and_then(|v| v.as_str()) {
                let classifier = classifier.replace("${arch}", "64");
                let art = &lib["downloads"]["classifiers"][&classifier];
                if let (Some(path), Some(url)) = (art["path"].as_str(), art["url"].as_str()) {
                    let dest = paths::libraries().join(path);
                    items.push(DlItem {
                        url: url.into(),
                        dest: dest.clone(),
                        sha1: art["sha1"].as_str().map(String::from),
                    });
                    native_jars.push(dest);
                }
            }
            continue;
        }

        let (path, url, sha1) = if let Some(art) = lib["downloads"].get("artifact") {
            (
                art["path"].as_str().unwrap_or_default().to_string(),
                art["url"].as_str().unwrap_or_default().to_string(),
                art["sha1"].as_str().map(String::from),
            )
        } else {
            let base = lib["url"].as_str().unwrap_or("https://libraries.minecraft.net/");
            let path = maven_path(&name)?;
            (
                path.clone(),
                format!("{}{}", base.trim_end_matches('/').to_string() + "/", path),
                lib["sha1"].as_str().map(String::from),
            )
        };
        if path.is_empty() || !seen.insert(path.clone()) {
            continue;
        }
        let dest = paths::libraries().join(&path);
        if !url.is_empty() {
            items.push(DlItem {
                url,
                dest: dest.clone(),
                sha1,
            });
        }
        if name.contains(":natives-") {
            native_jars.push(dest.clone());
        }
        classpath.push(dest);
    }
    net::download_many(items, "Bibliotheken", progress).await?;
    extract_natives(native_jars, natives_dir.clone()).await?;
    classpath.push(jar.clone());

    // 5. assets
    let ai = &version["assetIndex"];
    let asset_index = ai["id"].as_str().context("assetIndex fehlt")?.to_string();
    let index_path = paths::assets().join("indexes").join(format!("{asset_index}.json"));
    net::download_many(
        vec![DlItem {
            url: ai["url"].as_str().context("assetIndex url fehlt")?.into(),
            dest: index_path.clone(),
            sha1: ai["sha1"].as_str().map(String::from),
        }],
        "Asset-Index",
        progress,
    )
    .await?;
    let index: Value = serde_json::from_str(&tokio::fs::read_to_string(&index_path).await?)?;
    let mut hashes = HashSet::new();
    if let Some(objects) = index["objects"].as_object() {
        for o in objects.values() {
            if let Some(h) = o["hash"].as_str() {
                hashes.insert(h.to_string());
            }
        }
    }
    let items = hashes
        .into_iter()
        .map(|h| DlItem {
            url: format!("{ASSET_BASE}/{}/{h}", &h[..2]),
            dest: paths::assets().join("objects").join(&h[..2]).join(&h),
            sha1: Some(h),
        })
        .collect();
    net::download_many(items, "Assets", progress).await?;

    Ok(Prepared {
        java_major: version["javaVersion"]["majorVersion"].as_u64().unwrap_or(8) as u32,
        main_class: version["mainClass"].as_str().context("mainClass fehlt")?.into(),
        version,
        jar,
        classpath,
        natives_dir,
        asset_index,
        version_name,
    })
}

async fn extract_natives(jars: Vec<PathBuf>, dest: PathBuf) -> Result<()> {
    tokio::task::spawn_blocking(move || -> Result<()> {
        for jar in jars {
            let file = std::fs::File::open(&jar)?;
            let mut zip = zip::ZipArchive::new(file)?;
            for i in 0..zip.len() {
                let mut entry = zip.by_index(i)?;
                if entry.is_dir() {
                    continue;
                }
                let Some(name) = entry.enclosed_name() else { continue };
                if name.starts_with("META-INF") {
                    continue;
                }
                let ext = name.extension().and_then(|e| e.to_str()).unwrap_or_default();
                if !matches!(ext, "dll" | "so" | "dylib" | "jnilib") {
                    continue;
                }
                let Some(file_name) = name.file_name() else { continue };
                let mut out = std::fs::File::create(dest.join(file_name))?;
                std::io::copy(&mut entry, &mut out)?;
            }
        }
        Ok(())
    })
    .await??;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn maven_names_become_paths() {
        assert_eq!(
            maven_path("net.fabricmc:fabric-loader:0.16.5").unwrap(),
            "net/fabricmc/fabric-loader/0.16.5/fabric-loader-0.16.5.jar"
        );
        assert_eq!(
            maven_path("org.lwjgl:lwjgl:3.3.3:natives-windows").unwrap(),
            "org/lwjgl/lwjgl/3.3.3/lwjgl-3.3.3-natives-windows.jar"
        );
    }

    #[test]
    fn rules_follow_os_and_ignore_features() {
        assert!(rules_allow(None));
        let other_os = if os_name() == "windows" { "linux" } else { "windows" };
        let rules = json!([{"action": "allow"}, {"action": "disallow", "os": {"name": other_os}}]);
        assert!(rules_allow(Some(&rules)));
        let only_other = json!([{"action": "allow", "os": {"name": other_os}}]);
        assert!(!rules_allow(Some(&only_other)));
        let feature = json!([{"action": "allow", "features": {"is_demo_user": true}}]);
        assert!(!rules_allow(Some(&feature)));
    }

    #[test]
    fn loader_profile_extends_vanilla() {
        let vanilla = json!({
            "mainClass": "net.minecraft.client.main.Main",
            "libraries": [{"name": "a:b:1"}],
            "arguments": {"jvm": ["-cp", "${classpath}"], "game": ["--username"]},
            "assetIndex": {"id": "17"}
        });
        let fabric = json!({
            "mainClass": "net.fabricmc.loader.impl.launch.knot.KnotClient",
            "libraries": [{"name": "c:d:2"}],
            "arguments": {"jvm": ["-DFabric=1"], "game": []}
        });
        let m = merge(vanilla, fabric);
        assert_eq!(m["mainClass"], "net.fabricmc.loader.impl.launch.knot.KnotClient");
        assert_eq!(m["libraries"][0]["name"], "c:d:2");
        assert_eq!(m["libraries"][1]["name"], "a:b:1");
        assert_eq!(m["arguments"]["jvm"], json!(["-cp", "${classpath}", "-DFabric=1"]));
        assert_eq!(m["assetIndex"]["id"], "17");
    }
}
