use crate::auth::Account;
use crate::instances::{self, Instance};
use crate::net::ProgressFn;
use crate::{cosmetics, java, paths, versions};
use anyhow::Result;
use serde_json::Value;
use std::collections::HashMap;
use std::process::Stdio;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, BufReader};

pub struct LaunchOptions {
    pub account: Account,
    pub ram_mb: u32,
    pub java_path: String,
    pub equipped: Vec<String>,
    pub cosmetics_mod_url: String,
    pub perf_flags: bool,
}

pub type LogFn = Arc<dyn Fn(String) + Send + Sync>;
pub type ExitFn = Arc<dyn Fn(Option<i32>) + Send + Sync>;

fn expand(s: &str, vars: &HashMap<&str, String>) -> String {
    let mut out = s.to_string();
    for (k, v) in vars {
        out = out.replace(&format!("${{{k}}}"), v);
    }
    out
}

fn collect(list: Option<&Value>, vars: &HashMap<&str, String>) -> Vec<String> {
    let mut out = vec![];
    for item in list.and_then(|l| l.as_array()).into_iter().flatten() {
        match item {
            Value::String(s) => out.push(expand(s, vars)),
            Value::Object(_) => {
                if !versions::rules_allow(item.get("rules")) {
                    continue;
                }
                match &item["value"] {
                    Value::String(s) => out.push(expand(s, vars)),
                    Value::Array(a) => {
                        out.extend(a.iter().filter_map(|v| v.as_str()).map(|s| expand(s, vars)))
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
    out
}

/// Installs whatever is missing, starts the game and returns its PID.
/// `on_exit` fires once the game process ends.
pub async fn launch(
    inst: &Instance,
    opts: &LaunchOptions,
    progress: ProgressFn,
    log: LogFn,
    on_exit: ExitFn,
) -> Result<u32> {
    if inst.loader == "fabric" && cosmetics::mod_supports(&inst.mc_version) && !opts.equipped.is_empty() {
        if let Err(e) = cosmetics::ensure_mod(inst, &opts.cosmetics_mod_url, &progress).await {
            log(format!("[Atlas] Cosmetics-Mod konnte nicht installiert werden: {e:#}"));
        }
    }
    let prep = versions::prepare(inst, &progress).await?;
    let java_bin = java::resolve(prep.java_major, &opts.java_path, &progress).await?;

    let game_dir = instances::dir(&inst.id);
    std::fs::create_dir_all(&game_dir)?;
    let sep = if cfg!(windows) { ";" } else { ":" };
    let classpath = prep
        .classpath
        .iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .join(sep);

    let mut vars: HashMap<&str, String> = HashMap::new();
    vars.insert("auth_player_name", opts.account.name.clone());
    vars.insert("auth_uuid", opts.account.uuid.clone());
    vars.insert("auth_access_token", opts.account.access_token.clone());
    vars.insert("auth_session", opts.account.access_token.clone());
    vars.insert("user_type", opts.account.user_type.clone());
    vars.insert("user_properties", "{}".into());
    vars.insert("clientid", "atlas".into());
    vars.insert("auth_xuid", "0".into());
    vars.insert("version_name", prep.version_name.clone());
    vars.insert("version_type", "Atlas".into());
    vars.insert("game_directory", game_dir.to_string_lossy().to_string());
    vars.insert("assets_root", paths::assets().to_string_lossy().to_string());
    vars.insert("game_assets", paths::assets().to_string_lossy().to_string());
    vars.insert("assets_index_name", prep.asset_index.clone());
    vars.insert("natives_directory", prep.natives_dir.to_string_lossy().to_string());
    vars.insert("library_directory", paths::libraries().to_string_lossy().to_string());
    vars.insert("classpath", classpath.clone());
    vars.insert("classpath_separator", sep.into());
    vars.insert("launcher_name", "Atlas".into());
    vars.insert("launcher_version", env!("CARGO_PKG_VERSION").into());

    let ram = inst.ram_mb.unwrap_or(opts.ram_mb);
    let xms = if opts.perf_flags { ram / 2 } else { ram / 4 };
    let mut args: Vec<String> = vec![format!("-Xmx{ram}M"), format!("-Xms{}M", xms.max(512))];
    if opts.perf_flags {
        // G1 tuned for short, even frame times instead of server throughput.
        args.extend(
            [
                "-XX:+UseG1GC",
                "-XX:MaxGCPauseMillis=40",
                "-XX:G1NewSizePercent=30",
                "-XX:G1MaxNewSizePercent=50",
                "-XX:G1HeapRegionSize=16M",
                "-XX:G1ReservePercent=15",
                "-XX:InitiatingHeapOccupancyPercent=20",
                "-XX:+ParallelRefProcEnabled",
                "-XX:+DisableExplicitGC",
                "-XX:+PerfDisableSharedMem",
            ]
            .map(String::from),
        );
    }

    // JVM args: modern versions list them (incl. -cp); old ones need the defaults.
    let jvm = prep.version.get("arguments").and_then(|a| a.get("jvm"));
    if jvm.is_some() {
        args.extend(collect(jvm, &vars));
    } else {
        args.push(format!("-Djava.library.path={}", prep.natives_dir.display()));
        args.push("-cp".into());
        args.push(classpath);
    }

    // Hand the cosmetics to the in-game mod.
    args.push(format!("-Datlas.cosmetics.dir={}", cosmetics::files_dir().display()));
    args.push(format!("-Datlas.cosmetics.catalog={}", cosmetics::catalog_file().display()));
    args.push(format!("-Datlas.options={}", cosmetics::options_file().display()));
    args.push(format!("-Datlas.equipped={}", opts.equipped.join(",")));
    args.push(format!("-Datlas.account.uuid={}", opts.account.uuid));

    args.push(prep.main_class.clone());

    if let Some(game) = prep.version.get("arguments").and_then(|a| a.get("game")) {
        args.extend(collect(Some(game), &vars));
    } else if let Some(legacy) = prep.version["minecraftArguments"].as_str() {
        args.extend(legacy.split_whitespace().map(|s| expand(s, &vars)));
    }

    let mut cmd = tokio::process::Command::new(&java_bin);
    cmd.args(&args)
        .current_dir(&game_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(false);
    #[cfg(windows)]
    {
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }

    let mut child = cmd.spawn()?;
    let pid = child.id().unwrap_or(0);

    if let Some(out) = child.stdout.take() {
        let log = log.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(out).lines();
            while let Ok(Some(l)) = lines.next_line().await {
                log(l);
            }
        });
    }
    if let Some(err) = child.stderr.take() {
        let log = log.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(err).lines();
            while let Ok(Some(l)) = lines.next_line().await {
                log(l);
            }
        });
    }
    tokio::spawn(async move {
        let code = child.wait().await.ok().and_then(|s| s.code());
        on_exit(code);
    });

    Ok(pid)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn arguments_are_expanded_and_filtered() {
        let mut vars = HashMap::new();
        vars.insert("auth_player_name", "Robert".to_string());
        let list = json!([
            "--username", "${auth_player_name}",
            {"rules": [{"action": "allow", "features": {"is_demo_user": true}}], "value": "--demo"},
            {"rules": [{"action": "allow"}], "value": ["--a", "--b"]}
        ]);
        let out = collect(Some(&list), &vars);
        assert_eq!(out, vec!["--username", "Robert", "--a", "--b"]);
    }
}
