use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtworkSummary {
    pub id: i64,
    pub canonical_id: String,
    pub display_id: String,
    pub caf_artwork_id: Option<String>,
    pub snikt_artwork_id: Option<String>,
    pub raremarq_artwork_id: Option<String>,
    pub title: String,
    pub media: Option<String>,
    pub format: Option<String>,
    pub source_folder: PathBuf,
    pub thumbnail_path: Option<PathBuf>,
    pub file_count: i64,
    pub manifest_path: Option<PathBuf>,
    pub gallery_ids: Vec<i64>,
    pub gallery_names: Vec<String>,
    pub artist_credits: Vec<ArtistCredit>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionSummary {
    pub id: i64,
    pub stable_id: String,
    pub name: String,
    pub manifest_path: PathBuf,
    pub caf_collection_id: Option<String>,
    pub snikt_collection_id: Option<String>,
    pub raremarq_collection_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecentCollection {
    pub name: String,
    pub path: PathBuf,
    pub last_opened_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GallerySummary {
    pub id: i64,
    pub stable_id: String,
    pub name: String,
    pub manifest_path: PathBuf,
    pub caf_gallery_room_id: Option<String>,
    pub snikt_gallery_id: Option<String>,
    pub snikt_gallery_inherits_collection: bool,
    pub raremarq_gallery_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct GalleryMergeUpdate {
    pub collection_id: i64,
    pub source_gallery_id: i64,
    pub target_gallery_id: i64,
    pub name: String,
    pub caf_gallery_room_id: Option<String>,
    pub raremarq_gallery_id: Option<String>,
    pub snikt_gallery_inherits_collection: bool,
}

#[derive(Debug, Clone)]
pub struct ArtworkMergeUpdate {
    pub collection_id: i64,
    pub source_gallery_id: i64,
    pub source_artwork_id: i64,
    pub target_artwork_id: i64,
    pub metadata: MetadataUpdate,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceState {
    pub mode: String,
    pub collection: Option<CollectionSummary>,
    pub galleries: Vec<GallerySummary>,
    pub selected_gallery_id: Option<i64>,
    pub artworks: Vec<ArtworkSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceLoadProgress {
    pub phase: String,
    pub message: String,
    pub artworks_total: usize,
    pub artworks_loaded: usize,
    pub current_artwork: Option<String>,
    pub done: bool,
}

#[derive(Debug, Clone, Default, Serialize)]
pub(crate) struct CollectionOpenDebugProfile {
    pub(crate) collection_path: String,
    pub(crate) collection_name: Option<String>,
    pub(crate) total_ms: u128,
    pub(crate) collection_manifest_read_ms: u128,
    pub(crate) reset_ms: u128,
    pub(crate) collection_upsert_ms: u128,
    pub(crate) galleries_total: usize,
    pub(crate) gallery_manifest_read_ms: u128,
    pub(crate) gallery_upsert_and_link_ms: u128,
    pub(crate) artworks_total: usize,
    pub(crate) artwork_manifest_read_ms: u128,
    pub(crate) artwork_row_upsert_ms: u128,
    pub(crate) artwork_membership_ms: u128,
    pub(crate) artwork_payload_ms: u128,
    pub(crate) files_seen: usize,
    pub(crate) files_imported: usize,
}

pub(crate) struct CollectionOpenProfiler {
    pub(crate) enabled: bool,
    pub(crate) output_path: Option<PathBuf>,
    pub(crate) profile: CollectionOpenDebugProfile,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchMatchMode {
    Contains,
    Prefix,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceSearchTerm {
    pub value: String,
    pub mode: SearchMatchMode,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileAsset {
    pub id: i64,
    pub artwork_id: i64,
    pub original_path: PathBuf,
    pub current_path: PathBuf,
    pub relative_path: String,
    pub file_name: String,
    pub extension: String,
    pub size_bytes: i64,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub dpi_x: Option<f64>,
    pub dpi_y: Option<f64>,
    pub image_role: Option<String>,
    pub source_kind: String,
    pub is_primary: bool,
}

#[derive(Debug, Clone)]
pub struct FileAssetMetadata {
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub dpi_x: Option<f64>,
    pub dpi_y: Option<f64>,
}

#[derive(Debug, Clone)]
pub struct FileAssetKnownMetadataInsert<'a> {
    pub original_path: &'a Path,
    pub root: &'a Path,
    pub path: &'a Path,
    pub is_primary: bool,
    pub source_kind: &'a str,
    pub metadata: FileAssetMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalLinkRecord {
    pub provider: String,
    pub external_id: Option<String>,
    pub url: String,
    pub extensions: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileExternalLinkRecord {
    pub provider: String,
    pub external_id: String,
    pub url: String,
    pub extensions: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DerivedAsset {
    pub id: i64,
    pub artwork_id: i64,
    pub source_file_asset_id: Option<i64>,
    pub derivative_type: String,
    pub format: String,
    pub path: PathBuf,
    pub width: i64,
    pub height: i64,
    pub image_role: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AssetKind {
    File,
    Derived,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DeletePreview {
    pub files_to_trash: Vec<DeleteFilePreview>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeleteFilePreview {
    pub path: PathBuf,
    pub label: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DeleteResult {
    pub trashed_files: Vec<DeleteFilePreview>,
    pub trash_failures: Vec<DeleteTrashFailure>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeleteTrashFailure {
    pub path: PathBuf,
    pub error: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteArtworkFileResult {
    pub detail: ArtworkDetail,
    pub result: DeleteResult,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileRenamePlan {
    pub asset_kind: AssetKind,
    pub asset_id: i64,
    pub artwork_id: i64,
    pub current_path: PathBuf,
    pub new_path: PathBuf,
    pub new_file_name: String,
    pub physical_file_rename: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileRenameExecution {
    pub plan: FileRenamePlan,
    #[serde(default)]
    pub confirmed_physical_file_rename: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileRenameResult {
    pub detail: ArtworkDetail,
    pub plan: FileRenamePlan,
    pub renamed: bool,
    pub rolled_back: bool,
}

pub struct DerivedAssetInsert<'a> {
    pub source_file_asset_id: Option<i64>,
    pub derivative_type: &'a str,
    pub format: &'a str,
    pub path: &'a Path,
    pub width: i64,
    pub height: i64,
    pub image_role: Option<&'a str>,
}

#[derive(Debug, Clone)]
pub(crate) struct DerivedAssetCacheRow {
    pub artwork_id: i64,
    pub source_file_asset_id: i64,
    pub derivative_type: String,
    pub format: String,
    pub path: PathBuf,
    pub width: i64,
    pub height: i64,
}

#[derive(Debug, Clone)]
pub struct DerivedAssetRenderInsert {
    pub derived_asset_id: i64,
    pub purpose: String,
    pub recipe_key: String,
    pub recipe_json: String,
    pub source_path: PathBuf,
    pub source_size_bytes: u64,
    pub source_modified_at: Option<String>,
    pub source_width: i64,
    pub source_height: i64,
    pub output_width: i64,
    pub output_height: i64,
    pub output_size_bytes: u64,
    pub renderer: String,
    pub renderer_version: String,
    pub renderer_options_json: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DerivedAssetRender {
    pub derived_asset_id: i64,
    pub purpose: String,
    pub recipe_key: String,
    pub recipe_json: String,
    pub source_path: PathBuf,
    pub source_size_bytes: i64,
    pub source_modified_at: Option<String>,
    pub source_width: i64,
    pub source_height: i64,
    pub output_width: i64,
    pub output_height: i64,
    pub output_size_bytes: i64,
    pub renderer: String,
    pub renderer_version: String,
    pub renderer_options_json: String,
    pub created_at: String,
}

#[derive(Debug, Clone)]
pub struct FileAssetImageProbeInsert {
    pub file_asset_id: i64,
    pub probe_status: String,
    pub render_status: String,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub dpi_x: Option<f64>,
    pub dpi_y: Option<f64>,
    pub container_format: Option<String>,
    pub detected_mime: Option<String>,
    pub compression: Option<String>,
    pub photometric: Option<String>,
    pub bits_per_sample: Option<i64>,
    pub samples_per_pixel: Option<i64>,
    pub has_alpha: Option<bool>,
    pub preferred_renderer: Option<String>,
    pub renderer_version: Option<String>,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtistCredit {
    pub name: String,
    pub role: Option<String>,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub role_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtistCreditUpdate {
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub role_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtworkDetail {
    pub id: i64,
    pub canonical_id: String,
    pub display_id: String,
    pub caf_artwork_id: Option<String>,
    pub snikt_artwork_id: Option<String>,
    pub raremarq_artwork_id: Option<String>,
    pub title: String,
    pub description: Option<String>,
    pub for_sale_status: Option<String>,
    pub media_type_id: Option<String>,
    pub media: Option<String>,
    pub art_type_id: Option<String>,
    pub format: Option<String>,
    pub publication_status_id: Option<String>,
    pub active: bool,
    pub illustration_exchange: bool,
    pub ix_for_sale: bool,
    pub caf_url: Option<String>,
    pub snikt_url: Option<String>,
    pub raremarq_url: Option<String>,
    pub generic_url: Option<String>,
    pub caf_csv_image_link: Option<String>,
    pub caf_csv_added_to_caf: Option<String>,
    pub snikt_csv_created_date: Option<String>,
    pub snikt_metadata: SniktMetadata,
    pub purchase_price: Option<String>,
    pub estimated_value: Option<String>,
    pub purchase_date: Option<String>,
    pub provenance: Option<String>,
    pub personal_notes: Option<String>,
    pub source_folder: PathBuf,
    pub artist_credits: Vec<ArtistCredit>,
    pub file_assets: Vec<FileAsset>,
    pub derived_assets: Vec<DerivedAsset>,
    #[serde(default)]
    pub cache_warnings: Vec<ArtworkCacheWarning>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ArtworkCacheWarning {
    pub file_asset_id: i64,
    pub path: PathBuf,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SniktMetadata {
    pub art_type: Option<String>,
    pub comic_publisher: Option<String>,
    pub series_title: Option<String>,
    pub issue_number: Option<String>,
    pub series_page_number: Option<String>,
    pub year: Option<String>,
    pub character: Option<String>,
    pub subcategory: Option<String>,
    pub animation_studio: Option<String>,
    pub episode_number: Option<String>,
    pub episode_title: Option<String>,
    pub published_date: Option<String>,
    pub strip_title: Option<String>,
    pub is_sunday_strip: bool,
    pub other: Option<String>,
    pub tags: Option<String>,
    pub is_nsfw: bool,
    pub is_for_sale: bool,
    pub price: Option<String>,
    pub is_open_to_offers: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetadataUpdate {
    pub artwork_id: i64,
    pub title: String,
    pub description: Option<String>,
    pub for_sale_status: Option<String>,
    pub media_type_id: Option<String>,
    pub art_type_id: Option<String>,
    pub publication_status_id: Option<String>,
    pub active: bool,
    pub illustration_exchange: bool,
    pub ix_for_sale: bool,
    pub artist_credits: Vec<ArtistCreditUpdate>,
    pub media: Option<String>,
    pub format: Option<String>,
    pub caf_url: Option<String>,
    pub snikt_url: Option<String>,
    pub raremarq_url: Option<String>,
    pub generic_url: Option<String>,
    pub snikt_metadata: Option<SniktMetadataUpdate>,
    pub purchase_price: Option<String>,
    pub estimated_value: Option<String>,
    pub purchase_date: Option<String>,
    pub provenance: Option<String>,
    pub personal_notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct SniktMetadataUpdate {
    pub art_type: Option<String>,
    pub comic_publisher: Option<String>,
    pub series_title: Option<String>,
    pub issue_number: Option<String>,
    pub series_page_number: Option<String>,
    pub year: Option<String>,
    pub character: Option<String>,
    pub subcategory: Option<String>,
    pub animation_studio: Option<String>,
    pub episode_number: Option<String>,
    pub episode_title: Option<String>,
    pub published_date: Option<String>,
    pub strip_title: Option<String>,
    pub is_sunday_strip: bool,
    pub other: Option<String>,
    pub tags: Option<String>,
    pub is_nsfw: bool,
    pub is_for_sale: bool,
    pub price: Option<String>,
    pub is_open_to_offers: bool,
}

#[derive(Debug, Clone)]
pub struct ImportedCafArtwork {
    pub piece_id: String,
    pub title: String,
    pub description: Option<String>,
    pub for_sale_status: Option<String>,
    pub media_type_id: Option<String>,
    pub art_type_id: Option<String>,
    pub artist_credits: Vec<ArtistCreditUpdate>,
    pub caf_url: String,
    pub primary_image_url: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ImportedCafImageArtwork {
    pub source_image_url: String,
    pub source_thumbnail_url: Option<String>,
    pub added_to_caf: Option<String>,
    pub title: String,
    pub description: Option<String>,
    pub for_sale_status: Option<String>,
    pub media_type_id: Option<String>,
    pub art_type_id: Option<String>,
    pub artist_credits: Vec<ArtistCreditUpdate>,
    pub purchase_price: Option<String>,
    pub estimated_value: Option<String>,
    pub purchase_date: Option<String>,
    pub provenance: Option<String>,
    pub personal_notes: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ImportedSniktArtwork {
    pub snikt_id: String,
    pub title: String,
    pub description: Option<String>,
    pub artist_credits: Vec<ArtistCreditUpdate>,
    pub snikt_url: String,
    pub snikt_metadata: Option<SniktMetadataUpdate>,
}

#[derive(Debug, Clone)]
pub struct ImportedSniktCsvArtwork {
    pub title: String,
    pub created_date: Option<String>,
    pub description: Option<String>,
    pub active: bool,
    pub artist_credits: Vec<ArtistCreditUpdate>,
    pub snikt_metadata: Option<SniktMetadataUpdate>,
    pub estimated_value: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppPreferences {
    pub default_attach_mode: String,
    pub default_png_export_variant: String,
    pub default_provider_focus: String,
    pub artwork_id_label_preference: String,
    pub theme: String,
    pub startup_behavior: String,
    pub default_workspace_root: String,
    pub raremarq_csv_export_scope: String,
    pub raremarq_csv_url_mode: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtworkIdLabelPreference {
    Oac,
    PreferCaf,
    PreferSnikt,
    PreferRaremarq,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationLog {
    pub id: i64,
    pub artwork_id: i64,
    pub file_asset_id: Option<i64>,
    pub old_path: PathBuf,
    pub new_path: PathBuf,
    pub result: String,
    pub message: Option<String>,
    pub created_at: String,
}
