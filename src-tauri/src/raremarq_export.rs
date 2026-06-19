// Copyright (c) 2026 Remgrandt Works. All rights reserved.

use crate::catalog::{ArtworkDetail, Catalog, FileAsset};
use crate::export_policy::ExportPolicy;
use crate::{AppError, Result};
use image::codecs::jpeg::JpegEncoder;
use reqwest::blocking::{multipart, Client};
use serde::{Deserialize, Serialize};
use std::collections::{hash_map::DefaultHasher, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::hash::{Hash, Hasher};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use url::Url;

const RAREMARQ_BULK_UPLOAD_HEADERS: &[&str] = &[
    "title",
    "primary_image_url",
    "category",
    "for_sale",
    "quantity",
    "active",
    "description",
    "accept_offers",
    "min_offer_amount",
    "markets",
    "for_sale_price",
    "discount",
    "discount_price",
    "stock_status",
    "shipping_cost",
    "shipping_cost_intl",
];

const RAREMARQ_BULK_UPLOAD_HELP_ROW: &[&str] = &[
    "[required] text",
    "[required] url",
    "[required] Categories are: \"animation cel\", \"book\" ,\"comic art commission\", \"experience\", \"print\", \"original comic art\", \"signed comic\", \"sketch\", \"sketch card\", \"sketch cover\"",
    "[required] true/false",
    "[required] number",
    "[required] true/false",
    "[optional] text",
    "[optional] true/false",
    "[optional] number",
    "[optional] \"US\", \"Global\"",
    "[optional] number",
    "[optional] true/false",
    "[optional] number",
    "[optional] \"in stock\", \"out of stock\"",
    "[optional] number",
    "[optional] number",
];

const RAREMARQ_MAX_UPLOAD_BYTES: u64 = 20 * 1024 * 1024;
const TMPFILES_EXPIRY_SECONDS: &str = "86400";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RaremarqCsvExportScope {
    All,
    Untracked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RaremarqCsvUrlMode {
    GenericUrl,
    Blank,
    Tmpfiles,
}

#[derive(Debug, Clone)]
pub struct RaremarqCsvExportOptions {
    pub collection_id: i64,
    pub csv_path: PathBuf,
    pub scope: RaremarqCsvExportScope,
    pub url_mode: RaremarqCsvUrlMode,
    pub allow_overwrite: bool,
    pub confirmed_temporary_upload: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RaremarqCsvExportPlan {
    pub collection_id: i64,
    pub total_artworks: usize,
    pub raremarq_tracked_artworks: usize,
    pub all: RaremarqCsvExportPlanScope,
    pub untracked: RaremarqCsvExportPlanScope,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RaremarqCsvExportPlanScope {
    pub rows_exported: usize,
    pub duplicate_raremarq_url_count: usize,
    pub generic_url_blank_count: usize,
    pub blank_url_count: usize,
    pub tmpfiles_upload_count: usize,
    pub tmpfiles_missing_file_count: usize,
    pub tmpfiles_large_file_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RaremarqCsvExportProgress {
    pub phase: String,
    pub message: String,
    pub current: usize,
    pub total: usize,
    pub done: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RaremarqCsvExportReport {
    pub collection_id: i64,
    pub csv_path: PathBuf,
    pub rows_exported: usize,
    pub rows_missing_primary_image_url: usize,
    pub rows_skipped_existing_raremarq_url: usize,
    pub tmpfiles_uploaded: usize,
    pub tmpfiles_resized: usize,
    pub messages: Vec<String>,
}

pub fn raremarq_csv_export_plan(
    catalog: &Catalog,
    collection_id: i64,
) -> Result<RaremarqCsvExportPlan> {
    let collection = catalog.collection_summary(collection_id)?;
    let artworks = collection_artwork_details(catalog, collection.id)?;
    let all = plan_scope(&artworks);
    let untracked_artworks = artworks
        .iter()
        .filter(|detail| !has_raremarq_url(detail))
        .cloned()
        .collect::<Vec<_>>();
    let untracked = plan_scope(&untracked_artworks);
    Ok(RaremarqCsvExportPlan {
        collection_id: collection.id,
        total_artworks: artworks.len(),
        raremarq_tracked_artworks: artworks
            .iter()
            .filter(|detail| has_raremarq_url(detail))
            .count(),
        all,
        untracked,
    })
}

pub fn export_raremarq_csv(
    catalog: &Catalog,
    collection_id: i64,
    csv_path: &Path,
) -> Result<RaremarqCsvExportReport> {
    export_raremarq_csv_with_options(
        catalog,
        RaremarqCsvExportOptions {
            collection_id,
            csv_path: csv_path.to_path_buf(),
            scope: RaremarqCsvExportScope::All,
            url_mode: RaremarqCsvUrlMode::GenericUrl,
            allow_overwrite: false,
            confirmed_temporary_upload: false,
        },
    )
}

pub fn export_raremarq_csv_with_options(
    catalog: &Catalog,
    options: RaremarqCsvExportOptions,
) -> Result<RaremarqCsvExportReport> {
    export_raremarq_csv_with_progress(catalog, options, |_| {})
}

pub fn export_raremarq_csv_with_progress<F>(
    catalog: &Catalog,
    options: RaremarqCsvExportOptions,
    progress: F,
) -> Result<RaremarqCsvExportReport>
where
    F: FnMut(RaremarqCsvExportProgress),
{
    let policy =
        ExportPolicy::provider_upload(options.allow_overwrite, options.confirmed_temporary_upload);
    policy
        .require_temporary_upload_confirmation(options.url_mode == RaremarqCsvUrlMode::Tmpfiles)?;
    let uploader = if options.url_mode == RaremarqCsvUrlMode::Tmpfiles {
        Some(TmpfilesUploader::new()?)
    } else {
        None
    };
    export_raremarq_csv_internal(
        catalog,
        options,
        progress,
        uploader
            .as_ref()
            .map(|uploader| uploader as &dyn TemporaryImageUploader),
    )
}

fn export_raremarq_csv_internal<F>(
    catalog: &Catalog,
    options: RaremarqCsvExportOptions,
    mut progress: F,
    uploader: Option<&dyn TemporaryImageUploader>,
) -> Result<RaremarqCsvExportReport>
where
    F: FnMut(RaremarqCsvExportProgress),
{
    let collection = catalog.collection_summary(options.collection_id)?;
    let all_artworks = collection_artwork_details(catalog, collection.id)?;
    let rows_skipped_existing_raremarq_url = if options.scope == RaremarqCsvExportScope::Untracked {
        all_artworks
            .iter()
            .filter(|detail| has_raremarq_url(detail))
            .count()
    } else {
        0
    };
    let artworks = selected_artworks(all_artworks, options.scope);
    let tmp_dir = if options.url_mode == RaremarqCsvUrlMode::Tmpfiles {
        let path =
            std::env::temp_dir().join(format!("oa-curator-raremarq-export-{}", monotonic_stamp()));
        fs::create_dir_all(&path)?;
        Some(path)
    } else {
        None
    };

    prepare_csv_destination(&options.csv_path, options.allow_overwrite)?;
    let final_csv_path = options.csv_path.clone();
    let temporary_csv_path = temporary_csv_path(&final_csv_path)?;

    if let Some(parent) = temporary_csv_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)?;
    }

    let mut writer = csv::Writer::from_path(&temporary_csv_path)?;
    writer.write_record(RAREMARQ_BULK_UPLOAD_HEADERS)?;
    writer.write_record(RAREMARQ_BULK_UPLOAD_HELP_ROW)?;

    let mut rows_missing_primary_image_url = 0usize;
    let mut tmpfiles_uploaded = 0usize;
    let mut tmpfiles_resized = 0usize;
    let mut messages = Vec::new();
    let total = artworks.len();
    for (index, detail) in artworks.iter().enumerate() {
        let primary_image_url = primary_image_url_for_detail(
            detail,
            options.url_mode,
            tmp_dir.as_deref(),
            uploader,
            index,
            total,
            &mut progress,
        )?;
        if options.url_mode == RaremarqCsvUrlMode::Tmpfiles && primary_image_url.is_some() {
            tmpfiles_uploaded += 1;
            if detail
                .file_assets
                .first()
                .map(|file_asset| file_asset.size_bytes > RAREMARQ_MAX_UPLOAD_BYTES as i64)
                .unwrap_or(false)
            {
                tmpfiles_resized += 1;
            }
        }
        if primary_image_url.is_none() {
            rows_missing_primary_image_url += 1;
            messages.push(missing_url_message(detail, options.url_mode));
        }

        writer.write_record(raremarq_row(detail, primary_image_url.as_deref()))?;
    }
    writer.flush()?;
    drop(writer);
    place_csv_output(
        &temporary_csv_path,
        &final_csv_path,
        options.allow_overwrite,
    )?;
    progress(RaremarqCsvExportProgress {
        phase: "finished".to_string(),
        message: "Raremarq CSV finished writing".to_string(),
        current: total,
        total,
        done: true,
    });
    if let Some(path) = tmp_dir {
        let _ = fs::remove_dir_all(path);
    }

    Ok(RaremarqCsvExportReport {
        collection_id: collection.id,
        csv_path: final_csv_path,
        rows_exported: artworks.len(),
        rows_missing_primary_image_url,
        rows_skipped_existing_raremarq_url,
        tmpfiles_uploaded,
        tmpfiles_resized,
        messages,
    })
}

fn prepare_csv_destination(path: &Path, allow_overwrite: bool) -> Result<()> {
    if path.as_os_str().is_empty() {
        return Err(AppError::Message(
            "Raremarq CSV path cannot be empty".to_string(),
        ));
    }
    if path.try_exists()? && !allow_overwrite {
        return Err(AppError::Message(format!(
            "Raremarq CSV already exists: {}",
            path.display()
        )));
    }
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)?;
    }
    Ok(())
}

fn temporary_csv_path(destination: &Path) -> Result<PathBuf> {
    let file_name = destination
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            AppError::Message(format!(
                "Raremarq CSV path does not include a file name: {}",
                destination.display()
            ))
        })?;
    let parent = destination
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    Ok(parent.join(format!(".{file_name}.{}.tmp", monotonic_stamp())))
}

