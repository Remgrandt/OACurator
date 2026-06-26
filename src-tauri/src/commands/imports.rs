use super::cache::start_thumbnail_cache_generation;
use super::*;

fn required_import_destination_root(
    destination_root: Option<String>,
) -> std::result::Result<PathBuf, crate::AppError> {
    let destination_root = destination_root
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            crate::AppError::Message(
                "Destination folder is required when importing a new Collection".to_string(),
            )
        })?;
    Ok(expand_user_path(&destination_root))
}

#[tauri::command]
pub async fn import_caf_csv_command(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    request: ImportCafCsvRequest,
) -> std::result::Result<CafImportReport, String> {
    let catalog = state.catalog.clone();
    let cache_dir = state.cache_dir.clone();
    let csv_path = expand_user_path(&request.csv_path);
    let destination_root = request.destination_root;
    let target_collection_id = request.target_collection_id;
    let allow_caf_collection_id_override = request.allow_caf_collection_id_override;
    tauri::async_runtime::spawn_blocking(move || {
        let emit_progress = move |progress| {
            let _ = app.emit("caf-import-progress", progress);
        };
        if let Some(collection_id) = target_collection_id {
            import_caf_csv_into_collection_public_with_progress(
                &catalog,
                &csv_path,
                collection_id,
                &cache_dir,
                allow_caf_collection_id_override,
                emit_progress,
            )
        } else {
            let destination_root = required_import_destination_root(destination_root)
                .map_err(|error| error.to_string())?;
            import_caf_csv_public_with_progress(
                &catalog,
                &csv_path,
                &destination_root,
                &cache_dir,
                emit_progress,
            )
        }
        .map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| format!("CAF CSV import task failed: {error}"))?
}

#[tauri::command]
pub async fn write_caf_missing_report_command(
    request: WriteCafMissingReportRequest,
) -> std::result::Result<usize, String> {
    tauri::async_runtime::spawn_blocking(move || {
        write_caf_missing_artwork_report(
            &expand_user_path(&request.path),
            &request.rows,
            request.include_private_metadata,
        )
        .map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| format!("CAF missing report task failed: {error}"))?
}

#[tauri::command]
pub async fn resolve_caf_reconciliation_command(
    state: tauri::State<'_, AppState>,
    request: ResolveCafReconciliationRequest,
) -> std::result::Result<ArtworkSummary, String> {
    let catalog = state.catalog.clone();
    tauri::async_runtime::spawn_blocking(move || {
        resolve_caf_csv_reconciliation(&catalog, request.item, request.target_artwork_id)
            .map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| format!("CAF reconciliation task failed: {error}"))?
}

#[tauri::command]
pub async fn try_auto_resolve_caf_reconciliation_command(
    state: tauri::State<'_, AppState>,
    request: TryAutoResolveCafReconciliationRequest,
) -> std::result::Result<Option<ArtworkSummary>, String> {
    let catalog = state.catalog.clone();
    tauri::async_runtime::spawn_blocking(move || {
        try_auto_resolve_caf_csv_reconciliation(&catalog, request.item)
            .map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| format!("CAF reconciliation auto-resolve task failed: {error}"))?
}

#[tauri::command]
pub async fn import_oaa_archive_command(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    request: ImportOaaArchiveRequest,
) -> std::result::Result<OaaImportReport, String> {
    let catalog = state.catalog.clone();
    let cache_dir = state.cache_dir.clone();
    let import_app = app.clone();
    let import_catalog = catalog.clone();
    let import_cache_dir = cache_dir.clone();
    let report = tauri::async_runtime::spawn_blocking(move || {
        let emit_progress = move |progress| {
            let _ = import_app.emit("oaa-import-progress", progress);
        };
        import_oaa_archive_with_progress(
            &import_catalog,
            OaaImportOptions {
                archive_path: expand_user_path(&request.archive_path),
                destination_root: request.destination_root.map(|path| expand_user_path(&path)),
                target_collection_id: request.target_collection_id,
                cache_dir: import_cache_dir,
            },
            emit_progress,
        )
        .map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| format!("OAA import task failed: {error}"))??;
    start_thumbnail_cache_generation(app, catalog, cache_dir, report.collection_id);
    Ok(report)
}

#[tauri::command]
pub async fn import_snikt_collection_command(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    request: ImportSniktCollectionRequest,
) -> std::result::Result<SniktImportReport, String> {
    let catalog = state.catalog.clone();
    let csv_path = expand_user_path(&request.csv_path);
    let destination_root = request.destination_root;
    let target_collection_id = request.target_collection_id;
    tauri::async_runtime::spawn_blocking(move || {
        let emit_progress = move |progress| {
            let _ = app.emit("snikt-import-progress", progress);
        };
        if let Some(collection_id) = target_collection_id {
            import_snikt_csv_into_collection_public_with_progress(
                &catalog,
                &csv_path,
                collection_id,
                emit_progress,
            )
        } else {
            let destination_root = required_import_destination_root(destination_root)
                .map_err(|error| error.to_string())?;
            import_snikt_csv_public_with_progress(
                &catalog,
                &csv_path,
                &destination_root,
                emit_progress,
            )
        }
        .map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| format!("SNIKT.com CSV import task failed: {error}"))?
}

#[tauri::command]
pub async fn resolve_snikt_reconciliation_command(
    state: tauri::State<'_, AppState>,
    request: ResolveSniktReconciliationRequest,
) -> std::result::Result<ArtworkSummary, String> {
    let catalog = state.catalog.clone();
    tauri::async_runtime::spawn_blocking(move || {
        resolve_snikt_csv_reconciliation(&catalog, request.item, request.target_artwork_id)
            .map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| format!("SNIKT.com reconciliation task failed: {error}"))?
}
