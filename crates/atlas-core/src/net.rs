use anyhow::{bail, Context, Result};
use futures::{StreamExt, TryStreamExt};
use serde::Serialize;
use sha1::{Digest, Sha1};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock};

pub const UA: &str = concat!("AtlasLauncher/", env!("CARGO_PKG_VERSION"));

#[derive(Clone, Serialize)]
pub struct Progress {
    pub stage: String,
    pub done: usize,
    pub total: usize,
}

pub type ProgressFn = Arc<dyn Fn(Progress) + Send + Sync>;

pub fn http() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .user_agent(UA)
            .build()
            .expect("http client")
    })
}

pub async fn get_json<T: serde::de::DeserializeOwned>(url: &str) -> Result<T> {
    let resp = http()
        .get(url)
        .send()
        .await
        .with_context(|| format!("GET {url}"))?
        .error_for_status()
        .with_context(|| format!("GET {url}"))?;
    Ok(resp.json::<T>().await?)
}

pub async fn get_bytes(url: &str) -> Result<Vec<u8>> {
    let resp = http()
        .get(url)
        .send()
        .await
        .with_context(|| format!("GET {url}"))?
        .error_for_status()
        .with_context(|| format!("GET {url}"))?;
    Ok(resp.bytes().await?.to_vec())
}

pub async fn sha1_file(path: &Path) -> Result<String> {
    let bytes = tokio::fs::read(path).await?;
    Ok(hex::encode(Sha1::digest(&bytes)))
}

/// Downloads `url` to `dest` unless a matching file is already there.
pub async fn download(url: &str, dest: &Path, sha1: Option<&str>) -> Result<()> {
    if dest.exists() {
        match sha1 {
            Some(h) => {
                if sha1_file(dest).await?.eq_ignore_ascii_case(h) {
                    return Ok(());
                }
            }
            None => return Ok(()),
        }
    }
    if let Some(parent) = dest.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let bytes = get_bytes(url).await?;
    if let Some(h) = sha1 {
        let got = hex::encode(Sha1::digest(&bytes));
        if !got.eq_ignore_ascii_case(h) {
            bail!("Hash stimmt nicht für {url}");
        }
    }
    let mut tmp = dest.as_os_str().to_owned();
    tmp.push(".part");
    let tmp = PathBuf::from(tmp);
    tokio::fs::write(&tmp, &bytes).await?;
    tokio::fs::rename(&tmp, dest).await?;
    Ok(())
}

pub struct DlItem {
    pub url: String,
    pub dest: PathBuf,
    pub sha1: Option<String>,
}

pub async fn download_many(items: Vec<DlItem>, stage: &str, progress: &ProgressFn) -> Result<()> {
    let total = items.len();
    let done = Arc::new(AtomicUsize::new(0));
    progress(Progress {
        stage: stage.to_string(),
        done: 0,
        total,
    });
    futures::stream::iter(items)
        .map(|it| {
            let done = done.clone();
            let progress = progress.clone();
            let stage = stage.to_string();
            async move {
                let mut last = None;
                for _ in 0..3 {
                    match download(&it.url, &it.dest, it.sha1.as_deref()).await {
                        Ok(()) => {
                            last = None;
                            break;
                        }
                        Err(e) => last = Some(e),
                    }
                }
                if let Some(e) = last {
                    return Err(e);
                }
                let n = done.fetch_add(1, Ordering::SeqCst) + 1;
                progress(Progress {
                    stage,
                    done: n,
                    total,
                });
                Ok::<_, anyhow::Error>(())
            }
        })
        .buffer_unordered(16)
        .try_collect::<Vec<_>>()
        .await?;
    Ok(())
}

/// Rejects absolute paths and `..` so downloaded archives cannot escape `base`.
pub fn safe_join(base: &Path, rel: &str) -> Option<PathBuf> {
    let rel = rel.replace('\\', "/");
    if rel.starts_with('/') || rel.split('/').any(|p| p == ".." || p.contains(':')) {
        return None;
    }
    Some(base.join(rel))
}