fn place_csv_output(
    temporary_path: &Path,
    destination: &Path,
    allow_overwrite: bool,
) -> Result<()> {
    if allow_overwrite {
        replace_file_with_temp(temporary_path, destination)
    } else {
        create_file_from_temp_without_overwrite(temporary_path, destination)
    }
}

fn replace_file_with_temp(temporary_path: &Path, destination: &Path) -> Result<()> {
    #[cfg(target_os = "windows")]
    if destination.try_exists()? {
        fs::remove_file(destination)?;
    }
    fs::rename(temporary_path, destination)?;
    Ok(())
}

fn create_file_from_temp_without_overwrite(
    temporary_path: &Path,
    destination: &Path,
) -> Result<()> {
    let mut source = File::open(temporary_path)?;
    let mut destination_file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)
        .map_err(|error| {
            if error.kind() == io::ErrorKind::AlreadyExists {
                AppError::Message(format!(
                    "Raremarq CSV already exists: {}",
                    destination.display()
                ))
            } else {
                AppError::Io(error)
            }
        })?;
    let copy_result = io::copy(&mut source, &mut destination_file)
        .and_then(|_| destination_file.flush())
        .and_then(|_| destination_file.sync_all());
    if let Err(error) = copy_result {
        let _ = fs::remove_file(destination);
        return Err(AppError::Io(error));
    }
    fs::remove_file(temporary_path)?;
    Ok(())
}

