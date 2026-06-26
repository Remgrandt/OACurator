use super::cache::start_thumbnail_cache_generation;
use super::*;

#[tauri::command]
pub async fn workspace_state_command(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    search_query: Option<String>,
) -> std::result::Result<WorkspaceState, String> {
    let catalog = state.catalog.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let emit_progress = move |progress: WorkspaceLoadProgress| {
            let _ = app.emit("workspace-load-progress", progress);
        };
        catalog
            .workspace_state_with_search_and_progress(search_query.as_deref(), emit_progress)
            .map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| format!("Workspace state task failed: {error}"))?
}

#[tauri::command]
pub async fn open_collection_command(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    request: OpenManifestRequest,
) -> std::result::Result<CollectionSummary, String> {
    let catalog = state.catalog.clone();
    let path = PathBuf::from(request.path);
    let progress_app = app.clone();
    let collection = tauri::async_runtime::spawn_blocking(move || {
        let emit_progress = move |progress: WorkspaceLoadProgress| {
            let _ = progress_app.emit("workspace-load-progress", progress);
        };
        catalog
            .open_collection_with_progress(&path, emit_progress)
            .map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| format!("Open Collection task failed: {error}"))??;
    start_thumbnail_cache_generation(
        app,
        state.catalog.clone(),
        state.cache_dir.clone(),
        collection.id,
    );
    Ok(collection)
}

#[tauri::command]
pub async fn close_collection_command(
    state: tauri::State<'_, AppState>,
) -> std::result::Result<WorkspaceState, String> {
    let catalog = state.catalog.clone();
    catalog_blocking("Close collection", move || {
        catalog
            .close_collection()
            .map_err(|error| error.to_string())?;
        catalog.workspace_state().map_err(|error| error.to_string())
    })
    .await
}
