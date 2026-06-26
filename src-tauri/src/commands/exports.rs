use super::*;
use crate::diagnostics::{png_export_error, write_diagnostic_log, DiagnosticOperation};

#[tauri::command]
pub async fn export_oaa_archive_command(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    request: ExportOaaArchiveRequest,
) -> std::result::Result<OaaExportReport, String> {
    let catalog = state.catalog.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let emit_progress = move |progress| {
            let _ = app.emit("oaa-export-progress", progress);
        };
        export_oaa_archive_with_progress(
            &catalog,
            OaaExportOptions {
                collection_id: request.collection_id,
                archive_path: expand_user_path(&request.archive_path),
                include_images: request.include_images,
                include_private_metadata: request.include_private_metadata,
                allow_overwrite: request.allow_overwrite,
            },
            emit_progress,
        )
        .map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| format!("OAA export task failed: {error}"))?
}

#[tauri::command]
pub fn destination_file_exists_command(path: String) -> std::result::Result<bool, String> {
    let path = path.trim();
    if path.is_empty() {
        return Ok(false);
    }
    Ok(expand_user_path(path).is_file())
}

#[tauri::command]
pub async fn raremarq_csv_export_plan_command(
    state: tauri::State<'_, AppState>,
    collection_id: i64,
) -> std::result::Result<RaremarqCsvExportPlan, String> {
    let catalog = state.catalog.clone();
    tauri::async_runtime::spawn_blocking(move || {
        raremarq_csv_export_plan(&catalog, collection_id).map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| format!("Raremarq CSV export plan task failed: {error}"))?
}

#[tauri::command]
pub async fn export_raremarq_csv_command(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    request: ExportRaremarqCsvRequest,
) -> std::result::Result<RaremarqCsvExportReport, String> {
    let catalog = state.catalog.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let emit_progress = move |progress| {
            let _ = app.emit("raremarq-export-progress", progress);
        };
        export_raremarq_csv_with_progress(
            &catalog,
            RaremarqCsvExportOptions {
                collection_id: request.collection_id,
                csv_path: expand_user_path(&request.csv_path),
                scope: request.scope,
                url_mode: request.url_mode,
                allow_overwrite: request.allow_overwrite,
                confirmed_temporary_upload: request.confirmed_temporary_upload,
            },
            emit_progress,
        )
        .map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| format!("Raremarq CSV export task failed: {error}"))?
}

#[tauri::command]
pub async fn create_png_derivative_command(
    state: tauri::State<'_, AppState>,
    artwork_id: i64,
    source_file_asset_id: i64,
    export_root: String,
    variant: PngExportVariant,
) -> std::result::Result<DerivedAsset, String> {
    let catalog = state.catalog.clone();
    let cache_dir = state.cache_dir.clone();
    let export_root = expand_user_path(&export_root);
    tauri::async_runtime::spawn_blocking(move || {
        create_png_derivative(
            &catalog,
            artwork_id,
            source_file_asset_id,
            &export_root,
            variant,
        )
        .map_err(|error| {
            let presentation = png_export_error(&error);
            let _ = write_diagnostic_log(
                &cache_dir,
                DiagnosticOperation::PngExport,
                Some(&export_root),
                &presentation,
            );
            presentation.message
        })
    })
    .await
    .map_err(|error| error.to_string())?
}