fn collection_artwork_details(catalog: &Catalog, collection_id: i64) -> Result<Vec<ArtworkDetail>> {
    let mut details = Vec::new();
    let mut seen = BTreeSet::new();
    for gallery in catalog.galleries_for_collection(collection_id)? {
        for artwork in catalog.artworks_for_gallery(gallery.id)? {
            if seen.insert(artwork.id) {
                details.push(catalog.artwork_detail(artwork.id)?);
            }
        }
    }
    Ok(details)
}

fn selected_artworks(
    artworks: Vec<ArtworkDetail>,
    scope: RaremarqCsvExportScope,
) -> Vec<ArtworkDetail> {
    match scope {
        RaremarqCsvExportScope::All => artworks,
        RaremarqCsvExportScope::Untracked => artworks
            .into_iter()
            .filter(|detail| !has_raremarq_url(detail))
            .collect(),
    }
}

fn plan_scope(artworks: &[ArtworkDetail]) -> RaremarqCsvExportPlanScope {
    RaremarqCsvExportPlanScope {
        rows_exported: artworks.len(),
        duplicate_raremarq_url_count: artworks
            .iter()
            .filter(|detail| has_raremarq_url(detail))
            .count(),
        generic_url_blank_count: artworks
            .iter()
            .filter(|detail| normalized_optional(detail.generic_url.as_deref()).is_none())
            .count(),
        blank_url_count: artworks.len(),
        tmpfiles_upload_count: artworks
            .iter()
            .filter(|detail| !detail.file_assets.is_empty())
            .count(),
        tmpfiles_missing_file_count: artworks
            .iter()
            .filter(|detail| detail.file_assets.is_empty())
            .count(),
        tmpfiles_large_file_count: artworks
            .iter()
            .filter(|detail| {
                detail
                    .file_assets
                    .first()
                    .map(|file_asset| file_asset.size_bytes > RAREMARQ_MAX_UPLOAD_BYTES as i64)
                    .unwrap_or(false)
            })
            .count(),
    }
}

