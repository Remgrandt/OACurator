use crate::catalog::{ArtworkDetail, Catalog, FileAsset};
use crate::csv_safety::spreadsheet_safe_cell;
use crate::export_policy::ExportPolicy;
use crate::image_render::{
    raremarq_upload_jpeg_recipe, render_image_to_file, RenderLimits, RenderPurpose, RenderRequest,
};
use crate::{AppError, Result};
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
    pub tmpfiles_unrenderable_file_count: usize,
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
            .filter(|detail| uploadable_primary_file(detail).is_some())
            .count(),
        tmpfiles_missing_file_count: artworks
            .iter()
            .filter(|detail| detail.file_assets.is_empty())
            .count(),
        tmpfiles_unrenderable_file_count: artworks
            .iter()
            .filter(|detail| {
                detail
                    .file_assets
                    .first()
                    .is_some_and(|file_asset| !is_raremarq_uploadable_file_asset(file_asset))
            })
            .count(),
        tmpfiles_large_file_count: artworks
            .iter()
            .filter(|detail| {
                uploadable_primary_file(detail)
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
            if !is_raremarq_uploadable_file_asset(file_asset) {
                return Ok(None);
            }
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
            let (staged_path, resized) = match stage_upload_file(file_asset, tmp_dir, index) {
                Ok(staged) => staged,
                Err(error) if is_render_staging_failure(&error) => return Ok(None),
                Err(error) => return Err(error),
            };
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

fn uploadable_primary_file(detail: &ArtworkDetail) -> Option<&FileAsset> {
    detail
        .file_assets
        .first()
        .filter(|file_asset| is_raremarq_uploadable_file_asset(file_asset))
}

fn is_raremarq_uploadable_file_asset(file_asset: &FileAsset) -> bool {
    file_asset.width.is_some()
        && file_asset.height.is_some()
        && matches!(
            normalized_file_asset_extension(file_asset).as_deref(),
            Some("jpg" | "jpeg" | "png" | "tif" | "tiff")
        )
}

fn stage_upload_file(
    file_asset: &FileAsset,
    tmp_dir: &Path,
    index: usize,
) -> Result<(PathBuf, bool)> {
    let source = &file_asset.current_path;
    let source_size = fs::metadata(source)?.len();
    let resized = source_size > RAREMARQ_MAX_UPLOAD_BYTES;
    let staged_path = tmp_dir.join(obfuscated_file_name(source, index, "jpg"));
    write_downsized_jpeg(source, &staged_path)?;
    Ok((staged_path, resized))
}

fn normalized_file_asset_extension(file_asset: &FileAsset) -> Option<String> {
    file_asset
        .current_path
        .extension()
        .and_then(|value| value.to_str())
        .or_else(|| {
            let value = file_asset.extension.trim();
            (!value.is_empty()).then_some(value)
        })
        .map(|value| value.trim().trim_start_matches('.').to_ascii_lowercase())
        .filter(|value| !value.is_empty())
}

fn is_render_staging_failure(error: &AppError) -> bool {
    matches!(error, AppError::Render(_))
}

fn write_downsized_jpeg(source: &Path, target: &Path) -> Result<()> {
    let mut max_dimension = 2000u32;
    let mut quality = 86u8;
    loop {
        render_image_to_file(RenderRequest {
            source_path: source.to_path_buf(),
            destination_path: target.to_path_buf(),
            purpose: RenderPurpose::RaremarqUploadImage,
            recipe: raremarq_upload_jpeg_recipe(max_dimension, quality),
            limits: RenderLimits::default(),
        })?;
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
    let row = vec![
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
    ];
    row.into_iter().map(spreadsheet_safe_cell).collect()
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
        RaremarqCsvUrlMode::Tmpfiles => match detail.file_assets.first() {
            None => format!(
                "Artwork \"{}\" has no primary file to upload; Raremarq bulk upload requires one.",
                detail.title
            ),
            Some(file_asset) if !is_raremarq_uploadable_file_asset(file_asset) => format!(
                "Artwork \"{}\" has a primary file that is not a supported image for temporary upload.",
                detail.title
            ),
            Some(_) => format!(
                "Artwork \"{}\" has a primary image that could not be rendered for temporary upload.",
                detail.title
            ),
        },
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
    use std::cell::{Cell, RefCell};
    use tempfile::TempDir;

    #[test]
    fn tmpfiles_direct_download_url_uses_dl_route_for_api_result() {
        assert_eq!(
            tmpfiles_direct_download_url("https://tmpfiles.org/123/example.jpg").as_deref(),
            Some("https://tmpfiles.org/dl/123/example.jpg")
        );
    }

    #[test]
    fn plan_scope_does_not_count_unrenderable_primary_file_as_tmpfiles_upload() {
        let dir = TempDir::new().unwrap();
        let source = dir.path().join("supporting-document.pdf");
        fs::write(&source, b"not an image").unwrap();
        let detail = test_artwork_detail(
            "Reference Document",
            vec![test_file_asset(&source, "pdf", None, None)],
        );

        let scope = plan_scope(&[detail]);

        assert_eq!(scope.tmpfiles_upload_count, 0);
        assert_eq!(scope.tmpfiles_missing_file_count, 0);
        assert_eq!(scope.tmpfiles_unrenderable_file_count, 1);
    }

    #[test]
    fn tmpfiles_primary_url_skips_unrenderable_primary_file_without_uploading() {
        let dir = TempDir::new().unwrap();
        let source = dir.path().join("supporting-document.pdf");
        fs::write(&source, b"not an image").unwrap();
        let detail = test_artwork_detail(
            "Reference Document",
            vec![test_file_asset(&source, "pdf", None, None)],
        );
        let uploader = RecordingUploader::default();
        let mut progress = |_| {};

        let result = primary_image_url_for_detail(
            &detail,
            RaremarqCsvUrlMode::Tmpfiles,
            Some(dir.path()),
            Some(&uploader),
            0,
            1,
            &mut progress,
        )
        .unwrap();

        assert!(result.is_none());
        assert_eq!(uploader.uploads.get(), 0);
    }

    #[test]
    fn tmpfiles_primary_url_skips_tiff_that_cannot_be_rendered_without_uploading() {
        let dir = TempDir::new().unwrap();
        let source = dir.path().join("broken-scan.tif");
        fs::write(&source, b"not a real tiff").unwrap();
        let detail = test_artwork_detail(
            "Broken TIFF",
            vec![test_file_asset(&source, "tif", Some(100), Some(100))],
        );
        let uploader = RecordingUploader::default();
        let mut progress = |_| {};

        let result = primary_image_url_for_detail(
            &detail,
            RaremarqCsvUrlMode::Tmpfiles,
            Some(dir.path()),
            Some(&uploader),
            0,
            1,
            &mut progress,
        )
        .unwrap();

        assert!(result.is_none());
        assert_eq!(uploader.uploads.get(), 0);
    }

    #[test]
    fn tmpfiles_export_warns_when_primary_file_is_not_a_supported_image() {
        let dir = TempDir::new().unwrap();
        let source = dir.path().join("supporting-document.pdf");
        fs::write(&source, b"not an image").unwrap();
        let (catalog, collection_id) =
            catalog_with_artwork_file(&dir, "Reference Document", &source, None, None);
        let uploader = RecordingUploader::default();
        let mut progress = |_| {};

        let report = export_raremarq_csv_internal(
            &catalog,
            RaremarqCsvExportOptions {
                collection_id,
                csv_path: dir.path().join("raremarq.csv"),
                scope: RaremarqCsvExportScope::All,
                url_mode: RaremarqCsvUrlMode::Tmpfiles,
                allow_overwrite: false,
                confirmed_temporary_upload: true,
            },
            &mut progress,
            Some(&uploader),
        )
        .unwrap();

        assert_eq!(report.tmpfiles_uploaded, 0);
        assert_eq!(report.rows_missing_primary_image_url, 1);
        assert_eq!(uploader.uploads.get(), 0);
        assert!(report.messages.iter().any(|message| {
            message.contains("Reference Document") && message.contains("not a supported image")
        }));
    }

    #[test]
    fn tmpfiles_export_warns_when_primary_image_cannot_be_rendered() {
        let dir = TempDir::new().unwrap();
        let source = dir.path().join("broken-scan.tif");
        fs::write(&source, b"not a real tiff").unwrap();
        let (catalog, collection_id) =
            catalog_with_artwork_file(&dir, "Broken TIFF", &source, Some(100), Some(100));
        let uploader = RecordingUploader::default();
        let mut progress = |_| {};

        let report = export_raremarq_csv_internal(
            &catalog,
            RaremarqCsvExportOptions {
                collection_id,
                csv_path: dir.path().join("raremarq.csv"),
                scope: RaremarqCsvExportScope::All,
                url_mode: RaremarqCsvUrlMode::Tmpfiles,
                allow_overwrite: false,
                confirmed_temporary_upload: true,
            },
            &mut progress,
            Some(&uploader),
        )
        .unwrap();

        assert_eq!(report.tmpfiles_uploaded, 0);
        assert_eq!(report.rows_missing_primary_image_url, 1);
        assert_eq!(uploader.uploads.get(), 0);
        assert!(report.messages.iter().any(|message| {
            message.contains("Broken TIFF") && message.contains("could not be rendered")
        }));
    }

    #[test]
    fn tmpfiles_export_renders_small_primary_image_before_uploading() {
        let dir = TempDir::new().unwrap();
        let source = dir.path().join("small-scan.png");
        write_test_png(&source);
        let (catalog, collection_id) =
            catalog_with_artwork_file(&dir, "Small PNG", &source, Some(1), Some(1));
        let uploader = RecordingUploader::default();
        let mut progress = |_| {};

        let report = export_raremarq_csv_internal(
            &catalog,
            RaremarqCsvExportOptions {
                collection_id,
                csv_path: dir.path().join("raremarq.csv"),
                scope: RaremarqCsvExportScope::All,
                url_mode: RaremarqCsvUrlMode::Tmpfiles,
                allow_overwrite: false,
                confirmed_temporary_upload: true,
            },
            &mut progress,
            Some(&uploader),
        )
        .unwrap();

        assert_eq!(report.tmpfiles_uploaded, 1);
        assert_eq!(uploader.uploads.get(), 1);
        assert_eq!(
            uploader.staged_extensions.borrow().clone(),
            vec!["jpg".to_string()]
        );
        let staged_bytes = uploader.staged_bytes.borrow();
        assert_eq!(&staged_bytes[0][..3], &[0xff, 0xd8, 0xff]);
    }

    #[test]
    fn tmpfiles_export_rejects_stale_image_metadata_when_current_file_is_not_renderable() {
        let dir = TempDir::new().unwrap();
        let source = dir.path().join("stale-scan.jpg");
        fs::write(&source, b"%PDF-1.7 not an image").unwrap();
        let (catalog, collection_id) =
            catalog_with_artwork_file(&dir, "Stale JPEG", &source, Some(100), Some(100));
        let uploader = RecordingUploader::default();
        let mut progress = |_| {};

        let report = export_raremarq_csv_internal(
            &catalog,
            RaremarqCsvExportOptions {
                collection_id,
                csv_path: dir.path().join("raremarq.csv"),
                scope: RaremarqCsvExportScope::All,
                url_mode: RaremarqCsvUrlMode::Tmpfiles,
                allow_overwrite: false,
                confirmed_temporary_upload: true,
            },
            &mut progress,
            Some(&uploader),
        )
        .unwrap();

        assert_eq!(report.tmpfiles_uploaded, 0);
        assert_eq!(report.rows_missing_primary_image_url, 1);
        assert_eq!(uploader.uploads.get(), 0);
        assert!(report.messages.iter().any(|message| {
            message.contains("Stale JPEG") && message.contains("could not be rendered")
        }));
    }

    fn catalog_with_artwork_file(
        dir: &TempDir,
        title: &str,
        source: &Path,
        width: Option<i64>,
        height: Option<i64>,
    ) -> (Catalog, i64) {
        let catalog = Catalog::open(dir.path().join("catalog.sqlite")).unwrap();
        catalog.init().unwrap();
        let collection = catalog
            .create_collection("Manual", &dir.path().join("Manual/.oacollection"))
            .unwrap();
        let gallery = catalog
            .create_gallery(
                "Manual Gallery",
                &dir.path().join("Manual/galleries/Manual/.oagallery"),
            )
            .unwrap();
        catalog
            .link_gallery_to_collection(collection.id, gallery.id)
            .unwrap();
        let artwork = catalog
            .create_artwork_in_gallery(gallery.id, title, None)
            .unwrap();
        catalog
            .upsert_file_asset_with_known_metadata(
                artwork.id,
                crate::catalog::FileAssetKnownMetadataInsert {
                    original_path: source,
                    root: dir.path(),
                    path: source,
                    is_primary: true,
                    source_kind: "imported",
                    metadata: crate::catalog::FileAssetMetadata {
                        width,
                        height,
                        dpi_x: None,
                        dpi_y: None,
                    },
                },
            )
            .unwrap();
        (catalog, collection.id)
    }

    fn write_test_png(path: &Path) {
        const ONE_BY_ONE_PNG: &[u8] = &[
            137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1,
            8, 4, 0, 0, 0, 181, 28, 12, 2, 0, 0, 0, 11, 73, 68, 65, 84, 120, 218, 99, 252, 255, 31,
            0, 3, 3, 2, 0, 239, 191, 167, 219, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
        ];
        fs::write(path, ONE_BY_ONE_PNG).unwrap();
    }

    #[derive(Default)]
    struct RecordingUploader {
        uploads: Cell<usize>,
        staged_extensions: RefCell<Vec<String>>,
        staged_bytes: RefCell<Vec<Vec<u8>>>,
    }

    impl TemporaryImageUploader for RecordingUploader {
        fn upload(&self, staged_path: &Path) -> Result<String> {
            self.uploads.set(self.uploads.get() + 1);
            self.staged_extensions.borrow_mut().push(
                staged_path
                    .extension()
                    .and_then(|value| value.to_str())
                    .unwrap_or_default()
                    .to_string(),
            );
            self.staged_bytes.borrow_mut().push(fs::read(staged_path)?);
            Ok("https://tmpfiles.org/dl/test-image.jpg".to_string())
        }
    }

    fn test_artwork_detail(title: &str, file_assets: Vec<FileAsset>) -> ArtworkDetail {
        ArtworkDetail {
            id: 1,
            canonical_id: "OAC-00001".to_string(),
            display_id: "OAC-00001".to_string(),
            caf_artwork_id: None,
            snikt_artwork_id: None,
            raremarq_artwork_id: None,
            title: title.to_string(),
            description: None,
            for_sale_status: None,
            media_type_id: None,
            media: None,
            art_type_id: None,
            format: None,
            publication_status_id: None,
            active: true,
            illustration_exchange: false,
            ix_for_sale: false,
            caf_url: None,
            snikt_url: None,
            raremarq_url: None,
            generic_url: None,
            caf_csv_image_link: None,
            caf_csv_added_to_caf: None,
            snikt_csv_created_date: None,
            snikt_metadata: crate::catalog::SniktMetadata::default(),
            purchase_price: None,
            estimated_value: None,
            purchase_date: None,
            provenance: None,
            personal_notes: None,
            source_folder: PathBuf::new(),
            artist_credits: Vec::new(),
            file_assets,
            derived_assets: Vec::new(),
            cache_warnings: Vec::new(),
        }
    }

    fn test_file_asset(
        source: &Path,
        extension: &str,
        width: Option<i64>,
        height: Option<i64>,
    ) -> FileAsset {
        FileAsset {
            id: 1,
            artwork_id: 1,
            original_path: source.to_path_buf(),
            current_path: source.to_path_buf(),
            relative_path: source
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or_default()
                .to_string(),
            file_name: source
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or_default()
                .to_string(),
            extension: extension.to_string(),
            size_bytes: fs::metadata(source).unwrap().len() as i64,
            width,
            height,
            dpi_x: None,
            dpi_y: None,
            image_role: None,
            source_kind: "imported".to_string(),
            is_primary: true,
        }
    }
}
