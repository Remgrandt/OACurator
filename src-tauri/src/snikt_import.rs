// Copyright (c) 2026 Remgrandt Works. All rights reserved.

use crate::catalog::{
    artist_role_id_for_label, default_collection_manifest_path, default_gallery_manifest_path,
    ArtistCreditUpdate, ArtworkSummary, Catalog, CollectionSummary, ImportedSniktCsvArtwork,
    SniktMetadataUpdate,
};
use crate::path_safety::unique_child_folder;
use crate::{AppError, Result};
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SniktImportReport {
    pub snikt_collection_id: String,
    pub collection: CollectionSummary,
    pub galleries_imported: usize,
    pub artworks_imported: usize,
    pub images_downloaded: usize,
    pub image_download_failures: usize,
    pub reconciliation_items: Vec<SniktImportReconciliationItem>,
    pub messages: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SniktImportReconciliationItem {
    pub gallery_id: i64,
    pub gallery_name: String,
    pub row: SniktImportReconciliationRow,
    pub candidates: Vec<SniktImportReconciliationCandidate>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SniktImportReconciliationRow {
    pub title: String,
    pub created_date: Option<String>,
    pub description: Option<String>,
    pub active: bool,
    pub artist_credits: Vec<ArtistCreditUpdate>,
    pub snikt_metadata: SniktMetadataUpdate,
    pub estimated_value: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SniktImportReconciliationCandidate {
    pub artwork_id: i64,
    pub display_id: String,
    pub title: String,
    pub thumbnail_path: Option<std::path::PathBuf>,
}

impl SniktImportReconciliationRow {
    fn into_imported(self) -> ImportedSniktCsvArtwork {
        ImportedSniktCsvArtwork {
            title: self.title,
            created_date: self.created_date,
            description: self.description,
            active: self.active,
            artist_credits: self.artist_credits,
            snikt_metadata: Some(self.snikt_metadata),
            estimated_value: self.estimated_value,
        }
    }
}

impl From<ArtworkSummary> for SniktImportReconciliationCandidate {
    fn from(summary: ArtworkSummary) -> Self {
        Self {
            artwork_id: summary.id,
            display_id: summary.display_id,
            title: summary.title,
            thumbnail_path: summary.thumbnail_path,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SniktImportProgress {
    pub phase: String,
    pub message: String,
    pub artworks_total: usize,
    pub artworks_imported: usize,
    pub images_downloaded: usize,
    pub image_download_failures: usize,
    pub current_artwork: Option<String>,
    pub done: bool,
}

enum SniktCollectionImportTarget<'a> {
    NewCollectionAtRoot(&'a Path),
    ExistingCollection(i64),
}

pub fn import_snikt_csv_public_with_progress<F>(
    catalog: &Catalog,
    csv_path: &Path,
    destination_root: &Path,
    mut progress: F,
) -> Result<SniktImportReport>
where
    F: FnMut(SniktImportProgress),
{
    import_snikt_csv_with_target_and_progress(
        catalog,
        csv_path,
        SniktCollectionImportTarget::NewCollectionAtRoot(destination_root),
        &mut progress,
    )
}

pub fn import_snikt_csv_into_collection_public_with_progress<F>(
    catalog: &Catalog,
    csv_path: &Path,
    target_collection_id: i64,
    mut progress: F,
) -> Result<SniktImportReport>
where
    F: FnMut(SniktImportProgress),
{
    import_snikt_csv_with_target_and_progress(
        catalog,
        csv_path,
        SniktCollectionImportTarget::ExistingCollection(target_collection_id),
        &mut progress,
    )
}

fn import_snikt_csv_with_target_and_progress<F>(
    catalog: &Catalog,
    csv_path: &Path,
    target: SniktCollectionImportTarget<'_>,
    progress: &mut F,
) -> Result<SniktImportReport>
where
    F: FnMut(SniktImportProgress),
{
    progress(SniktImportProgress {
        phase: "csv".to_string(),
        message: format!("Reading SNIKT.com CSV {}", csv_path.display()),
        artworks_total: 0,
        artworks_imported: 0,
        images_downloaded: 0,
        image_download_failures: 0,
        current_artwork: None,
        done: false,
    });

    let document = parse_snikt_csv(csv_path)?;
    let rows_total = document.rows.len();
    let (collection, collection_folder) = match target {
        SniktCollectionImportTarget::NewCollectionAtRoot(destination_root) => {
            let collection_name = "SNIKT.com CSV Collection";
            let collection_folder = unique_child_folder(
                destination_root,
                collection_name,
                "SNIKT.com CSV Collection",
            )?;
            let collection_manifest_path = default_collection_manifest_path(&collection_folder);
            let collection = catalog.create_collection_with_provider_ids(
                collection_name,
                &collection_manifest_path,
                None,
                None,
                None,
            )?;
            (collection, collection_folder)
        }
        SniktCollectionImportTarget::ExistingCollection(collection_id) => {
            let collection = catalog.collection_summary(collection_id)?;
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
            (collection, collection_folder)
        }
    };

    let gallery_name = "SNIKT.com gallery";
    let gallery_folder = collection_folder
        .join("galleries")
        .join(safe_path_component(gallery_name));
    let gallery_manifest_path = default_gallery_manifest_path(&gallery_folder);
    let gallery = if gallery_manifest_path.exists() {
        let gallery = catalog.open_gallery_without_activating(&gallery_manifest_path)?;
        catalog.link_gallery_to_collection(collection.id, gallery.id)?;
        gallery
    } else {
        let gallery = catalog.create_gallery(gallery_name, &gallery_manifest_path)?;
        catalog.link_gallery_to_collection(collection.id, gallery.id)?;
        gallery
    };

    let mut messages = document.messages;
    let mut artworks_imported = 0usize;
    let mut reconciliation_items = Vec::new();
    let batch_transaction = catalog.begin_batch_transaction()?;
    let mut canonical_ids = catalog.canonical_id_allocator()?;
    for row in document.rows {
        progress(SniktImportProgress {
            phase: "csv_artworks".to_string(),
            message: format!(
                "Importing SNIKT.com CSV artwork {} of {rows_total}: {}",
                artworks_imported + 1,
                row.title
            ),
            artworks_total: rows_total,
            artworks_imported,
            images_downloaded: 0,
            image_download_failures: 0,
            current_artwork: Some(row.title.clone()),
            done: false,
        });
        let candidates = catalog.snikt_csv_artwork_candidates_in_gallery(
            gallery.id,
            &row.title,
            row.created_date.as_deref(),
        )?;
        if !candidates.is_empty() {
            reconciliation_items.push(SniktImportReconciliationItem {
                gallery_id: gallery.id,
                gallery_name: gallery.name.clone(),
                row: row.into_reconciliation_row(),
                candidates: candidates
                    .into_iter()
                    .map(SniktImportReconciliationCandidate::from)
                    .collect(),
            });
            messages.push("SNIKT.com CSV row needs reconciliation before import.".to_string());
            continue;
        }
        match catalog.import_snikt_csv_artwork_as_new_in_gallery_deferred_manifest_with_allocator(
            gallery.id,
            row.into_imported(),
            Some(&mut canonical_ids),
        ) {
            Ok(_) => artworks_imported += 1,
            Err(error) => messages.push(format!("Skipped SNIKT.com CSV row: {error}")),
        }
    }
    catalog.rewrite_gallery_manifest(gallery.id)?;
    catalog.rewrite_collections_for_gallery(gallery.id)?;
    batch_transaction.commit()?;

    progress(SniktImportProgress {
        phase: "complete".to_string(),
        message: format!("Imported SNIKT.com CSV: 1 gallery, {artworks_imported} artworks"),
        artworks_total: rows_total,
        artworks_imported,
        images_downloaded: 0,
        image_download_failures: 0,
        current_artwork: None,
        done: true,
    });

    Ok(SniktImportReport {
        snikt_collection_id: collection.snikt_collection_id.clone().unwrap_or_default(),
        collection,
        galleries_imported: 1,
        artworks_imported,
        images_downloaded: 0,
        image_download_failures: 0,
        reconciliation_items,
        messages,
    })
}

pub fn resolve_snikt_csv_reconciliation(
    catalog: &Catalog,
    item: SniktImportReconciliationItem,
    target_artwork_id: Option<i64>,
) -> Result<ArtworkSummary> {
    let imported = item.row.into_imported();
    let summary = if let Some(artwork_id) = target_artwork_id {
        catalog.import_snikt_csv_artwork_into_existing_in_gallery(
            item.gallery_id,
            artwork_id,
            imported,
        )?
    } else {
        catalog.import_snikt_csv_artwork_as_new_in_gallery(item.gallery_id, imported)?
    };
    catalog.rewrite_gallery_manifest(item.gallery_id)?;
    Ok(summary)
}

#[derive(Debug, Clone)]
struct SniktCsvDocument {
    rows: Vec<SniktCsvArtworkRow>,
    messages: Vec<String>,
}

#[derive(Debug, Clone)]
struct SniktCsvArtworkRow {
    title: String,
    created_date: Option<String>,
    description: Option<String>,
    active: bool,
    artist_credits: Vec<ArtistCreditUpdate>,
    snikt_metadata: SniktMetadataUpdate,
    estimated_value: Option<String>,
}

impl SniktCsvArtworkRow {
    fn into_imported(self) -> ImportedSniktCsvArtwork {
        ImportedSniktCsvArtwork {
            title: self.title,
            created_date: self.created_date,
            description: self.description,
            active: self.active,
            artist_credits: self.artist_credits,
            snikt_metadata: Some(self.snikt_metadata),
            estimated_value: self.estimated_value,
        }
    }

    fn into_reconciliation_row(self) -> SniktImportReconciliationRow {
        SniktImportReconciliationRow {
            title: self.title,
            created_date: self.created_date,
            description: self.description,
            active: self.active,
            artist_credits: self.artist_credits,
            snikt_metadata: self.snikt_metadata,
            estimated_value: self.estimated_value,
        }
    }
}

fn parse_snikt_csv(path: &Path) -> Result<SniktCsvDocument> {
    let mut reader = csv::ReaderBuilder::new()
        .flexible(true)
        .trim(csv::Trim::All)
        .from_path(path)
        .map_err(|error| AppError::Message(format!("Could not open SNIKT.com CSV: {error}")))?;
    let headers = reader
        .headers()
        .map_err(|error| {
            AppError::Message(format!("Could not read SNIKT.com CSV headers: {error}"))
        })?
        .clone();
    let header_indexes = normalized_csv_header_indexes(&headers);
    let mut rows = Vec::new();
    let mut messages = Vec::new();
    for (index, record) in reader.records().enumerate() {
        let row_number = index + 2;
        let record = record.map_err(|error| {
            AppError::Message(format!("Could not read SNIKT.com CSV row: {error}"))
        })?;
        let Some(row) = parse_snikt_csv_row(row_number, &record, &header_indexes, &mut messages)?
        else {
            continue;
        };
        rows.push(row);
    }
    if rows.is_empty() {
        return Err(AppError::Message(
            "SNIKT.com CSV did not contain any importable rows".to_string(),
        ));
    }
    Ok(SniktCsvDocument { rows, messages })
}

fn parse_snikt_csv_row(
    row_number: usize,
    record: &csv::StringRecord,
    header_indexes: &BTreeMap<String, usize>,
    messages: &mut Vec<String>,
) -> Result<Option<SniktCsvArtworkRow>> {
    let Some(title) = csv_optional_value(record, header_indexes, "display name") else {
        messages.push(format!(
            "Skipped SNIKT.com CSV row {row_number}: missing Display Name."
        ));
        return Ok(None);
    };
    let created_date = csv_optional_value(record, header_indexes, "created date")
        .and_then(|value| normalize_snikt_csv_date(row_number, &value, messages));
    let snikt_metadata = SniktMetadataUpdate {
        art_type: csv_optional_value(record, header_indexes, "art type"),
        comic_publisher: csv_optional_value(record, header_indexes, "publisher"),
        series_title: csv_optional_value(record, header_indexes, "series title"),
        issue_number: csv_optional_value(record, header_indexes, "issue number"),
        series_page_number: csv_optional_value(record, header_indexes, "page number"),
        year: csv_optional_value(record, header_indexes, "year"),
        character: csv_optional_value(record, header_indexes, "character"),
        subcategory: csv_optional_value(record, header_indexes, "subcategory"),
        animation_studio: csv_optional_value(record, header_indexes, "animation studio"),
        episode_number: csv_optional_value(record, header_indexes, "episode number"),
        episode_title: csv_optional_value(record, header_indexes, "episode title"),
        published_date: None,
        strip_title: None,
        is_sunday_strip: false,
        other: csv_optional_value(record, header_indexes, "notes"),
        tags: csv_optional_value(record, header_indexes, "tags"),
        is_nsfw: csv_bool(record, header_indexes, "nsfw"),
        is_for_sale: csv_bool(record, header_indexes, "for sale"),
        price: csv_optional_value(record, header_indexes, "price"),
        is_open_to_offers: csv_bool(record, header_indexes, "open to offers"),
    };
    Ok(Some(SniktCsvArtworkRow {
        title,
        created_date,
        description: csv_optional_value(record, header_indexes, "notes"),
        active: csv_bool(record, header_indexes, "public"),
        artist_credits: snikt_artist_credits(record, header_indexes),
        snikt_metadata,
        estimated_value: csv_optional_value(record, header_indexes, "estimated value"),
    }))
}

fn snikt_artist_credits(
    record: &csv::StringRecord,
    header_indexes: &BTreeMap<String, usize>,
) -> Vec<ArtistCreditUpdate> {
    let mut credits = Vec::new();
    push_artist_credit(record, header_indexes, "pencils", "Penciller", &mut credits);
    push_artist_credit(record, header_indexes, "inker", "Inker", &mut credits);
    push_artist_credit(record, header_indexes, "letterer", "Letterer", &mut credits);
    if credits.is_empty() {
        push_artist_credit(record, header_indexes, "artist", "Penciller", &mut credits);
    }
    credits
}

fn push_artist_credit(
    record: &csv::StringRecord,
    header_indexes: &BTreeMap<String, usize>,
    column: &str,
    role: &str,
    credits: &mut Vec<ArtistCreditUpdate>,
) {
    let Some(name) = csv_optional_value(record, header_indexes, column) else {
        return;
    };
    let (first_name, last_name) = split_artist_name(&name);
    credits.push(ArtistCreditUpdate {
        first_name,
        last_name,
        role_id: artist_role_id_for_label(role).map(str::to_string),
    });
}

fn split_artist_name(name: &str) -> (Option<String>, Option<String>) {
    let name = name.trim();
    if name.is_empty() {
        return (None, None);
    }
    if let Some((first, last)) = name.rsplit_once(' ') {
        return (
            Some(first.trim().to_string()),
            Some(last.trim().to_string()),
        );
    }
    (None, Some(name.to_string()))
}

fn normalized_csv_header_indexes(headers: &csv::StringRecord) -> BTreeMap<String, usize> {
    let mut indexes = BTreeMap::new();
    for (index, header) in headers.iter().enumerate() {
        indexes
            .entry(header.trim().to_ascii_lowercase())
            .or_insert(index);
    }
    indexes
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

fn csv_bool(
    record: &csv::StringRecord,
    header_indexes: &BTreeMap<String, usize>,
    name: &str,
) -> bool {
    csv_optional_value(record, header_indexes, name)
        .is_some_and(|value| matches!(value.to_ascii_lowercase().as_str(), "yes" | "true" | "1"))
}

fn normalize_snikt_csv_date(
    row_number: usize,
    value: &str,
    messages: &mut Vec<String>,
) -> Option<String> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    if let Ok(date) = NaiveDate::parse_from_str(value, "%Y-%m-%d") {
        return Some(date.format("%Y-%m-%d").to_string());
    }
    for format in ["%m/%d/%Y", "%-m/%-d/%Y"] {
        if let Ok(date) = NaiveDate::parse_from_str(value, format) {
            return Some(date.format("%Y-%m-%d").to_string());
        }
    }
    messages.push(format!(
        "SNIKT.com CSV row {row_number} Created Date \"{value}\" is not a supported date; leaving it blank."
    ));
    None
}

fn safe_path_component(value: &str) -> String {
    let mut sanitized = value
        .chars()
        .map(|character| match character {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            character if character.is_control() => '_',
            character => character,
        })
        .collect::<String>()
        .trim()
        .trim_matches('.')
        .to_string();
    if sanitized.is_empty() {
        sanitized = "SNIKT.com CSV Collection".to_string();
    }
    sanitized
}