fn primary_image_url_for_detail<F>(
    detail: &ArtworkDetail,
    url_mode: RaremarqCsvUrlMode,
    tmp_dir: Option<&Path>,
    uploader: Option<&dyn TemporaryImageUploader>,
    index: usize,
    total: usize,
    progress: &mut F,
) -> Result<Option<String>>
where
    F: FnMut(RaremarqCsvExportProgress),
{
    match url_mode {
        RaremarqCsvUrlMode::GenericUrl => {
            Ok(normalized_optional(detail.generic_url.as_deref()).map(str::to_string))
        }
        RaremarqCsvUrlMode::Blank => Ok(None),
        RaremarqCsvUrlMode::Tmpfiles => {
            let Some(file_asset) = detail.file_assets.first() else {
                return Ok(None);
            };
            let Some(tmp_dir) = tmp_dir else {
                return Err(AppError::Message(
                    "Temporary upload staging folder was not initialized".to_string(),
                ));
            };
            let Some(uploader) = uploader else {
                return Err(AppError::Message(
                    "Temporary upload client was not initialized".to_string(),
                ));
            };
            let (staged_path, resized) = stage_upload_file(file_asset, tmp_dir, index)?;
            let verb = if resized {
                "Uploading downsized"
            } else {
                "Uploading"
            };
            progress(RaremarqCsvExportProgress {
                phase: "uploading".to_string(),
                message: format!("{verb} image {} of {total}", index + 1),
                current: index,
                total,
                done: false,
            });
            let url = uploader.upload(&staged_path)?;
            progress(RaremarqCsvExportProgress {
                phase: "uploaded".to_string(),
                message: format!("Uploaded image {} of {total}", index + 1),
                current: index + 1,
                total,
                done: false,
            });
            Ok(Some(url))
        }
    }
}

fn stage_upload_file(
    file_asset: &FileAsset,
    tmp_dir: &Path,
    index: usize,
) -> Result<(PathBuf, bool)> {
    let source = &file_asset.current_path;
    let source_size = fs::metadata(source)?.len();
    let resized = source_size > RAREMARQ_MAX_UPLOAD_BYTES;
    let extension = if resized {
        "jpg".to_string()
    } else {
        source
            .extension()
            .and_then(|value| value.to_str())
            .filter(|value| !value.trim().is_empty())
            .unwrap_or("bin")
            .to_ascii_lowercase()
    };
    let staged_path = tmp_dir.join(obfuscated_file_name(source, index, &extension));
    if resized {
        write_downsized_jpeg(source, &staged_path)?;
    } else {
        fs::copy(source, &staged_path)?;
    }
    Ok((staged_path, resized))
}

