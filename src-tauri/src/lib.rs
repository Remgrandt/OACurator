// Copyright (c) 2026 Remgrandt Works. All rights reserved.

pub mod caf_import;
pub mod catalog;
pub mod commands;
pub mod export;
pub mod export_policy;
pub mod file_operations;
pub mod file_ops;
pub mod image_metadata;
pub mod image_render;
pub mod jobs;
pub mod manifest;
pub mod oaa_archive;
pub mod oaa_validation;
pub mod path_safety;
pub mod raremarq_export;
pub mod scanner;
pub mod snikt_import;

use catalog::Catalog;
use commands::AppState;
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::PathBuf;
use std::process;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::Manager;

static STARTUP_TRACE_EVENTS: OnceLock<Mutex<Vec<StartupTraceEvent>>> = OnceLock::new();

pub type Result<T> = std::result::Result<T, AppError>;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("file error: {0}")]
    Io(#[from] std::io::Error),
    #[error("image error: {0}")]
    Image(#[from] image::ImageError),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("zip error: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("{0}")]
    Message(String),
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct StartupTraceEvent {
    name: String,
    category: String,
    timestamp_ms: f64,
    source: String,
    detail: Option<serde_json::Value>,
}

pub fn app_dirs() -> Result<(std::path::PathBuf, std::path::PathBuf)> {
    if let Some(data_dir) = env_path("OACURATOR_DATA_DIR") {
        let cache_dir = env_path("OACURATOR_CACHE_DIR").unwrap_or_else(|| data_dir.join("cache"));
        fs::create_dir_all(&data_dir)?;
        fs::create_dir_all(&cache_dir)?;
        return Ok((data_dir, cache_dir));
    }
    let dirs = ProjectDirs::from("com", "OACurator", "OA Curator")
        .ok_or_else(|| AppError::Message("Could not resolve app data directories".to_string()))?;
    let data_dir = dirs.data_local_dir().to_path_buf();
    let cache_dir = dirs.cache_dir().to_path_buf();
    fs::create_dir_all(&data_dir)?;
    fs::create_dir_all(&cache_dir)?;
    Ok((data_dir, cache_dir))
}

fn env_path(name: &str) -> Option<std::path::PathBuf> {
    env::var_os(name)
        .map(std::path::PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty())
}

#[tauri::command]
fn finish_startup_trace_command(
    frontend_events: Vec<StartupTraceEvent>,
) -> std::result::Result<Option<String>, String> {
    let Some(output_dir) = startup_trace_output_dir() else {
        return Ok(None);
    };
    startup_trace_mark("rust_trace_write_begin", "rust", None);
    let mut events = startup_trace_snapshot();
    events.extend(frontend_events);
    events.sort_by(|left, right| {
        left.timestamp_ms
            .partial_cmp(&right.timestamp_ms)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    fs::create_dir_all(&output_dir).map_err(|error| error.to_string())?;

    let raw_path = output_dir.join("oac-startup-events.json");
    let chrome_path = output_dir.join("oac-startup-chrome-trace.json");
    let summary_path = output_dir.join("oac-startup-summary.json");

    fs::write(
        &raw_path,
        serde_json::to_vec_pretty(&events).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;

    let first_timestamp = events
        .first()
        .map(|event| event.timestamp_ms)
        .unwrap_or_else(now_ms);
    let chrome_events: Vec<_> = events
        .iter()
        .map(|event| {
            json!({
                "name": event.name,
                "cat": event.category,
                "ph": "i",
                "s": "t",
                "ts": ((event.timestamp_ms - first_timestamp) * 1000.0).max(0.0),
                "pid": process::id(),
                "tid": event.source,
                "args": event.detail.clone().unwrap_or_else(|| json!({})),
            })
        })
        .collect();
    fs::write(
        &chrome_path,
        serde_json::to_vec_pretty(&json!({ "traceEvents": chrome_events }))
            .map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;

    let summary = json!({
        "eventCount": events.len(),
        "firstEvent": events.first(),
        "lastEvent": events.last(),
        "idleMsFromFirstEvent": event_delta_ms(&events, first_timestamp, "frontend_idle_ready"),
        "firstPaintMsFromFirstEvent": event_delta_ms(&events, first_timestamp, "paint:first-paint"),
        "firstContentfulPaintMsFromFirstEvent": event_delta_ms(&events, first_timestamp, "paint:first-contentful-paint"),
        "catalogInitEndMsFromFirstEvent": event_delta_ms(&events, first_timestamp, "catalog_init_end"),
        "workspaceStateEndMsFromFirstEvent": event_delta_ms(&events, first_timestamp, "workspace_state_command_end"),
        "defaultRootEndMsFromFirstEvent": event_delta_ms(&events, first_timestamp, "default_oac_root_command_end"),
        "eventsPath": raw_path,
        "chromeTracePath": chrome_path,
    });
    fs::write(
        &summary_path,
        serde_json::to_vec_pretty(&summary).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;

    Ok(Some(summary_path.to_string_lossy().to_string()))
}

fn event_delta_ms(
    events: &[StartupTraceEvent],
    first_timestamp: f64,
    event_name: &str,
) -> Option<f64> {
    events
        .iter()
        .find(|event| event.name == event_name)
        .map(|event| event.timestamp_ms - first_timestamp)
}

fn startup_trace_mark(name: &str, category: &str, detail: Option<serde_json::Value>) {
    let event = StartupTraceEvent {
        name: name.to_string(),
        category: category.to_string(),
        timestamp_ms: now_ms(),
        source: "rust".to_string(),
        detail,
    };
    startup_trace_events()
        .lock()
        .expect("startup trace event lock")
        .push(event);
}

fn startup_trace_snapshot() -> Vec<StartupTraceEvent> {
    startup_trace_events()
        .lock()
        .expect("startup trace event lock")
        .clone()
}

fn startup_trace_events() -> &'static Mutex<Vec<StartupTraceEvent>> {
    STARTUP_TRACE_EVENTS.get_or_init(|| {
        Mutex::new(vec![StartupTraceEvent {
            name: "rust_trace_store_init".to_string(),
            category: "rust".to_string(),
            timestamp_ms: now_ms(),
            source: "rust".to_string(),
            detail: None,
        }])
    })
}

fn now_ms() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time before unix epoch")
        .as_secs_f64()
        * 1000.0
}

fn startup_trace_output_dir_from_env(path: Option<OsString>) -> Option<PathBuf> {
    path.map(PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty())
}

fn startup_trace_output_dir() -> Option<PathBuf> {
    startup_trace_output_dir_from_env(env::var_os("OACURATOR_STARTUP_TRACE_DIR"))
}

fn focus_main_window<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    startup_trace_mark("rust_run_enter", "rust", None);
    startup_trace_mark("app_dirs_begin", "rust", None);
    let (data_dir, cache_dir) = app_dirs().expect("resolve OA Curator app directories");
    startup_trace_mark(
        "app_dirs_end",
        "rust",
        Some(json!({
            "dataDir": data_dir.to_string_lossy(),
            "cacheDir": cache_dir.to_string_lossy(),
        })),
    );
    startup_trace_mark("catalog_open_begin", "rust", None);
    let catalog =
        Catalog::open(data_dir.join("oa-curator.sqlite3")).expect("open OA Curator catalog");
    startup_trace_mark("catalog_open_end", "rust", None);
    startup_trace_mark("catalog_init_begin", "rust", None);
    catalog.init().expect("initialize OA Curator catalog");
    startup_trace_mark("catalog_init_end", "rust", None);

    startup_trace_mark("tauri_builder_begin", "rust", None);
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            focus_main_window(app);
        }))
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .setup(|_| {
            startup_trace_mark("tauri_setup_enter", "rust", None);
            startup_trace_mark("tauri_setup_exit", "rust", None);
            Ok(())
        })
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::new(catalog, cache_dir))
        .invoke_handler(tauri::generate_handler![
            commands::workspace::workspace_state_command,
            commands::preferences::default_oac_root_command,
            commands::preferences::app_preferences_command,
            commands::preferences::set_app_preferences_command,
            commands::preferences::recent_collections_command,
            commands::preferences::create_collection_command,
            commands::preferences::artwork_id_label_preference_command,
            commands::preferences::set_artwork_id_label_preference_command,
            commands::imports::import_caf_csv_command,
            commands::imports::write_caf_missing_report_command,
            commands::imports::resolve_caf_reconciliation_command,
            commands::imports::try_auto_resolve_caf_reconciliation_command,
            commands::imports::import_oaa_archive_command,
            commands::exports::export_oaa_archive_command,
            commands::exports::destination_file_exists_command,
            commands::exports::raremarq_csv_export_plan_command,
            commands::exports::export_raremarq_csv_command,
            commands::imports::import_snikt_collection_command,
            commands::imports::resolve_snikt_reconciliation_command,
            commands::workspace::open_collection_command,
            commands::workspace::close_collection_command,
            commands::jobs::list_jobs_command,
            commands::jobs::cancel_job_command,
            commands::maintenance::catalog_consistency_check_command,
            commands::maintenance::repair_manifest_projections_command,
            commands::maintenance::file_operation_recovery_report_command,
            commands::artwork::create_gallery_command,
            commands::artwork::create_artwork_command,
            commands::artwork::attach_file_assets_command,
            commands::artwork::add_artwork_to_gallery_command,
            commands::artwork::preview_delete_collection_command,
            commands::artwork::preview_delete_gallery_command,
            commands::artwork::preview_delete_artwork_command,
            commands::artwork::preview_delete_artwork_file_command,
            commands::artwork::preview_rename_artwork_file_command,
            commands::artwork::delete_collection_command,
            commands::artwork::delete_gallery_command,
            commands::artwork::delete_artwork_command,
            commands::artwork::delete_artwork_file_command,
            commands::artwork::rename_collection_command,
            commands::artwork::rename_gallery_command,
            commands::artwork::save_collection_provider_ids_command,
            commands::artwork::save_gallery_provider_ids_command,
            commands::artwork::merge_gallery_command,
            commands::artwork::merge_artwork_command,
            commands::artwork::rename_artwork_command,
            commands::artwork::execute_file_rename_command,
            commands::artwork::select_gallery_command,
            commands::artwork::list_artworks_command,
            commands::artwork::artwork_detail_command,
            commands::artwork::ensure_artwork_thumbnail_command,
            commands::artwork::save_metadata_command,
            commands::artwork::save_image_metadata_command,
            commands::artwork::reorder_file_assets_command,
            commands::artwork::snikt_upload_prefill_url_command,
            commands::exports::create_png_derivative_command,
            commands::images::show_path_in_file_manager_command,
            commands::images::cache_image_data_url_command,
            commands::images::file_asset_image_data_url_command,
            commands::images::derived_asset_image_data_url_command,
            finish_startup_trace_command
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::startup_trace_output_dir_from_env;
    use std::ffi::OsString;
    use std::path::PathBuf;

    #[test]
    fn startup_trace_output_dir_is_opt_in() {
        assert_eq!(startup_trace_output_dir_from_env(None), None);
        assert_eq!(
            startup_trace_output_dir_from_env(Some(OsString::new())),
            None
        );
        assert_eq!(
            startup_trace_output_dir_from_env(Some(OsString::from("C:\\trace"))),
            Some(PathBuf::from("C:\\trace"))
        );
    }
}
