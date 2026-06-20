// Copyright (c) 2026 Remgrandt Works. All rights reserved.

use crate::caf_import::{
    import_caf_csv_into_collection_public_with_progress, import_caf_csv_public_with_progress,
    resolve_caf_csv_reconciliation, try_auto_resolve_caf_csv_reconciliation,
    write_caf_missing_artwork_report, CafImportReconciliationItem, CafImportReport,
    CafMissingArtworkReportRow,
};
use crate::catalog::{
    art_type_id_for_label, artist_role_id_for_label, artist_role_label_for_id,
    media_type_id_for_label, AppPreferences, ArtistCreditUpdate, ArtworkDetail,
    ArtworkIdLabelPreference, ArtworkMergeUpdate, ArtworkSummary, AssetKind, Catalog,
    CollectionSummary, DeleteArtworkFileResult, DeletePreview, DeleteResult, DerivedAsset,
    FileRenameExecution, FileRenamePlan, FileRenameResult, GalleryMergeUpdate, GallerySummary,
    MetadataUpdate, RecentCollection, WorkspaceLoadProgress, WorkspaceState,
};
use crate::export::{create_png_derivative, PngExportVariant};
use crate::file_operations::FileOperationService;
use crate::jobs::{JobCancellation, JobResult, JobService};
use crate::oaa_archive::{
    export_oaa_archive_with_progress, import_oaa_archive_with_progress, OaaExportOptions,
    OaaExportReport, OaaImportOptions, OaaImportReport,
};
use crate::raremarq_export::{
    export_raremarq_csv_with_progress, raremarq_csv_export_plan, RaremarqCsvExportOptions,
    RaremarqCsvExportPlan, RaremarqCsvExportReport, RaremarqCsvExportScope, RaremarqCsvUrlMode,
};
use crate::scanner::{
    attach_files_to_artwork, ensure_artwork_cache_derivatives,
    ensure_artwork_cache_derivatives_with_warnings,
    generate_cache_derivatives_parallel_with_cancellation, preview_cache_work_items_for_collection,
    register_thumbnail_cache_work_result, thumbnail_cache_work_items_for_collection,
    thumbnail_cache_worker_count, AttachMode, CacheDerivativeOptions, ThumbnailCacheProgress,
    ThumbnailCacheWorkItem,
};
use crate::snikt_import::{
    import_snikt_csv_into_collection_public_with_progress, import_snikt_csv_public_with_progress,
    resolve_snikt_csv_reconciliation, SniktImportReconciliationItem, SniktImportReport,
};
use directories::UserDirs;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tauri::Emitter;

pub(crate) mod artwork;
mod cache;
pub(crate) mod exports;
pub(crate) mod images;
pub(crate) mod imports;
pub(crate) mod jobs;
pub(crate) mod maintenance;
pub(crate) mod preferences;
pub(crate) mod workspace;

pub use artwork::*;
pub use exports::*;
pub use images::{
    cache_image_data_url_command, derived_asset_image_data_url_command,
    file_asset_image_data_url_command, show_path_in_file_manager_command,
};
pub use imports::*;
pub use jobs::*;
pub use maintenance::*;
pub use preferences::*;
pub use workspace::*;

pub struct AppState {
    pub catalog: Catalog,
    pub cache_dir: PathBuf,
    pub jobs: JobService,
}

impl AppState {
    pub fn new(catalog: Catalog, cache_dir: PathBuf) -> Self {
        Self {
            catalog,
            cache_dir,
            jobs: JobService::default(),
        }
    }
}