fn write_downsized_jpeg(source: &Path, target: &Path) -> Result<()> {
    let image = image::open(source)?;
    let mut max_dimension = 2000u32;
    let mut quality = 86u8;
    loop {
        let resized = image.thumbnail(max_dimension, max_dimension);
        let file = File::create(target)?;
        let mut encoder = JpegEncoder::new_with_quality(file, quality);
        encoder.encode_image(&resized)?;
        if fs::metadata(target)?.len() <= RAREMARQ_MAX_UPLOAD_BYTES {
            return Ok(());
        }
        if quality > 58 {
            quality -= 10;
        } else if max_dimension > 640 {
            max_dimension = (max_dimension * 3 / 4).max(640);
            quality = 86;
        } else {
            return Err(AppError::Message(format!(
                "Could not downsize {} below Raremarq's 20 MB limit",
                source.display()
            )));
        }
    }
}

trait TemporaryImageUploader {
    fn upload(&self, staged_path: &Path) -> Result<String>;
}

struct TmpfilesUploader {
    client: Client,
}

impl TmpfilesUploader {
    fn new() -> Result<Self> {
        let client = Client::builder()
            .user_agent("OA Curator Raremarq CSV export")
            .build()
            .map_err(|error| {
                AppError::Message(format!("Could not create upload client: {error}"))
            })?;
        Ok(Self { client })
    }

    fn confirm_live_url(&self, url: &str) -> Result<String> {
        let response = self
            .client
            .get(url)
            .header(reqwest::header::RANGE, "bytes=0-0")
            .send()
            .map_err(|error| AppError::Message(format!("Could not verify temp URL: {error}")))?;
        if response.status().is_success() {
            Ok(url.to_string())
        } else {
            Err(AppError::Message(format!(
                "Temporary URL was not live: {} returned {}",
                url,
                response.status()
            )))
        }
    }
}

impl TemporaryImageUploader for TmpfilesUploader {
    fn upload(&self, staged_path: &Path) -> Result<String> {
        let form = multipart::Form::new()
            .file("file", staged_path)
            .map_err(|error| {
                AppError::Message(format!("Could not read staged upload file: {error}"))
            })?
            .text("expire", TMPFILES_EXPIRY_SECONDS);
        let response = self
            .client
            .post("https://tmpfiles.org/api/v1/upload")
            .multipart(form)
            .send()
            .map_err(|error| AppError::Message(format!("Could not upload temp file: {error}")))?;
        if !response.status().is_success() {
            return Err(AppError::Message(format!(
                "tmpfiles.org upload failed with {}",
                response.status()
            )));
        }
        let payload = response.json::<TmpfilesUploadResponse>().map_err(|error| {
            AppError::Message(format!("Could not read tmpfiles response: {error}"))
        })?;
        if payload.status != "success" {
            return Err(AppError::Message(
                "tmpfiles.org did not accept the upload".to_string(),
            ));
        }
        let url = payload.data.url;
        let direct_url = tmpfiles_direct_download_url(&url).unwrap_or(url);
        self.confirm_live_url(&direct_url)
    }
}

#[derive(Debug, Deserialize)]
struct TmpfilesUploadResponse {
    status: String,
    data: TmpfilesUploadData,
}

#[derive(Debug, Deserialize)]
struct TmpfilesUploadData {
    url: String,
}

fn tmpfiles_direct_download_url(value: &str) -> Option<String> {
    let url = Url::parse(value).ok()?;
    if url.host_str()? != "tmpfiles.org" {
        return None;
    }
    let segments = url.path_segments()?.map(str::to_string).collect::<Vec<_>>();
    if segments.len() < 2 || segments.first()? == "dl" {
        return None;
    }
    let mut direct = url;
    direct.set_path(&format!("/dl/{}/{}", segments[0], segments[1..].join("/")));
    Some(direct.to_string())
}

