#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use atlas_core::{
    auth::Account,
    cosmetics::{self, Catalog},
    instances::{self, Instance},
    launch::{self, LaunchOptions},
    modrinth::{self, PackResult, SearchResult},
    settings::{self, CosmeticOption, Settings},
    versions::{self, VersionEntry},
    ProgressFn,
};
use serde::Serialize;
use std::sync::Arc;
use tauri::{AppHandle, Emitter};

type R<T> = Result<T, String>;

fn err<E: std::fmt::Display>(e: E) -> String {
    format!("{e:#}")
}

fn progress_fn(app: &AppHandle) -> ProgressFn {
    let app = app.clone();
    Arc::new(move |p| {
        let _ = app.emit("progress", p);
    })
}

#[tauri::command]
fn get_settings() -> Settings {
    settings::load()
}

#[tauri::command]
fn save_settings(settings: Settings) -> R<()> {
    Account::offline(&settings.username).map_err(err)?;
    if !settings::valid_hex(&settings.theme_accent) || !settings::valid_hex(&settings.theme_bg) {
        return Err("Farben müssen im Format #rrggbb sein".into());
    }
    settings::save(&settings).map_err(err)
}

#[tauri::command]
async fn mc_versions(snapshots: bool) -> R<Vec<VersionEntry>> {
    versions::list_versions(snapshots).await.map_err(err)
}

#[tauri::command]
fn list_instances() -> R<Vec<Instance>> {
    instances::list().map_err(err)
}

#[tauri::command]
async fn create_instance(name: String, mc_version: String, loader: String) -> R<Instance> {
    let loader_version = if loader == "fabric" {
        Some(versions::latest_fabric_loader(&mc_version).await.map_err(err)?)
    } else {
        None
    };
    instances::create(&name, &mc_version, &loader, loader_version).map_err(err)
}

#[tauri::command]
fn delete_instance(id: String) -> R<()> {
    instances::delete(&id).map_err(err)
}

#[tauri::command]
async fn play(app: AppHandle, id: String) -> R<u32> {
    let inst = instances::get(&id).map_err(err)?;
    let s = settings::load();
    let account = Account::offline(&s.username).map_err(err)?;
    let opts = LaunchOptions {
        account,
        ram_mb: s.ram_mb,
        java_path: s.java_path,
        equipped: s.equipped,
        cosmetics_mod_url: s.cosmetics_mod_url,
        perf_flags: s.perf_flags,
    };

    // The mod reads the merged catalog and the per-cosmetic options at startup.
    cosmetics::write_merged().map_err(err)?;
    cosmetics::write_options(&s.cosmetic_options).map_err(err)?;

    let log_app = app.clone();
    let log_id = id.clone();
    let log = Arc::new(move |line: String| {
        let _ = log_app.emit("game-log", serde_json::json!({ "id": log_id, "line": line }));
    });
    let exit_app = app.clone();
    let exit_id = id.clone();
    let on_exit = Arc::new(move |code: Option<i32>| {
        let _ = exit_app.emit("game-exit", serde_json::json!({ "id": exit_id, "code": code }));
    });

    launch::launch(&inst, &opts, progress_fn(&app), log, on_exit)
        .await
        .map_err(err)
}

#[tauri::command]
async fn modrinth_search(
    query: String,
    project_type: String,
    mc_version: Option<String>,
    loader: Option<String>,
    offset: u32,
) -> R<SearchResult> {
    modrinth::search(&query, &project_type, mc_version.as_deref(), loader.as_deref(), offset)
        .await
        .map_err(err)
}

#[tauri::command]
async fn modrinth_install(
    app: AppHandle,
    project_id: String,
    project_type: String,
    instance_id: String,
) -> R<Vec<String>> {
    let inst = instances::get(&instance_id).map_err(err)?;
    modrinth::install_project(&project_id, &project_type, &inst, &progress_fn(&app))
        .await
        .map_err(err)
}

#[tauri::command]
async fn modrinth_install_modpack(app: AppHandle, project_id: String) -> R<Instance> {
    modrinth::install_modpack(&project_id, &progress_fn(&app))
        .await
        .map_err(err)
}

#[derive(Serialize)]
struct CosmeticsState {
    catalog: Catalog,
    equipped: Vec<String>,
}

#[tauri::command]
fn cosmetics_list() -> CosmeticsState {
    CosmeticsState {
        catalog: cosmetics::list(),
        equipped: settings::load().equipped,
    }
}

#[tauri::command]
async fn cosmetics_sync(app: AppHandle) -> R<CosmeticsState> {
    let url = settings::load().cosmetics_url;
    cosmetics::sync(&url, &progress_fn(&app)).await.map_err(err)?;
    Ok(cosmetics_list())
}

#[tauri::command]
fn cosmetics_equip(id: String) -> R<Vec<String>> {
    let mut s = settings::load();
    s.equipped = cosmetics::equip(&s.equipped, &id);
    settings::save(&s).map_err(err)?;
    Ok(s.equipped)
}

#[tauri::command]
fn cosmetics_unequip(id: String) -> R<Vec<String>> {
    let mut s = settings::load();
    s.equipped.retain(|e| e != &id);
    settings::save(&s).map_err(err)?;
    Ok(s.equipped)
}

#[tauri::command]
fn cosmetics_set_option(id: String, scale: f32, color: String) -> R<()> {
    if !color.is_empty() && !settings::valid_hex(&color) {
        return Err("Farbe muss #rrggbb sein".into());
    }
    let mut s = settings::load();
    s.cosmetic_options.insert(
        id,
        CosmeticOption {
            scale: scale.clamp(0.3, 3.0),
            color,
        },
    );
    settings::save(&s).map_err(err)
}

#[tauri::command]
fn cosmetics_add_cape(name: String, png: String) -> R<cosmetics::Cosmetic> {
    cosmetics::add_cape(&name, &png).map_err(err)
}

#[tauri::command]
fn cosmetics_delete(id: String) -> R<()> {
    cosmetics::delete_custom(&id).map_err(err)?;
    let mut s = settings::load();
    s.equipped.retain(|e| e != &id);
    s.cosmetic_options.remove(&id);
    settings::save(&s).map_err(err)
}

#[tauri::command]
async fn install_performance_pack(app: AppHandle, instance_id: String) -> R<PackResult> {
    let inst = instances::get(&instance_id).map_err(err)?;
    modrinth::install_performance_pack(&inst, &progress_fn(&app))
        .await
        .map_err(err)
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            get_settings,
            save_settings,
            mc_versions,
            list_instances,
            create_instance,
            delete_instance,
            play,
            modrinth_search,
            modrinth_install,
            modrinth_install_modpack,
            cosmetics_list,
            cosmetics_sync,
            cosmetics_equip,
            cosmetics_unequip,
            cosmetics_set_option,
            cosmetics_add_cape,
            cosmetics_delete,
            install_performance_pack,
        ])
        .run(tauri::generate_context!())
        .expect("Atlas konnte nicht gestartet werden");
}