async fn catalog_blocking<T, F>(operation: &'static str, task: F) -> std::result::Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> std::result::Result<T, String> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(task)
        .await
        .map_err(|error| format!("{operation} task failed: {error}"))?
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaveMetadataRequest {
    pub artwork_id: i64,
    pub title: String,
    pub description: Option<String>,
    pub for_sale_status: Option<String>,
    pub media_type_id: Option<String>,
    pub art_type_id: Option<String>,
    pub publication_status_id: Option<String>,
    pub active: Option<bool>,
    pub illustration_exchange: Option<bool>,
    pub ix_for_sale: Option<bool>,
    #[serde(default)]
    pub artist_credits: Vec<ArtistCreditRequest>,
    pub media: Option<String>,
    pub format: Option<String>,
    pub caf_url: Option<String>,
    pub snikt_url: Option<String>,
    pub raremarq_url: Option<String>,
    pub generic_url: Option<String>,
    #[serde(default)]
    pub snikt_metadata: Option<crate::catalog::SniktMetadataUpdate>,
    pub purchase_price: Option<String>,
    pub estimated_value: Option<String>,
    pub purchase_date: Option<String>,
    pub provenance: Option<String>,
    pub personal_notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtistCreditRequest {
    pub name: Option<String>,
    pub role: Option<String>,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub role_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateCollectionRequest {
    pub name: String,
    pub path: String,
    pub caf_collection_id: Option<String>,
    pub snikt_collection_id: Option<String>,
    pub raremarq_collection_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateGalleryRequest {
    pub name: String,
    pub path: String,
    pub collection_id: Option<i64>,
    pub caf_gallery_room_id: Option<String>,
    pub raremarq_gallery_id: Option<String>,
    #[serde(default = "default_true")]
    pub snikt_gallery_inherits_collection: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaveCollectionProviderIdsRequest {
    pub collection_id: i64,
    pub caf_collection_id: Option<String>,
    pub snikt_collection_id: Option<String>,
    pub raremarq_collection_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaveGalleryProviderIdsRequest {
    pub gallery_id: i64,
    pub caf_gallery_room_id: Option<String>,
    pub raremarq_gallery_id: Option<String>,
    #[serde(default)]
    pub snikt_gallery_inherits_collection: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeGalleryRequest {
    pub collection_id: i64,
    pub source_gallery_id: i64,
    pub target_gallery_id: i64,
    pub name: String,
    pub caf_gallery_room_id: Option<String>,
    pub raremarq_gallery_id: Option<String>,
    pub snikt_gallery_inherits_collection: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeArtworkRequest {
    pub collection_id: i64,
    pub source_gallery_id: i64,
    pub source_artwork_id: i64,
    pub target_artwork_id: i64,
    pub metadata: SaveMetadataRequest,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddArtworkToGalleryRequest {
    pub collection_id: i64,
    pub artwork_id: i64,
    pub gallery_id: i64,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenManifestRequest {
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportCafCsvRequest {
    pub csv_path: String,
    #[serde(default)]
    pub destination_root: Option<String>,
    #[serde(default)]
    pub target_collection_id: Option<i64>,
    #[serde(default)]
    pub allow_caf_collection_id_override: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WriteCafMissingReportRequest {
    pub path: String,
    pub rows: Vec<CafMissingArtworkReportRow>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolveCafReconciliationRequest {
    pub item: CafImportReconciliationItem,
    pub target_artwork_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TryAutoResolveCafReconciliationRequest {
    pub item: CafImportReconciliationItem,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportSniktCollectionRequest {
    pub csv_path: String,
    #[serde(default)]
    pub destination_root: Option<String>,
    #[serde(default)]
    pub target_collection_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolveSniktReconciliationRequest {
    pub item: SniktImportReconciliationItem,
    pub target_artwork_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportOaaArchiveRequest {
    pub archive_path: String,
    #[serde(default)]
    pub destination_root: Option<String>,
    #[serde(default)]
    pub target_collection_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportOaaArchiveRequest {
    pub collection_id: i64,
    pub archive_path: String,
    #[serde(default = "default_include_oaa_images")]
    pub include_images: bool,
    #[serde(default = "default_include_private_metadata")]
    pub include_private_metadata: bool,
    #[serde(default)]
    pub allow_overwrite: bool,
}

fn default_include_oaa_images() -> bool {
    true
}

fn default_include_private_metadata() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportRaremarqCsvRequest {
    pub collection_id: i64,
    pub csv_path: String,
    pub scope: RaremarqCsvExportScope,
    pub url_mode: RaremarqCsvUrlMode,
    #[serde(default)]
    pub allow_overwrite: bool,
    #[serde(default)]
    pub confirmed_temporary_upload: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateArtworkRequest {
    pub gallery_id: i64,
    pub title: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttachFileAssetsRequest {
    pub artwork_id: i64,
    pub paths: Vec<String>,
    pub mode: Option<AttachMode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteGalleryRequest {
    pub gallery_id: i64,
    pub collection_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteArtworkRequest {
    pub artwork_id: i64,
    pub gallery_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteArtworkFileRequest {
    pub asset_kind: AssetKind,
    pub asset_id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenameArtworkFileRequest {
    pub asset_kind: AssetKind,
    pub asset_id: i64,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecuteFileRenameRequest {
    pub plan: FileRenamePlan,
    #[serde(default)]
    pub confirmed_physical_file_rename: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaveImageMetadataRequest {
    pub asset_kind: AssetKind,
    pub asset_id: i64,
    pub image_role: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReorderFileAssetsRequest {
    pub artwork_id: i64,
    pub file_asset_ids: Vec<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SniktUploadPrefillUrlRequest {
    pub artwork_id: i64,
}
