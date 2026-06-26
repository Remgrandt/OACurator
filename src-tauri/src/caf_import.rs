use crate::catalog::{
    art_type_id_for_label, artist_role_id_for_label, default_gallery_manifest_path,
    media_type_id_for_label, ArtistCreditUpdate, ArtworkSummary, Catalog, CollectionSummary,
    ImportedCafImageArtwork,
};
use crate::csv_safety::spreadsheet_safe_cell;
use crate::path_safety::{safe_path_component, unique_child_folder};
use crate::{AppError, Result};
use chrono::{NaiveDate, NaiveDateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use url::Url;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CafImportReport {
    pub caf_collection_id: String,
    pub collection: CollectionSummary,
    pub galleries_imported: usize,
    pub artworks_imported: usize,
    pub images_downloaded: usize,
    pub image_download_failures: usize,
    pub skipped_artworks: usize,
    pub missing_artworks: Vec<CafMissingArtworkReportRow>,
    pub reconciliation_items: Vec<CafImportReconciliationItem>,
    pub debug_log_path: Option<PathBuf>,
    pub messages: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CafImportReconciliationItem {
    pub gallery_id: i64,
    pub gallery_name: String,
    pub row: CafImportReconciliationRow,
    pub candidates: Vec<CafImportReconciliationCandidate>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CafImportReconciliationRow {
    pub csv_row_number: usize,
    pub gcat: String,
    pub gsub: String,
    pub image_link: String,
    pub full_image_url: String,
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
    pub personal_notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CafImportReconciliationCandidate {
    pub artwork_id: i64,
    pub display_id: String,
    pub title: String,
    pub thumbnail_path: Option<PathBuf>,
}

impl CafImportReconciliationRow {
    fn into_imported_artwork(self) -> ImportedCafImageArtwork {
        ImportedCafImageArtwork {
            source_image_url: self.full_image_url,
            source_thumbnail_url: Some(self.image_link),
            added_to_caf: self.added_to_caf,
            title: self.title,
            description: self.description,
            for_sale_status: self.for_sale_status,
            media_type_id: self.media_type_id,
            art_type_id: self.art_type_id,
            artist_credits: self.artist_credits,
            purchase_price: self.purchase_price,
            estimated_value: self.estimated_value,
            purchase_date: self.purchase_date,
            provenance: None,
            personal_notes: self.personal_notes,
        }
    }
}

impl From<ArtworkSummary> for CafImportReconciliationCandidate {
    fn from(summary: ArtworkSummary) -> Self {
        Self {
            artwork_id: summary.id,
            display_id: summary.display_id,
            title: summary.title,
            thumbnail_path: summary.thumbnail_path,
        }
    }
}

impl CafImportReconciliationItem {
    fn row_number_for_debug(&self) -> usize {
        self.row.csv_row_number
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CafMissingArtworkReportRow {
    pub image_link: String,
    pub title: String,
    pub artists: String,
    pub media_type: String,
    pub art_type: String,
    pub for_sale: String,
    pub added_to_caf: String,
    pub description: String,
    pub purchase_date: String,
    pub purchase_price: String,
    pub estimated_value: String,
    pub personal_notes: String,
}

pub fn write_caf_missing_artwork_report(
    path: &Path,
    rows: &[CafMissingArtworkReportRow],
    include_private_metadata: bool,
) -> Result<usize> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut writer = csv::Writer::from_path(path).map_err(|error| {
        AppError::Message(format!("Could not create CAF missing report: {error}"))
    })?;
    let mut headers = vec![
        "image_link",
        "title",
        "artists",
        "media_type",
        "art_type",
        "for_sale",
        "added_to_caf",
        "description",
    ];
    if include_private_metadata {
        headers.extend([
            "purchase_date",
            "purchase_price",
            "estimated_value",
            "personal_notes",
        ]);
    }
    writer.write_record(headers)?;
    for row in rows {
        let mut record = vec![
            row.image_link.as_str(),
            row.title.as_str(),
            row.artists.as_str(),
            row.media_type.as_str(),
            row.art_type.as_str(),
            row.for_sale.as_str(),
            row.added_to_caf.as_str(),
            row.description.as_str(),
        ];
        if include_private_metadata {
            record.extend([
                row.purchase_date.as_str(),
                row.purchase_price.as_str(),
                row.estimated_value.as_str(),
                row.personal_notes.as_str(),
            ]);
        }
        let safe_record = record
            .into_iter()
            .map(spreadsheet_safe_cell)
            .collect::<Vec<_>>();
        writer.write_record(safe_record)?;
    }
    writer.flush()?;
    Ok(rows.len())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CafImportProgress {
    pub phase: String,
    pub message: String,
    pub galleries_total: usize,
    pub galleries_imported: usize,
    pub artworks_total: usize,
    pub artworks_imported: usize,
    pub images_downloaded: usize,
    pub image_download_failures: usize,
    pub skipped_artworks: usize,
    pub current_gallery: Option<String>,
    pub current_artwork: Option<String>,
    pub done: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CafImportDebugProfile {
    pub caf_collection_id: String,
    pub galleries_total: usize,
    pub artworks_total: usize,
    pub galleries_imported: usize,
    pub artworks_imported: usize,
    pub images_downloaded: usize,
    pub image_download_failures: usize,
    pub skipped_artworks: usize,
    pub total_ms: u128,
    pub collection_fetch: ProfileStage,
    pub collection_parse: ProfileStage,
    pub gallery_fetch: ProfileStage,
    pub gallery_parse: ProfileStage,
    pub artwork_fetch: ProfileStage,
    pub artwork_parse: ProfileStage,
    pub artwork_catalog_write: ProfileStage,
    pub image_fetch: ProfileStage,
    pub image_duplicate_check: ProfileStage,
    pub image_write: ProfileStage,
    pub image_dimension: ProfileStage,
    pub image_index_total: ProfileStage,
    pub file_asset_upsert: ProfileStage,
    pub cache_worker_count: usize,
    pub cache_worker_wall: ProfileStage,
    pub cache_total: ProfileStage,
    pub cache_open: ProfileStage,
    pub thumbnail_resize: ProfileStage,
    pub thumbnail_write: ProfileStage,
    pub thumbnail_db: ProfileStage,
    pub preview_resize: ProfileStage,
    pub preview_write: ProfileStage,
    pub preview_db: ProfileStage,
    pub image_role_update: ProfileStage,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProfileStage {
    pub count: usize,
    pub total_ms: u128,
    pub bytes: u64,
}

struct ImportProfiler {
    enabled: bool,
    output_path: Option<PathBuf>,
    profile: CafImportDebugProfile,
}

impl ImportProfiler {
    fn from_env(caf_collection_id: &str) -> Self {
        let enabled = cfg!(debug_assertions) && env_flag("OACURATOR_DEBUG_PROFILE_CAF_IMPORT");
        Self {
            enabled,
            output_path: env::var_os("OACURATOR_DEBUG_PROFILE_PATH").map(PathBuf::from),
            profile: CafImportDebugProfile {
                caf_collection_id: caf_collection_id.to_string(),
                ..CafImportDebugProfile::default()
            },
        }
    }

    fn finish(
        mut self,
        report: &CafImportReport,
        galleries_total: usize,
        artworks_total: usize,
        total_elapsed: Duration,
    ) {
        if !self.enabled {
            return;
        }
        self.profile.total_ms = total_elapsed.as_millis();
        self.profile.galleries_total = galleries_total;
        self.profile.artworks_total = artworks_total;
        self.profile.galleries_imported = report.galleries_imported;
        self.profile.artworks_imported = report.artworks_imported;
        self.profile.images_downloaded = report.images_downloaded;
        self.profile.image_download_failures = report.image_download_failures;
        self.profile.skipped_artworks = report.skipped_artworks;

        let Ok(contents) = serde_json::to_string_pretty(&self.profile) else {
            return;
        };
        if let Some(path) = self.output_path {
            if let Some(parent) = path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            let _ = fs::write(path, format!("{contents}\n"));
        } else {
            eprintln!("{contents}");
        }
    }
}

struct CafImportDebugLog {
    path: PathBuf,
    lines: Vec<String>,
}

impl CafImportDebugLog {
    fn new(cache_dir: &Path, csv_path: &Path) -> Self {
        let timestamp = Utc::now().format("%Y%m%d-%H%M%S%.3f");
        let csv_stem = csv_path
            .file_stem()
            .and_then(|value| value.to_str())
            .map(|value| safe_path_component(value, "caf-csv"))
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "caf-csv".to_string());
        let path = cache_dir
            .join("import-logs")
            .join(format!("{timestamp}-{csv_stem}.log"));
        let mut log = Self {
            path,
            lines: Vec::new(),
        };
        log.line(format!(
            "CAF CSV import started at {}",
            Utc::now().to_rfc3339()
        ));
        log.line(format!("CSV path: {}", csv_path.display()));
        log
    }

    fn line(&mut self, message: impl Into<String>) {
        self.lines.push(message.into());
    }

    fn finish(mut self) -> Result<PathBuf> {
        self.line(format!(
            "CAF CSV import finished at {}",
            Utc::now().to_rfc3339()
        ));
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut file = fs::File::create(&self.path)?;
        for line in &self.lines {
            writeln!(file, "{line}")?;
        }
        Ok(self.path)
    }
}

fn env_flag(name: &str) -> bool {
    env::var(name)
        .map(|value| {
            matches!(
                value.to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}

enum CafCollectionImportTarget<'a> {
    NewCollectionAtRoot(&'a Path),
    ExistingCollection {
        collection_id: i64,
        allow_caf_collection_id_override: bool,
    },
}

pub fn import_caf_csv_public_with_progress<F>(
    catalog: &Catalog,
    csv_path: &Path,
    destination_root: &Path,
    cache_dir: &Path,
    progress: F,
) -> Result<CafImportReport>
where
    F: FnMut(CafImportProgress),
{
    import_caf_csv_with_target_and_progress(
        catalog,
        csv_path,
        CafCollectionImportTarget::NewCollectionAtRoot(destination_root),
        cache_dir,
        progress,
    )
}

pub fn import_caf_csv_into_collection_public_with_progress<F>(
    catalog: &Catalog,
    csv_path: &Path,
    target_collection_id: i64,
    cache_dir: &Path,
    allow_caf_collection_id_override: bool,
    progress: F,
) -> Result<CafImportReport>
where
    F: FnMut(CafImportProgress),
{
    import_caf_csv_into_collection_with_progress(
        catalog,
        csv_path,
        target_collection_id,
        cache_dir,
        allow_caf_collection_id_override,
        progress,
    )
}

pub fn import_caf_csv(
    catalog: &Catalog,
    csv_path: &Path,
    destination_root: &Path,
    cache_dir: &Path,
) -> Result<CafImportReport> {
    import_caf_csv_with_progress(catalog, csv_path, destination_root, cache_dir, |_| {})
}

pub fn import_caf_csv_into_collection(
    catalog: &Catalog,
    csv_path: &Path,
    target_collection_id: i64,
    cache_dir: &Path,
) -> Result<CafImportReport> {
    import_caf_csv_into_collection_with_progress(
        catalog,
        csv_path,
        target_collection_id,
        cache_dir,
        false,
        |_| {},
    )
}

pub fn import_caf_csv_with_progress<F>(
    catalog: &Catalog,
    csv_path: &Path,
    destination_root: &Path,
    cache_dir: &Path,
    progress: F,
) -> Result<CafImportReport>
where
    F: FnMut(CafImportProgress),
{
    import_caf_csv_with_target_and_progress(
        catalog,
        csv_path,
        CafCollectionImportTarget::NewCollectionAtRoot(destination_root),
        cache_dir,
        progress,
    )
}

pub fn import_caf_csv_into_collection_with_progress<F>(
    catalog: &Catalog,
    csv_path: &Path,
    target_collection_id: i64,
    cache_dir: &Path,
    allow_caf_collection_id_override: bool,
    progress: F,
) -> Result<CafImportReport>
where
    F: FnMut(CafImportProgress),
{
    import_caf_csv_with_target_and_progress(
        catalog,
        csv_path,
        CafCollectionImportTarget::ExistingCollection {
            collection_id: target_collection_id,
            allow_caf_collection_id_override,
        },
        cache_dir,
        progress,
    )
}

fn caf_import_complete_message(report: &CafImportReport) -> String {
    format!(
        "Imported CAF Collection {}: {} galleries, {} artworks",
        report.caf_collection_id, report.galleries_imported, report.artworks_imported
    )
}

fn import_caf_csv_with_target_and_progress<F>(
    catalog: &Catalog,
    csv_path: &Path,
    target: CafCollectionImportTarget<'_>,
    cache_dir: &Path,
    mut progress: F,
) -> Result<CafImportReport>
where
    F: FnMut(CafImportProgress),
{
    let mut debug_log = CafImportDebugLog::new(cache_dir, csv_path);
    emit_import_progress(
        &mut progress,
        ImportProgressState {
            phase: "csv",
            message: format!("Reading CAF CSV {}", csv_path.display()),
            ..ImportProgressState::default()
        },
    );
    let document = match parse_caf_csv(csv_path) {
        Ok(document) => {
            debug_log.line(format!(
                "Parsed CAF CSV: gcat={}, importable_rows={}, skipped_rows={}, parse_messages={}",
                document.gcat,
                document.rows.len(),
                document.skipped_rows,
                document.messages.len()
            ));
            for message in &document.messages {
                debug_log.line(format!("CSV parse message: {message}"));
            }
            document
        }
        Err(error) => {
            debug_log.line(format!("Fatal parse error: {error}"));
            let log_path = debug_log.finish().ok();
            let suffix = log_path
                .map(|path| format!(" CAF CSV import log: {}", path.display()))
                .unwrap_or_default();
            return Err(AppError::Message(format!("{error}{suffix}")));
        }
    };
    if let CafCollectionImportTarget::ExistingCollection {
        collection_id,
        allow_caf_collection_id_override,
    } = &target
    {
        if *allow_caf_collection_id_override {
            catalog.set_collection_caf_collection_id(*collection_id, &document.gcat)?;
        } else {
            catalog.ensure_collection_caf_collection_id(*collection_id, &document.gcat)?;
        }
    }
    let total_started = Instant::now();
    let profiler = ImportProfiler::from_env(&document.gcat);
    let rows_total = document.rows.len();
    let import_rows_as_new = matches!(&target, CafCollectionImportTarget::NewCollectionAtRoot(_));
    let (collection, collection_folder, using_generic_collection_name) = match target {
        CafCollectionImportTarget::NewCollectionAtRoot(destination_root) => {
            let collection_name = format!("CAF Collection {}", document.gcat);
            let collection_folder =
                unique_child_folder(destination_root, &collection_name, "CAF Collection")?;
            let collection = catalog.create_collection_with_caf_collection_id(
                &collection_name,
                &collection_folder,
                Some(&document.gcat),
            )?;
            let collection =
                catalog.ensure_collection_caf_collection_id(collection.id, &document.gcat)?;
            (collection, collection_folder, true)
        }
        CafCollectionImportTarget::ExistingCollection {
            collection_id,
            allow_caf_collection_id_override,
        } => {
            let collection = if allow_caf_collection_id_override {
                catalog.set_collection_caf_collection_id(collection_id, &document.gcat)?
            } else {
                catalog.ensure_collection_caf_collection_id(collection_id, &document.gcat)?
            };
            let collection_folder = collection
                .manifest_path
                .parent()
                .ok_or_else(|| {
                    AppError::Message(format!(
                        "Collection manifest has no parent folder: {}",
                        collection.manifest_path.display()
                    ))
                })?
                .to_path_buf();
            (collection, collection_folder, false)
        }
    };

    let mut messages = document.messages;
    if using_generic_collection_name {
        messages.push(format!(
            "CAF CSV does not include a Collection name; created \"CAF Collection {}\".",
            document.gcat
        ));
    }

    let mut rows_by_gallery: BTreeMap<String, Vec<CafCsvArtworkRow>> = BTreeMap::new();
    let mut imported_image_links = BTreeSet::new();
    for row in document.rows {
        imported_image_links.insert(row.image_link.clone());
        rows_by_gallery
            .entry(row.gsub.clone())
            .or_default()
            .push(row);
    }
    let galleries_total = rows_by_gallery.len();
    let mut galleries_imported = 0usize;
    let mut artworks_imported = 0usize;
    let images_downloaded = 0usize;
    let image_download_failures = 0usize;
    let mut skipped_artworks = document.skipped_rows;
    let mut valid_artwork_rows_processed = 0usize;
    let mut used_generic_gallery_names = false;
    let mut reconciliation_items = Vec::new();

    let batch_transaction = catalog.begin_batch_transaction()?;
    let mut canonical_ids = catalog.canonical_id_allocator()?;
    for (gsub, rows) in rows_by_gallery {
        debug_log.line(format!(
            "Processing Gallery Room GSub {gsub} with {} importable rows",
            rows.len()
        ));
        let fallback_gallery_name = format!("CAF Gallery Room {gsub}");
        let gallery_folder = collection_folder
            .join("galleries")
            .join(safe_path_component(&fallback_gallery_name, "CAF Import"));
        let gallery_manifest_path = default_gallery_manifest_path(&gallery_folder);
        let gallery = if let Some(gallery) = catalog.linked_caf_gallery_for_collection(
            collection.id,
            &gsub,
            &gallery_manifest_path,
        )? {
            gallery
        } else {
            used_generic_gallery_names = true;
            let gallery = if gallery_manifest_path.exists() {
                let gallery = catalog.open_gallery_without_activating(&gallery_manifest_path)?;
                catalog.mark_gallery_as_caf_gallery(gallery.id, &gsub)?
            } else {
                catalog.create_gallery_with_caf_gallery_room_id(
                    &fallback_gallery_name,
                    &gallery_folder,
                    Some(&gsub),
                )?
            };
            catalog.link_gallery_to_collection(collection.id, gallery.id)?;
            gallery
        };
        debug_log.line(format!(
            "Resolved GSub {gsub} to local gallery id={} name=\"{}\"",
            gallery.id, gallery.name
        ));
        galleries_imported += 1;

        let mut gallery_manifest_dirty = false;
        for row in rows {
            valid_artwork_rows_processed += 1;
            debug_log.line(format!(
                "CSV row {}: importing title=\"{}\" image_link=\"{}\" added_to_caf={:?}",
                row.row_number, row.title, row.image_link, row.added_to_caf
            ));
            emit_import_progress(
                &mut progress,
                ImportProgressState {
                    phase: "csv_artworks",
                    message: format!(
                        "Importing CAF CSV artwork {} of {rows_total}: {}",
                        valid_artwork_rows_processed, row.title
                    ),
                    galleries_total,
                    galleries_imported,
                    artworks_total: rows_total,
                    artworks_imported,
                    images_downloaded,
                    image_download_failures,
                    skipped_artworks,
                    current_gallery: Some(gallery.name.clone()),
                    current_artwork: Some(row.title.clone()),
                    done: false,
                },
            );
            let import_result = if import_rows_as_new {
                import_caf_csv_artwork_row_as_new(
                    catalog,
                    gallery.id,
                    row.clone(),
                    &mut canonical_ids,
                )
            } else {
                import_caf_csv_artwork_row(catalog, gallery.id, row.clone(), &mut canonical_ids)
            };
            match import_result {
                Ok(()) => {
                    debug_log.line(format!(
                        "CSV row {}: imported or refreshed without reconciliation",
                        row.row_number
                    ));
                    gallery_manifest_dirty = true;
                    artworks_imported += 1;
                }
                Err(error) => {
                    skipped_artworks += 1;
                    debug_log.line(format!("CSV row {}: skipped: {error}", row.row_number));
                    if error.to_string().contains("reconciliation is required") {
                        if let Some(item) = caf_reconciliation_item_for_row(
                            catalog,
                            gallery.id,
                            &gallery.name,
                            row,
                        )? {
                            debug_log.line(format!(
                                "CSV row {}: queued reconciliation with {} candidate(s)",
                                item.row_number_for_debug(),
                                item.candidates.len()
                            ));
                            reconciliation_items.push(item);
                        }
                    }
                    messages.push(format!("Skipped CAF CSV row: {error}"));
                }
            }
        }
        if gallery_manifest_dirty {
            catalog.rewrite_gallery_manifest(gallery.id)?;
            catalog.rewrite_collections_for_gallery(gallery.id)?;
        }
    }
    batch_transaction.commit()?;

    if used_generic_gallery_names {
        messages.push(
            "CAF CSV does not include Gallery Room names; new Galleries were named by CAF Gallery Room ID."
                .to_string(),
        );
    }

    let missing_artworks = caf_missing_artwork_rows(catalog, collection.id, &imported_image_links)?;
    debug_log.line(format!(
        "Import result before report: galleries_imported={galleries_imported}, artworks_imported={artworks_imported}, skipped_artworks={skipped_artworks}, reconciliation_items={}, missing_artworks={}",
        reconciliation_items.len(),
        missing_artworks.len()
    ));
    let debug_log_path = match debug_log.finish() {
        Ok(path) => Some(path),
        Err(error) => {
            messages.push(format!("Could not write CAF CSV import log: {error}"));
            None
        }
    };
    if let Some(path) = debug_log_path.as_ref() {
        if skipped_artworks > 0 || !reconciliation_items.is_empty() || !messages.is_empty() {
            messages.push(format!("CAF CSV import log: {}", path.display()));
        }
    }
    let report = CafImportReport {
        caf_collection_id: document.gcat,
        collection,
        galleries_imported,
        artworks_imported,
        images_downloaded,
        image_download_failures,
        skipped_artworks,
        missing_artworks,
        reconciliation_items,
        debug_log_path,
        messages,
    };
    emit_import_progress(
        &mut progress,
        ImportProgressState {
            phase: "complete",
            message: caf_import_complete_message(&report),
            galleries_total,
            galleries_imported: report.galleries_imported,
            artworks_total: rows_total,
            artworks_imported: report.artworks_imported,
            images_downloaded: report.images_downloaded,
            image_download_failures: report.image_download_failures,
            skipped_artworks: report.skipped_artworks,
            current_gallery: None,
            current_artwork: None,
            done: true,
        },
    );
    profiler.finish(
        &report,
        galleries_total,
        rows_total,
        total_started.elapsed(),
    );
    Ok(report)
}

fn import_caf_csv_artwork_row(
    catalog: &Catalog,
    gallery_id: i64,
    row: CafCsvArtworkRow,
    canonical_ids: &mut crate::catalog::CanonicalIdAllocator,
) -> Result<()> {
    catalog.import_caf_image_artwork_in_gallery_deferred_manifest_with_allocator(
        gallery_id,
        row.into_imported_artwork(),
        canonical_ids,
    )?;
    Ok(())
}

fn import_caf_csv_artwork_row_as_new(
    catalog: &Catalog,
    gallery_id: i64,
    row: CafCsvArtworkRow,
    canonical_ids: &mut crate::catalog::CanonicalIdAllocator,
) -> Result<()> {
    catalog.import_caf_image_artwork_as_new_in_gallery_deferred_manifest_with_allocator(
        gallery_id,
        row.into_imported_artwork(),
        canonical_ids,
    )?;
    Ok(())
}

pub fn resolve_caf_csv_reconciliation(
    catalog: &Catalog,
    item: CafImportReconciliationItem,
    target_artwork_id: Option<i64>,
) -> Result<ArtworkSummary> {
    let imported = item.row.into_imported_artwork();
    let summary = if let Some(artwork_id) = target_artwork_id {
        catalog.import_caf_image_artwork_into_existing_deferred_manifest(
            item.gallery_id,
            artwork_id,
            imported,
        )?
    } else {
        catalog.import_caf_image_artwork_as_new_in_gallery_deferred_manifest(
            item.gallery_id,
            imported,
        )?
    };
    catalog.rewrite_gallery_manifest(item.gallery_id)?;
    catalog.rewrite_collections_for_gallery(item.gallery_id)?;
    Ok(summary)
}

pub fn try_auto_resolve_caf_csv_reconciliation(
    catalog: &Catalog,
    item: CafImportReconciliationItem,
) -> Result<Option<ArtworkSummary>> {
    let imported = item.row.into_imported_artwork();
    let Some(artwork_id) =
        catalog.caf_csv_auto_match_artwork_id_in_gallery(item.gallery_id, &imported)?
    else {
        return Ok(None);
    };
    let summary = catalog.import_caf_image_artwork_into_existing_deferred_manifest(
        item.gallery_id,
        artwork_id,
        imported,
    )?;
    catalog.rewrite_gallery_manifest(item.gallery_id)?;
    catalog.rewrite_collections_for_gallery(item.gallery_id)?;
    Ok(Some(summary))
}

fn caf_reconciliation_item_for_row(
    catalog: &Catalog,
    gallery_id: i64,
    gallery_name: &str,
    row: CafCsvArtworkRow,
) -> Result<Option<CafImportReconciliationItem>> {
    let candidates = catalog.artwork_title_candidates_in_gallery(gallery_id, &row.title)?;
    if candidates.is_empty() {
        return Ok(None);
    }
    Ok(Some(CafImportReconciliationItem {
        gallery_id,
        gallery_name: gallery_name.to_string(),
        row: row.into_reconciliation_row(),
        candidates: candidates
            .into_iter()
            .map(CafImportReconciliationCandidate::from)
            .collect(),
    }))
}

fn caf_missing_artwork_rows(
    catalog: &Catalog,
    collection_id: i64,
    imported_image_links: &BTreeSet<String>,
) -> Result<Vec<CafMissingArtworkReportRow>> {
    let mut rows = Vec::new();
    for summary in catalog.artworks_for_collection(collection_id)? {
        let detail = catalog.artwork_detail(summary.id)?;
        let image_link = detail.caf_csv_image_link.clone().unwrap_or_default();
        if !image_link.is_empty() && imported_image_links.contains(&image_link) {
            continue;
        };
        rows.push(CafMissingArtworkReportRow {
            image_link,
            title: detail.title,
            artists: detail
                .artist_credits
                .iter()
                .filter_map(|credit| {
                    let first = credit.first_name.as_deref().unwrap_or_default().trim();
                    let last = credit.last_name.as_deref().unwrap_or_default().trim();
                    let name = format!("{first} {last}").trim().to_string();
                    (!name.is_empty()).then_some(name).or_else(|| {
                        let name = credit.name.trim();
                        (!name.is_empty()).then(|| name.to_string())
                    })
                })
                .collect::<Vec<_>>()
                .join(", "),
            media_type: detail.media.unwrap_or_default(),
            art_type: detail.format.unwrap_or_default(),
            for_sale: detail.for_sale_status.unwrap_or_default(),
            added_to_caf: detail.caf_csv_added_to_caf.unwrap_or_default(),
            description: detail.description.unwrap_or_default(),
            purchase_date: detail.purchase_date.unwrap_or_default(),
            purchase_price: detail.purchase_price.unwrap_or_default(),
            estimated_value: detail.estimated_value.unwrap_or_default(),
            personal_notes: detail.personal_notes.unwrap_or_default(),
        });
    }
    Ok(rows)
}

#[derive(Debug, Clone)]
struct CafCsvDocument {
    gcat: String,
    rows: Vec<CafCsvArtworkRow>,
    skipped_rows: usize,
    messages: Vec<String>,
}

#[derive(Debug, Clone)]
struct CafCsvArtworkRow {
    row_number: usize,
    gcat: String,
    gsub: String,
    image_link: String,
    full_image_url: String,
    added_to_caf: Option<String>,
    title: String,
    description: Option<String>,
    for_sale_status: Option<String>,
    media_type_id: Option<String>,
    art_type_id: Option<String>,
    artist_credits: Vec<ArtistCreditUpdate>,
    purchase_price: Option<String>,
    estimated_value: Option<String>,
    purchase_date: Option<String>,
    personal_notes: Option<String>,
}

impl CafCsvArtworkRow {
    fn into_imported_artwork(self) -> ImportedCafImageArtwork {
        ImportedCafImageArtwork {
            source_image_url: self.full_image_url,
            source_thumbnail_url: Some(self.image_link),
            added_to_caf: self.added_to_caf,
            title: self.title,
            description: self.description,
            for_sale_status: self.for_sale_status,
            media_type_id: self.media_type_id,
            art_type_id: self.art_type_id,
            artist_credits: self.artist_credits,
            purchase_price: self.purchase_price,
            estimated_value: self.estimated_value,
            purchase_date: self.purchase_date,
            provenance: None,
            personal_notes: self.personal_notes,
        }
    }

    fn into_reconciliation_row(self) -> CafImportReconciliationRow {
        CafImportReconciliationRow {
            csv_row_number: self.row_number,
            gcat: self.gcat,
            gsub: self.gsub,
            image_link: self.image_link,
            full_image_url: self.full_image_url,
            added_to_caf: self.added_to_caf,
            title: self.title,
            description: self.description,
            for_sale_status: self.for_sale_status,
            media_type_id: self.media_type_id,
            art_type_id: self.art_type_id,
            artist_credits: self.artist_credits,
            purchase_price: self.purchase_price,
            estimated_value: self.estimated_value,
            purchase_date: self.purchase_date,
            personal_notes: self.personal_notes,
        }
    }
}

fn parse_caf_csv(path: &Path) -> Result<CafCsvDocument> {
    let mut reader = csv::ReaderBuilder::new()
        .flexible(true)
        .trim(csv::Trim::All)
        .from_path(path)
        .map_err(|error| AppError::Message(format!("Could not open CAF CSV: {error}")))?;
    let headers = reader
        .headers()
        .map_err(|error| AppError::Message(format!("Could not read CAF CSV headers: {error}")))?
        .clone();
    let header_indexes = normalized_csv_header_indexes(&headers);
    let mut rows = Vec::new();
    let mut skipped_rows = 0usize;
    let mut messages = Vec::new();
    let mut gcat: Option<String> = None;

    for (index, record) in reader.records().enumerate() {
        let row_number = index + 2;
        let record = record
            .map_err(|error| AppError::Message(format!("Could not read CAF CSV row: {error}")))?;
        if record.len() > headers.len() {
            let extra_count = record.len() - headers.len();
            messages.push(format!(
                "Skipped CSV row {row_number}: has {extra_count} unexpected extra {}; row was ignored to avoid importing shifted data.",
                if extra_count == 1 { "field" } else { "fields" }
            ));
            skipped_rows += 1;
            continue;
        }
        match parse_caf_csv_row(row_number, &record, &header_indexes, &mut messages)? {
            Some(row) => {
                if let Some(existing_gcat) = &gcat {
                    if existing_gcat != &row.gcat {
                        return Err(AppError::Message(format!(
                            "CAF CSV contains multiple Collection IDs: {existing_gcat} and {}",
                            row.gcat
                        )));
                    }
                } else {
                    gcat = Some(row.gcat.clone());
                }
                rows.push(row);
            }
            None => skipped_rows += 1,
        }
    }

    let gcat = gcat.ok_or_else(|| {
        AppError::Message("CAF CSV did not contain any importable image rows".to_string())
    })?;
    Ok(CafCsvDocument {
        gcat,
        rows,
        skipped_rows,
        messages,
    })
}

fn parse_caf_csv_row(
    row_number: usize,
    record: &csv::StringRecord,
    header_indexes: &BTreeMap<String, usize>,
    messages: &mut Vec<String>,
) -> Result<Option<CafCsvArtworkRow>> {
    let Some(thumbnail_url) = csv_optional_value(record, header_indexes, "image_link") else {
        messages.push(format!("Skipped CSV row {row_number}: missing image_link."));
        return Ok(None);
    };
    let full_image_url = match caf_full_image_url_from_thumbnail(&thumbnail_url) {
        Ok(url) => url,
        Err(error) => {
            messages.push(format!("Skipped CSV row {row_number}: {error}"));
            return Ok(None);
        }
    };
    let Some((gcat, gsub)) = caf_image_collection_and_gallery_ids(&full_image_url) else {
        messages.push(format!(
            "Skipped CSV row {row_number}: image URL does not include CAF Category and subcat IDs."
        ));
        return Ok(None);
    };
    let title = csv_optional_value(record, header_indexes, "title")
        .unwrap_or_else(|| file_name_from_url(&full_image_url));
    let artist_credits = csv_optional_value(record, header_indexes, "artists")
        .map(|value| parse_artist_credits(&value))
        .unwrap_or_default();
    let media_type_id = csv_optional_value(record, header_indexes, "media_type")
        .and_then(|value| media_type_id_for_label(&value).map(str::to_string));
    let art_type_id = csv_optional_value(record, header_indexes, "art_type")
        .and_then(|value| art_type_id_for_label(&value).map(str::to_string));
    let purchase_date = csv_optional_value(record, header_indexes, "purchase_date")
        .and_then(|value| normalize_caf_csv_date(row_number, &value, messages));
    let added_to_caf = csv_optional_value(record, header_indexes, "added_to_caf")
        .and_then(|value| normalize_caf_csv_datetime_minute(row_number, &value, messages));

    Ok(Some(CafCsvArtworkRow {
        row_number,
        gcat,
        gsub,
        image_link: thumbnail_url,
        full_image_url,
        added_to_caf,
        title,
        description: csv_optional_value(record, header_indexes, "description"),
        for_sale_status: csv_optional_value(record, header_indexes, "for_sale"),
        media_type_id,
        art_type_id,
        artist_credits,
        purchase_price: csv_money_value(record, header_indexes, "purchase_price"),
        estimated_value: csv_money_value(record, header_indexes, "estimated_value"),
        purchase_date,
        personal_notes: csv_optional_value(record, header_indexes, "personal_notes"),
    }))
}

fn normalized_csv_header_indexes(headers: &csv::StringRecord) -> BTreeMap<String, usize> {
    let mut indexes = BTreeMap::new();
    for (index, header) in headers.iter().enumerate() {
        indexes
            .entry(normalize_caf_csv_header(header))
            .or_insert(index);
    }
    indexes
}

fn normalize_caf_csv_header(header: &str) -> String {
    header.trim().to_ascii_lowercase()
}

fn csv_optional_value(
    record: &csv::StringRecord,
    header_indexes: &BTreeMap<String, usize>,
    name: &str,
) -> Option<String> {
    let index = *header_indexes.get(name)?;
    record
        .get(index)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn csv_money_value(
    record: &csv::StringRecord,
    header_indexes: &BTreeMap<String, usize>,
    name: &str,
) -> Option<String> {
    csv_optional_value(record, header_indexes, name).filter(|value| value.trim() != "0")
}

fn normalize_caf_csv_date(
    row_number: usize,
    value: &str,
    messages: &mut Vec<String>,
) -> Option<String> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    if NaiveDate::parse_from_str(value, "%Y-%m-%d").is_ok() {
        return Some(value.to_string());
    }
    let date_part = value.split_whitespace().next().unwrap_or_default();
    let parts = date_part.split('/').collect::<Vec<_>>();
    if parts.len() == 3 {
        let parsed = parts[0]
            .parse::<u32>()
            .ok()
            .zip(parts[1].parse::<u32>().ok())
            .zip(parts[2].parse::<i32>().ok())
            .and_then(|((month, day), year)| NaiveDate::from_ymd_opt(year, month, day));
        if let Some(date) = parsed {
            return Some(date.format("%Y-%m-%d").to_string());
        }
    }
    messages.push(format!(
        "CSV row {row_number} purchase_date \"{value}\" is not a supported date; leaving it blank."
    ));
    None
}

fn normalize_caf_csv_datetime_minute(
    row_number: usize,
    value: &str,
    messages: &mut Vec<String>,
) -> Option<String> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    for format in ["%Y-%m-%dT%H:%M", "%Y-%m-%d %H:%M"] {
        if let Ok(parsed) = NaiveDateTime::parse_from_str(value, format) {
            return Some(parsed.format("%Y-%m-%dT%H:%M").to_string());
        }
    }
    for format in [
        "%m/%d/%Y %I:%M:%S %p",
        "%m/%d/%Y %I:%M %p",
        "%-m/%-d/%Y %I:%M:%S %p",
        "%-m/%-d/%Y %I:%M %p",
    ] {
        if let Ok(parsed) = NaiveDateTime::parse_from_str(value, format) {
            return Some(parsed.format("%Y-%m-%dT%H:%M").to_string());
        }
    }
    if let Ok(parsed) = NaiveDate::parse_from_str(value, "%Y-%m-%d") {
        return Some(parsed.format("%Y-%m-%d").to_string());
    }
    let parts = value.split('/').collect::<Vec<_>>();
    if parts.len() == 3 {
        let parsed = parts[0]
            .parse::<u32>()
            .ok()
            .zip(parts[1].parse::<u32>().ok())
            .zip(parts[2].parse::<i32>().ok())
            .and_then(|((month, day), year)| NaiveDate::from_ymd_opt(year, month, day));
        if let Some(date) = parsed {
            return Some(date.format("%Y-%m-%d").to_string());
        }
    }
    messages.push(format!(
        "CSV row {row_number} added_to_caf \"{value}\" is not a supported datetime; leaving it blank."
    ));
    None
}

fn caf_full_image_url_from_thumbnail(thumbnail_url: &str) -> Result<String> {
    let mut url = Url::parse(thumbnail_url.trim())
        .map_err(|error| AppError::Message(format!("invalid image URL: {error}")))?;
    let mut segments = url
        .path_segments()
        .ok_or_else(|| AppError::Message("image URL has no path segments".to_string()))?
        .collect::<Vec<_>>();
    if let Some(index) = segments
        .iter()
        .position(|segment| segment.eq_ignore_ascii_case("thumbs"))
    {
        segments.remove(index);
        url.set_path(&segments.join("/"));
    }
    Ok(url.to_string())
}

fn caf_image_collection_and_gallery_ids(image_url: &str) -> Option<(String, String)> {
    let url = Url::parse(image_url).ok()?;
    let mut gcat = None;
    let mut gsub = None;
    for segment in url.path_segments()? {
        if let Some((prefix, id)) = segment.split_once('_') {
            if prefix.eq_ignore_ascii_case("Category") && is_positive_integer_text(id) {
                gcat = Some(id.to_string());
            } else if prefix.eq_ignore_ascii_case("subcat") && is_positive_integer_text(id) {
                gsub = Some(id.to_string());
            }
        }
    }
    gcat.zip(gsub)
}

#[derive(Clone, Default)]
struct ImportProgressState {
    phase: &'static str,
    message: String,
    galleries_total: usize,
    galleries_imported: usize,
    artworks_total: usize,
    artworks_imported: usize,
    images_downloaded: usize,
    image_download_failures: usize,
    skipped_artworks: usize,
    current_gallery: Option<String>,
    current_artwork: Option<String>,
    done: bool,
}

fn emit_import_progress<F>(progress: &mut F, state: ImportProgressState)
where
    F: FnMut(CafImportProgress),
{
    progress(CafImportProgress {
        phase: state.phase.to_string(),
        message: state.message,
        galleries_total: state.galleries_total,
        galleries_imported: state.galleries_imported,
        artworks_total: state.artworks_total,
        artworks_imported: state.artworks_imported,
        images_downloaded: state.images_downloaded,
        image_download_failures: state.image_download_failures,
        skipped_artworks: state.skipped_artworks,
        current_gallery: state.current_gallery,
        current_artwork: state.current_artwork,
        done: state.done,
    });
}

fn file_name_from_url(url: &str) -> String {
    let parsed = Url::parse(url).ok();
    let raw = parsed
        .as_ref()
        .and_then(|url| url.path_segments()?.next_back())
        .filter(|value| !value.is_empty())
        .unwrap_or("caf-image.jpg");
    safe_path_component(raw, "CAF Import")
}

fn is_positive_integer_text(value: &str) -> bool {
    value.chars().all(|character| character.is_ascii_digit())
        && value.parse::<u64>().is_ok_and(|parsed| parsed > 0)
}

fn parse_artist_credits(value: &str) -> Vec<ArtistCreditUpdate> {
    value
        .split([',', ';'])
        .filter_map(|part| {
            let part = part.trim();
            if part.is_empty() {
                return None;
            }
            let (name, role) = match part.rsplit_once('(') {
                Some((name, role)) => (
                    name.trim(),
                    role.trim().trim_end_matches(')').trim().to_string(),
                ),
                None => (part, "Penciller".to_string()),
            };
            if name.is_empty() {
                return None;
            }
            let role_id = artist_role_id_for_label(&role)
                .or_else(|| role.eq_ignore_ascii_case("artist").then_some("1"))
                .unwrap_or("1")
                .to_string();
            let (first_name, last_name) = split_artist_name(name);
            Some(ArtistCreditUpdate {
                first_name,
                last_name,
                role_id: Some(role_id),
            })
        })
        .collect()
}

fn split_artist_name(name: &str) -> (Option<String>, Option<String>) {
    let parts = name.split_whitespace().collect::<Vec<_>>();
    match parts.as_slice() {
        [] => (None, None),
        [single] => (None, Some((*single).to_string())),
        [first @ .., last] => (Some(first.join(" ")), Some((*last).to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn caf_missing_report_omits_private_collector_metadata_by_default() {
        let dir = TempDir::new().unwrap();
        let report_path = dir.path().join("caf-missing.csv");

        write_caf_missing_artwork_report(&report_path, &[private_report_row()], false).unwrap();

        let exported = fs::read_to_string(report_path).unwrap();
        assert!(!exported.contains("purchase_date"));
        assert!(!exported.contains("purchase_price"));
        assert!(!exported.contains("estimated_value"));
        assert!(!exported.contains("personal_notes"));
        assert!(!exported.contains("2025-12-24"));
        assert!(!exported.contains("$100"));
        assert!(!exported.contains("$150"));
        assert!(!exported.contains("Private note"));
    }

    #[test]
    fn caf_missing_report_includes_private_collector_metadata_when_requested() {
        let dir = TempDir::new().unwrap();
        let report_path = dir.path().join("caf-missing.csv");

        write_caf_missing_artwork_report(&report_path, &[private_report_row()], true).unwrap();

        let exported = fs::read_to_string(report_path).unwrap();
        assert!(exported.contains("purchase_date"));
        assert!(exported.contains("purchase_price"));
        assert!(exported.contains("estimated_value"));
        assert!(exported.contains("personal_notes"));
        assert!(exported.contains("2025-12-24"));
        assert!(exported.contains("$100"));
        assert!(exported.contains("$150"));
        assert!(exported.contains("Private note"));
    }

    #[test]
    fn caf_missing_report_neutralizes_spreadsheet_formula_cells() {
        let dir = TempDir::new().unwrap();
        let report_path = dir.path().join("caf-missing.csv");
        let row = CafMissingArtworkReportRow {
            image_link: "=HYPERLINK(\"https://evil.example\",\"image\")".to_string(),
            title: "+Missing CAF Piece".to_string(),
            artists: "-Jane Doe".to_string(),
            media_type: "@Pen and Ink".to_string(),
            art_type: "\tInterior Page".to_string(),
            for_sale: "\rNFS".to_string(),
            added_to_caf: "\n2025-06-06T10:33".to_string(),
            description: "=Missing from CAF CSV".to_string(),
            purchase_date: "+2025-12-24".to_string(),
            purchase_price: "-100".to_string(),
            estimated_value: "@SUM(1,2)".to_string(),
            personal_notes: "\tPrivate note".to_string(),
        };

        write_caf_missing_artwork_report(&report_path, &[row], true).unwrap();

        let mut reader = csv::Reader::from_path(&report_path).unwrap();
        let records = reader
            .records()
            .collect::<std::result::Result<Vec<_>, _>>()
            .unwrap();
        let exported_row = &records[0];
        assert_eq!(
            exported_row.get(0),
            Some("'=HYPERLINK(\"https://evil.example\",\"image\")")
        );
        assert_eq!(exported_row.get(1), Some("'+Missing CAF Piece"));
        assert_eq!(exported_row.get(2), Some("'-Jane Doe"));
        assert_eq!(exported_row.get(3), Some("'@Pen and Ink"));
        assert_eq!(exported_row.get(4), Some("'\tInterior Page"));
        assert_eq!(exported_row.get(5), Some("'\rNFS"));
        assert_eq!(exported_row.get(6), Some("'\n2025-06-06T10:33"));
        assert_eq!(exported_row.get(7), Some("'=Missing from CAF CSV"));
        assert_eq!(exported_row.get(8), Some("'+2025-12-24"));
        assert_eq!(exported_row.get(9), Some("'-100"));
        assert_eq!(exported_row.get(10), Some("'@SUM(1,2)"));
        assert_eq!(exported_row.get(11), Some("'\tPrivate note"));
    }

    fn private_report_row() -> CafMissingArtworkReportRow {
        CafMissingArtworkReportRow {
            image_link: "https://example.com/missing.jpg".to_string(),
            title: "Missing CAF Piece".to_string(),
            artists: "Jane Doe".to_string(),
            media_type: "Pen and Ink".to_string(),
            art_type: "Interior Page".to_string(),
            for_sale: "NFS".to_string(),
            added_to_caf: "2025-06-06T10:33".to_string(),
            description: "Missing from CAF CSV".to_string(),
            purchase_date: "2025-12-24".to_string(),
            purchase_price: "$100".to_string(),
            estimated_value: "$150".to_string(),
            personal_notes: "Private note".to_string(),
        }
    }
}
