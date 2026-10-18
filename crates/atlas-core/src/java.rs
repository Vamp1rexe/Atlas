use crate::net::{self, Progress, ProgressFn};
use crate::paths;
use anyhow::{bail, Result};
use std::path::{Path, PathBuf};

fn exe() -> &'static str {
    if cfg!(windows) {
        "java.exe"
    } else {
        "java"
    }
}

fn find_java(dir: &Path, depth: u32) -> Option<PathBuf> {
    if depth == 0 {
        return None;
    }
    for entry in std::fs::read_dir(dir).ok()? {
        let p = entry.ok()?.path();
        if p.is_dir() {
            if p.file_name().map(|n| n == "bin").unwrap_or(false) && p.join(exe()).exists() {
                return Some(p.join(exe()));
            }
            if let Some(found) = find_java(&p, depth - 1) {
                return Some(found);
            }
        }
    }
    None
}

/// Picks a Java executable: user override, then Atlas' own runtime, then a download from Adoptium.
pub async fn resolve(major: u32, custom: &str, progress: &ProgressFn) -> Result<PathBuf> {
    if !custom.trim().is_empty() {
        let p = PathBuf::from(custom.trim());
        if p.exists() {
            return Ok(p);
        }
        bail!("Java-Pfad aus den Einstellungen existiert nicht: {custom}");
    }

    // Adoptium has no Java 16; Minecraft 1.17 runs fine on 17.
    let major = if major == 16 { 17 } else { major };
    let dir = paths::runtime().join(format!("java-{major}"));
    if let Some(j) = find_java(&dir, 4) {
        return Ok(j);
    }

    let os = if cfg!(windows) {
        "windows"
    } else if cfg!(target_os = "macos") {
        "mac"
    } else {
        "linux"
    };
    let arch = if cfg!(target_arch = "aarch64") { "aarch64" } else { "x64" };
    let url = format!(
        "https://api.adoptium.net/v3/binary/latest/{major}/ga/{os}/{arch}/jre/hotspot/normal/eclipse"
    );
    progress(Progress {
        stage: format!("Java {major} herunterladen"),
        done: 0,
        total: 1,
    });
    let bytes = net::get_bytes(&url).await?;
    let dir2 = dir.clone();
    tokio::task::spawn_blocking(move || -> Result<()> {
        std::fs::create_dir_all(&dir2)?;
        if cfg!(windows) {
            let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes))?;
            for i in 0..zip.len() {
                let mut entry = zip.by_index(i)?;
                let Some(rel) = entry.enclosed_name() else { continue };
                let out = dir2.join(rel);
                if entry.is_dir() {
                    std::fs::create_dir_all(&out)?;
                } else {
                    if let Some(p) = out.parent() {
                        std::fs::create_dir_all(p)?;
                    }
                    let mut f = std::fs::File::create(&out)?;
                    std::io::copy(&mut entry, &mut f)?;
                }
            }
        } else {
            let gz = flate2::read::GzDecoder::new(std::io::Cursor::new(bytes));
            tar::Archive::new(gz).unpack(&dir2)?;
        }
        Ok(())
    })
    .await??;
    progress(Progress {
        stage: format!("Java {major} herunterladen"),
        done: 1,
        total: 1,
    });
    find_java(&dir, 4).ok_or_else(|| anyhow::anyhow!("Java wurde entpackt, aber keine Java-Datei gefunden"))
}