fn raremarq_row(detail: &ArtworkDetail, primary_image_url: Option<&str>) -> Vec<String> {
    let for_sale = raremarq_for_sale(detail);
    vec![
        detail.title.clone(),
        primary_image_url.unwrap_or("").to_string(),
        raremarq_category(detail).to_string(),
        bool_cell(for_sale),
        "1".to_string(),
        bool_cell(detail.active),
        html_to_text(detail.description.as_deref().unwrap_or("")),
        optional_bool_cell(detail.snikt_metadata.is_open_to_offers),
        String::new(),
        if for_sale {
            "Global".to_string()
        } else {
            String::new()
        },
        if for_sale {
            detail.snikt_metadata.price.clone().unwrap_or_default()
        } else {
            String::new()
        },
        String::new(),
        String::new(),
        if for_sale {
            "in stock".to_string()
        } else {
            String::new()
        },
        String::new(),
        String::new(),
    ]
}

fn missing_url_message(detail: &ArtworkDetail, url_mode: RaremarqCsvUrlMode) -> String {
    match url_mode {
        RaremarqCsvUrlMode::GenericUrl => format!(
            "Artwork \"{}\" has no Generic URL; Raremarq bulk upload requires one.",
            detail.title
        ),
        RaremarqCsvUrlMode::Blank => format!(
            "Artwork \"{}\" has blank URL by export option; Raremarq bulk upload requires one.",
            detail.title
        ),
        RaremarqCsvUrlMode::Tmpfiles => format!(
            "Artwork \"{}\" has no primary file to upload; Raremarq bulk upload requires one.",
            detail.title
        ),
    }
}

fn has_raremarq_url(detail: &ArtworkDetail) -> bool {
    normalized_optional(detail.raremarq_url.as_deref()).is_some()
}

fn normalized_optional(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

fn obfuscated_file_name(source: &Path, index: usize, extension: &str) -> String {
    let mut hasher = DefaultHasher::new();
    source.to_string_lossy().hash(&mut hasher);
    monotonic_stamp().hash(&mut hasher);
    index.hash(&mut hasher);
    format!(
        "oac-{}-{index:04}-{:016x}.{}",
        monotonic_stamp(),
        hasher.finish(),
        extension
    )
}

fn monotonic_stamp() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}

fn raremarq_category(detail: &ArtworkDetail) -> &'static str {
    let value = detail
        .format
        .as_deref()
        .or(detail.art_type_id.as_deref())
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    match value.as_str() {
        "animation" => "animation cel",
        "commission" => "comic art commission",
        "convention sketch" | "mystery sketch" | "prelim" | "sketchbook" => "sketch",
        "mystery sketch card" | "sketch card" | "trading card art" => "sketch card",
        "sketch cover" => "sketch cover",
        _ => "original comic art",
    }
}

fn raremarq_for_sale(detail: &ArtworkDetail) -> bool {
    if detail.snikt_metadata.is_for_sale {
        return true;
    }
    let Some(status) = detail.for_sale_status.as_deref() else {
        return false;
    };
    let status = status.trim().to_ascii_lowercase();
    matches!(
        status.as_str(),
        "for sale" | "fs" | "available" | "available for sale"
    )
}

fn bool_cell(value: bool) -> String {
    if value {
        "TRUE".to_string()
    } else {
        "FALSE".to_string()
    }
}

fn optional_bool_cell(value: bool) -> String {
    if value {
        bool_cell(true)
    } else {
        String::new()
    }
}

fn html_to_text(value: &str) -> String {
    let mut output = String::new();
    let mut in_tag = false;
    for character in value.chars() {
        match character {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => output.push(character),
            _ => {}
        }
    }
    output
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .trim()
        .to_string()
}

impl From<csv::Error> for AppError {
    fn from(error: csv::Error) -> Self {
        Self::Message(format!("CSV error: {error}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tmpfiles_direct_download_url_uses_dl_route_for_api_result() {
        assert_eq!(
            tmpfiles_direct_download_url("https://tmpfiles.org/123/example.jpg").as_deref(),
            Some("https://tmpfiles.org/dl/123/example.jpg")
        );
    }
}
