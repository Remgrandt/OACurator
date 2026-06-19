// Copyright (c) 2026 Remgrandt Works. All rights reserved.

use super::*;

#[tauri::command]
pub fn default_oac_root_command() -> std::result::Result<String, String> {
    default_oac_root()
}

fn default_oac_root() -> std::result::Result<String, String> {
    let home_dir = UserDirs::new()
        .map(|dirs| dirs.home_dir().to_path_buf())
        .or_else(|| std::env::var_os("USERPROFILE").map(PathBuf::from))
        .ok_or_else(|| "Could not resolve the user home folder".to_string())?;
    Ok(home_dir.join("OACurator").to_string_lossy().to_string())
}

#[tauri::command]
pub fn app_preferences_command(
    state: tauri::State<'_, AppState>,
) -> std::result::Result<AppPreferences, String> {
    let default_workspace_root = default_oac_root()?;
    state
        .catalog
        .app_preferences(&default_workspace_root)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn set_app_preferences_command(
    state: tauri::State<'_, AppState>,
    preferences: AppPreferences,
) -> std::result::Result<WorkspaceState, String> {
    let catalog = state.catalog.clone();
    catalog_blocking("Set app preferences", move || {
        catalog
            .set_app_preferences(preferences)
            .map_err(|error| error.to_string())?;
        catalog.workspace_state().map_err(|error| error.to_string())
    })
    .await
}

#[tauri::command]
pub fn recent_collections_command(
    state: tauri::State<'_, AppState>,
) -> std::result::Result<Vec<RecentCollection>, String> {
    state
        .catalog
        .recent_collections()
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn create_collection_command(
    state: tauri::State<'_, AppState>,
    request: CreateCollectionRequest,
) -> std::result::Result<CollectionSummary, String> {
    let catalog = state.catalog.clone();
    catalog_blocking("Create collection", move || {
        catalog
            .create_collection_with_provider_ids(
                &request.name,
                Path::new(&request.path),
                request.caf_collection_id.as_deref(),
                request.snikt_collection_id.as_deref(),
                request.raremarq_collection_id.as_deref(),
            )
            .map_err(|error| error.to_string())
    })
    .await
}

#[tauri::command]
pub fn artwork_id_label_preference_command(
    state: tauri::State<'_, AppState>,
) -> std::result::Result<String, String> {
    state
        .catalog
        .artwork_id_label_preference()
        .map(|preference| preference.as_setting_value().to_string())
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn set_artwork_id_label_preference_command(
    state: tauri::State<'_, AppState>,
    preference: String,
) -> std::result::Result<WorkspaceState, String> {
    let preference = ArtworkIdLabelPreference::from_setting_value(&preference)
        .map_err(|error| error.to_string())?;
    let catalog = state.catalog.clone();
    catalog_blocking("Set artwork ID label preference", move || {
        catalog
            .set_artwork_id_label_preference(preference)
            .map_err(|error| error.to_string())?;
        catalog.workspace_state().map_err(|error| error.to_string())
    })
    .await
}
