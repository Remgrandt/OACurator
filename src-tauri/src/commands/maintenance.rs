// Copyright (c) 2026 Remgrandt Works. All rights reserved.

use super::*;
use crate::catalog::{CatalogConsistencyCheck, CatalogConsistencyReport, ManifestRepairReport};
use crate::file_operations::{FileOperationRecoveryReport, FileOperationService};

#[tauri::command]
pub async fn catalog_consistency_check_command(
    state: tauri::State<'_, AppState>,
) -> std::result::Result<CatalogConsistencyReport, String> {
    let catalog = state.catalog.clone();
    catalog_blocking("Catalog consistency check", move || {
        CatalogConsistencyCheck::new(&catalog)
            .run()
            .map_err(|error| error.to_string())
    })
    .await
}

#[tauri::command]
pub async fn repair_manifest_projections_command(
    state: tauri::State<'_, AppState>,
) -> std::result::Result<ManifestRepairReport, String> {
    let catalog = state.catalog.clone();
    catalog_blocking("Repair manifest projections", move || {
        crate::catalog::ManifestRepairService::new(&catalog)
            .repair_dirty_projections()
            .map_err(|error| error.to_string())
    })
    .await
}

#[tauri::command]
pub async fn file_operation_recovery_report_command(
    state: tauri::State<'_, AppState>,
    artwork_id: i64,
) -> std::result::Result<FileOperationRecoveryReport, String> {
    let catalog = state.catalog.clone();
    catalog_blocking("File operation recovery report", move || {
        FileOperationService::new(&catalog)
            .recovery_report_for_artwork(artwork_id)
            .map_err(|error| error.to_string())
    })
    .await
}
