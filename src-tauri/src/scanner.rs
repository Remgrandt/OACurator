// Copyright (c) 2026 Remgrandt Works. All rights reserved.

use crate::catalog::{
    ArtworkCacheWarning, ArtworkDetail, AssetKind, Catalog, DerivedAssetInsert,
    FileAssetKnownMetadataInsert, FileAssetMetadata,
};
use crate::jobs::JobCancellation;
use crate::{AppError, Result};
use image::codecs::jpeg::JpegEncoder;
use image::imageops::FilterType;
use image::{DynamicImage, ImageReader, Limits};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::env;
use std::fs;
use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc, Condvar, Mutex};
use std::thread;
use std::time::{Instant, UNIX_EPOCH};

const CACHE_DECODE_MAX_ALLOC_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const CACHE_GENERATION_MEMORY_BUDGET_BYTES: u64 = 512 * 1024 * 1024;
const CACHE_PREVIEW_MAX_HEIGHT: u32 = 2000;
const CACHE_THUMBNAIL_MAX_DIMENSION: u32 = 320;
const CACHE_RESIZE_FILTER: FilterType = FilterType::Nearest;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttachMode {
    Copy,
    Link,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct CacheDerivativeProfile {
    pub total_ms: u128,
    pub open_ms: u128,
    pub thumbnail_resize_ms: u128,
    pub thumbnail_write_ms: u128,
    pub thumbnail_db_ms: u128,
    pub preview_resize_ms: u128,
    pub preview_write_ms: u128,
    pub preview_db_ms: u128,
    pub thumbnail_bytes: u64,
    pub preview_bytes: u64,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct FileAssetIndexProfile {
    pub total_ms: u128,
    pub dimensions_check_ms: u128,
    pub file_asset_upsert_ms: u128,
    pub cache_derivatives: CacheDerivativeProfile,
    pub role_update_ms: u128,
}

#[derive(Debug, Clone, Copy)]
pub struct CacheDerivativeOptions {
    pub create_thumbnail: bool,
    pub create_preview: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct FileAssetIndexRequest<'a> {
    pub path: &'a Path,
    pub cache_dir: &'a Path,
    pub is_primary: bool,
    pub image_role: Option<&'a str>,
    pub cache_options: CacheDerivativeOptions,
    pub source_kind: &'a str,
}

impl Default for CacheDerivativeOptions {
    fn default() -> Self {
        Self {
            create_thumbnail: true,
            create_preview: true,
        }
    }
}

#[derive(Debug, Clone)]
pub struct GeneratedCacheDerivative {
    pub derivative_type: String,
    pub format: String,
    pub path: PathBuf,
    pub width: i64,
    pub height: i64,
}

#[derive(Debug, Clone)]
pub struct GeneratedCacheDerivatives {
    pub artwork_id: i64,
    pub file_asset_id: i64,
    pub derivatives: Vec<GeneratedCacheDerivative>,
    pub profile: CacheDerivativeProfile,
}

#[derive(Debug, Clone)]
pub struct ThumbnailCacheWorkItem {
    pub artwork_id: i64,
    pub file_asset_id: i64,
    pub source_path: PathBuf,
    pub estimated_decode_bytes: u64,
}

#[derive(Debug)]
pub struct ThumbnailCacheWorkResult {
    pub item: ThumbnailCacheWorkItem,
    pub result: std::result::Result<GeneratedCacheDerivatives, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThumbnailCacheProgress {
    pub phase: String,
    pub message: String,
    pub total: usize,
    pub completed: usize,
    pub succeeded: usize,
    pub failed: usize,
    pub current_path: Option<PathBuf>,
    pub done: bool,
}

pub fn attach_files_to_artwork(
    catalog: &Catalog,
    artwork_id: i64,
    paths: &[PathBuf],
    cache_dir: &Path,
    mode: AttachMode,
) -> Result<ArtworkDetail> {
    if paths.is_empty() {
        return Err(AppError::Message("Choose at least one file".to_string()));
    }

    for path in paths {
        if !path.is_file() {
            return Err(AppError::Message(format!(
                "File does not exist: {}",
                path.display()
            )));
        }
        if is_supported_image(path) {
            image::image_dimensions(path)?;
        }
    }

    let existing_files = catalog.artwork_detail(artwork_id)?.file_assets.len();
    let mut cache_warnings = Vec::new();
    for (index, path) in paths.iter().enumerate() {
        let current_path = match mode {
            AttachMode::Copy => copy_into_artwork_assets(catalog, artwork_id, path)?,
            AttachMode::Link => path.clone(),
        };
        let source_kind = match mode {
            AttachMode::Copy => "copied",
            AttachMode::Link => "linked",
        };
        let root = current_path.parent().unwrap_or_else(|| Path::new(""));
        let is_primary = existing_files == 0 && index == 0;
        let file_asset_id = if is_supported_image(&current_path) {
            catalog.upsert_file_asset_with_paths(
                artwork_id,
                path,
                root,
                &current_path,
                is_primary,
                source_kind,
            )?
        } else {
            catalog.upsert_file_asset_with_known_metadata(
                artwork_id,
                FileAssetKnownMetadataInsert {
                    original_path: path,
                    root,
                    path: &current_path,
                    is_primary,
                    source_kind,
                    metadata: FileAssetMetadata {
                        width: None,
                        height: None,
                        dpi_x: None,
                        dpi_y: None,
                    },
                },
            )?
        };
        if is_supported_image(&current_path) {
            if let Err(error) = generate_cached_derivatives(
                catalog,
                artwork_id,
                file_asset_id,
                &current_path,
                cache_dir,
            ) {
                cache_warnings.push(cache_warning_for_error(file_asset_id, &current_path, error));
            }
        }
    }

    catalog.ensure_artwork_manifest(artwork_id)?;
    let mut detail = catalog.artwork_detail(artwork_id)?;
    detail.cache_warnings = cache_warnings;
    Ok(detail)
}

pub fn index_local_file_asset(
    catalog: &Catalog,
    artwork_id: i64,
    path: &Path,
    cache_dir: &Path,
    is_primary: bool,
) -> Result<i64> {
    let (file_asset_id, _) = index_file_asset_profiled(
        catalog,
        artwork_id,
        FileAssetIndexRequest {
            path,
            cache_dir,
            is_primary,
            image_role: None,
            cache_options: CacheDerivativeOptions::default(),
            source_kind: "linked",
        },
    )?;
    Ok(file_asset_id)
}

fn index_file_asset_profiled(
    catalog: &Catalog,
    artwork_id: i64,
    request: FileAssetIndexRequest<'_>,
) -> Result<(i64, FileAssetIndexProfile)> {
    let total_started = Instant::now();
    let mut profile = FileAssetIndexProfile::default();
    if !request.path.is_file() {
        return Err(AppError::Message(format!(
            "File does not exist: {}",
            request.path.display()
        )));
    }
    if !is_supported_image_extension(request.path) {
        return Err(AppError::Message(format!(
            "Unsupported image type: {}",
            request.path.display()
        )));
    }
    let dimensions_started = Instant::now();
    image::image_dimensions(request.path)?;
    profile.dimensions_check_ms = elapsed_ms(dimensions_started);
    let root = request.path.parent().unwrap_or_else(|| Path::new(""));
    let upsert_started = Instant::now();
    let file_asset_id = catalog.upsert_file_asset_with_paths(
        artwork_id,
        request.path,
        root,
        request.path,
        request.is_primary,
        request.source_kind,
    )?;
    profile.file_asset_upsert_ms = elapsed_ms(upsert_started);
    profile.cache_derivatives = generate_cached_derivatives_profiled(
        catalog,
        artwork_id,
        file_asset_id,
        request.path,
        request.cache_dir,
        request.cache_options,
    )?;
    catalog.ensure_artwork_manifest(artwork_id)?;
    profile.total_ms = elapsed_ms(total_started);
    Ok((file_asset_id, profile))
}

pub fn index_local_file_asset_with_role(
    catalog: &Catalog,
    artwork_id: i64,
    path: &Path,
    cache_dir: &Path,
    is_primary: bool,
    image_role: Option<&str>,
) -> Result<i64> {
    let (file_asset_id, _) = index_local_file_asset_with_role_profiled(
        catalog, artwork_id, path, cache_dir, is_primary, image_role,
    )?;
    Ok(file_asset_id)
}

pub fn index_local_file_asset_with_role_profiled(
    catalog: &Catalog,
    artwork_id: i64,
    path: &Path,
    cache_dir: &Path,
    is_primary: bool,
    image_role: Option<&str>,
) -> Result<(i64, FileAssetIndexProfile)> {
    index_local_file_asset_with_role_profiled_options(
        catalog,
        artwork_id,
        path,
        cache_dir,
        is_primary,
        image_role,
        CacheDerivativeOptions::default(),
    )
}

pub fn index_local_file_asset_with_role_profiled_options(
    catalog: &Catalog,
    artwork_id: i64,
    path: &Path,
    cache_dir: &Path,
    is_primary: bool,
    image_role: Option<&str>,
    cache_options: CacheDerivativeOptions,
) -> Result<(i64, FileAssetIndexProfile)> {
    index_local_file_asset_with_role_profiled_options_for_source_kind(
        catalog,
        artwork_id,
        FileAssetIndexRequest {
            path,
            cache_dir,
            is_primary,
            image_role,
            cache_options,
            source_kind: "linked",
        },
    )
}

pub fn index_local_file_asset_with_role_profiled_options_for_source_kind(
    catalog: &Catalog,
    artwork_id: i64,
    request: FileAssetIndexRequest<'_>,
) -> Result<(i64, FileAssetIndexProfile)> {
    let total_started = Instant::now();
    let (file_asset_id, mut profile) = index_file_asset_profiled(catalog, artwork_id, request)?;
    if let Some(image_role) = request.image_role {
        let role_started = Instant::now();
        catalog.update_image_role(AssetKind::File, file_asset_id, Some(image_role))?;
        profile.role_update_ms = elapsed_ms(role_started);
    }
    profile.total_ms = elapsed_ms(total_started);
    Ok((file_asset_id, profile))
}

fn copy_into_artwork_assets(catalog: &Catalog, artwork_id: i64, source: &Path) -> Result<PathBuf> {
    let asset_folder = catalog.artwork_asset_folder(artwork_id)?;
    fs::create_dir_all(&asset_folder)?;
    if source
        .parent()
        .and_then(|parent| parent.canonicalize().ok())
        == asset_folder.canonicalize().ok()
    {
        return Ok(source.to_path_buf());
    }

    let destination = unique_copy_destination(&asset_folder, source)?;
    fs::copy(source, &destination)?;
    Ok(destination)
}

fn unique_copy_destination(folder: &Path, source: &Path) -> Result<PathBuf> {
    let file_name = safe_copy_file_name(source);
    let first = folder.join(&file_name);
    if !first.exists() {
        return Ok(first);
    }

    let stem = Path::new(&file_name)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("image");
    let extension = Path::new(&file_name)
        .extension()
        .and_then(|value| value.to_str());
    for index in 2..10_000 {
        let candidate_name = match extension {
            Some(extension) if !extension.is_empty() => format!("{stem} {index}.{extension}"),
            _ => format!("{stem} {index}"),
        };
        let candidate = folder.join(candidate_name);
        if !candidate.exists() {
            return Ok(candidate);
        }
    }
    Err(AppError::Message(format!(
        "Could not choose a unique file name in {}",
        folder.display()
    )))
}

fn safe_copy_file_name(source: &Path) -> String {
    let raw = source
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("image");
    let cleaned = raw
        .chars()
        .map(|character| {
            if character.is_control()
                || matches!(
                    character,
                    '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*'
                )
            {
                ' '
            } else {
                character
            }
        })
        .collect::<String>();
    let cleaned = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    let cleaned = cleaned.trim_matches([' ', '.']).trim();
    if cleaned.is_empty() {
        "image".to_string()
    } else {
        cleaned.to_string()
    }
}

pub fn is_supported_image(path: &Path) -> bool {
    is_supported_image_extension(path)
}

fn is_supported_image_extension(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.to_ascii_lowercase())
            .as_deref(),
        Some("jpg" | "jpeg" | "png" | "tif" | "tiff")
    )
}

fn generate_cached_derivatives(
    catalog: &Catalog,
    artwork_id: i64,
    file_asset_id: i64,
    source: &Path,
    cache_dir: &Path,
) -> Result<()> {
    generate_cached_derivatives_profiled(
        catalog,
        artwork_id,
        file_asset_id,
        source,
        cache_dir,
        CacheDerivativeOptions::default(),
    )?;
    Ok(())
}

fn generate_cached_derivatives_profiled(
    catalog: &Catalog,
    artwork_id: i64,
    file_asset_id: i64,
    source: &Path,
    cache_dir: &Path,
    options: CacheDerivativeOptions,
) -> Result<CacheDerivativeProfile> {
    let generated =
        build_cached_derivatives_profiled(artwork_id, file_asset_id, source, cache_dir, options)?;
    register_cached_derivatives_profiled(catalog, generated)
}

pub fn build_cached_derivatives_profiled(
    artwork_id: i64,
    file_asset_id: i64,
    source: &Path,
    cache_dir: &Path,
    options: CacheDerivativeOptions,
) -> Result<GeneratedCacheDerivatives> {
    let total_started = Instant::now();
    if !options.create_thumbnail && !options.create_preview {
        return Ok(GeneratedCacheDerivatives {
            artwork_id,
            file_asset_id,
            derivatives: Vec::new(),
            profile: CacheDerivativeProfile::default(),
        });
    }

    let folder = image_stable_cache_folder(source, cache_dir)?;
    fs::create_dir_all(&folder)?;
    let mut derivatives = Vec::new();
    let existing_thumbnail = if options.create_thumbnail {
        existing_cache_derivative(&folder, "thumbnail")?
    } else {
        None
    };
    let existing_preview = if options.create_preview {
        existing_cache_derivative(&folder, "preview")?
    } else {
        None
    };
    let create_thumbnail = options.create_thumbnail && existing_thumbnail.is_none();
    let create_preview = options.create_preview && existing_preview.is_none();
    if let Some(derivative) = existing_thumbnail {
        derivatives.push(derivative);
    }
    if let Some(derivative) = existing_preview {
        derivatives.push(derivative);
    }
    if !create_thumbnail && !create_preview {
        return Ok(GeneratedCacheDerivatives {
            artwork_id,
            file_asset_id,
            derivatives,
            profile: CacheDerivativeProfile {
                total_ms: elapsed_ms(total_started),
                ..CacheDerivativeProfile::default()
            },
        });
    }

    let open_started = Instant::now();
    let image = decode_cache_source(source)?;
    let open_ms = elapsed_ms(open_started);
    let mut preview_image = None;

    let mut preview_resize_ms = 0;
    if create_preview {
        let preview_resize_started = Instant::now();
        preview_image = Some(cache_preview_image(&image));
        preview_resize_ms = elapsed_ms(preview_resize_started);
    }

    let mut thumbnail_resize_ms = 0;
    let mut thumbnail_write_ms = 0;
    let mut thumbnail_bytes = 0;
    let mut thumbnail_image = None;
    if create_thumbnail {
        let thumbnail_resize_started = Instant::now();
        thumbnail_image = Some(match preview_image.as_ref() {
            Some(preview) => cache_thumbnail_image(preview),
            None => cache_thumbnail_image(&image),
        });
        thumbnail_resize_ms = elapsed_ms(thumbnail_resize_started);
    }

    let cache_format = thumbnail_image
        .as_ref()
        .map(cache_format_for_image)
        .or_else(|| {
            preview_image
                .as_ref()
                .map(|preview| cache_format_for_image(preview.as_image()))
        })
        .unwrap_or(CacheImageFormat::Jpeg);

    if let Some(thumbnail) = thumbnail_image {
        let thumbnail_path = folder.join(format!("thumbnail.{}", cache_format.extension()));
        let thumbnail_write_started = Instant::now();
        write_cache_image(&thumbnail, &thumbnail_path, cache_format)?;
        thumbnail_write_ms = elapsed_ms(thumbnail_write_started);
        thumbnail_bytes = fs::metadata(&thumbnail_path).map(|metadata| metadata.len())?;
        derivatives.push(GeneratedCacheDerivative {
            derivative_type: "thumbnail".to_string(),
            format: cache_format.format().to_string(),
            path: thumbnail_path,
            width: thumbnail.width() as i64,
            height: thumbnail.height() as i64,
        });
    }

    let mut preview_write_ms = 0;
    let mut preview_bytes = 0;
    if let Some(preview) = preview_image {
        let preview = preview.as_image();
        let preview_path = folder.join(format!("preview.{}", cache_format.extension()));
        let preview_write_started = Instant::now();
        write_cache_image(preview, &preview_path, cache_format)?;
        preview_write_ms = elapsed_ms(preview_write_started);
        preview_bytes = fs::metadata(&preview_path).map(|metadata| metadata.len())?;
        derivatives.push(GeneratedCacheDerivative {
            derivative_type: "preview".to_string(),
            format: cache_format.format().to_string(),
            path: preview_path,
            width: preview.width() as i64,
            height: preview.height() as i64,
        });
    }

    Ok(GeneratedCacheDerivatives {
        artwork_id,
        file_asset_id,
        derivatives,
        profile: CacheDerivativeProfile {
            total_ms: elapsed_ms(total_started),
            open_ms,
            thumbnail_resize_ms,
            thumbnail_write_ms,
            thumbnail_db_ms: 0,
            preview_resize_ms,
            preview_write_ms,
            preview_db_ms: 0,
            thumbnail_bytes,
            preview_bytes,
        },
    })
}

fn image_stable_cache_folder(source: &Path, cache_dir: &Path) -> Result<PathBuf> {
    let metadata = fs::metadata(source)?;
    let canonical = fs::canonicalize(source).unwrap_or_else(|_| source.to_path_buf());
    let modified = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok());
    let (modified_secs, modified_nanos) = modified
        .map(|duration| (duration.as_secs(), duration.subsec_nanos()))
        .unwrap_or((0, 0));
    let identity = format!(
        "{}|{}|{}|{}",
        canonical.to_string_lossy(),
        metadata.len(),
        modified_secs,
        modified_nanos
    );
    Ok(cache_dir
        .join("images")
        .join(format!("{:016x}", fnv1a64(identity.as_bytes()))))
}

fn existing_cache_derivative(
    folder: &Path,
    derivative_type: &str,
) -> Result<Option<GeneratedCacheDerivative>> {
    for extension in ["jpg", "png"] {
        let path = folder.join(format!("{derivative_type}.{extension}"));
        if !path.is_file() {
            continue;
        }
        let reader = ImageReader::open(&path)?.with_guessed_format()?;
        let (width, height) = reader.into_dimensions()?;
        let format = match extension {
            "png" => "PNG",
            _ => "JPEG",
        };
        return Ok(Some(GeneratedCacheDerivative {
            derivative_type: derivative_type.to_string(),
            format: format.to_string(),
            path,
            width: width as i64,
            height: height as i64,
        }));
    }
    Ok(None)
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

pub fn register_cached_derivatives_profiled(
    catalog: &Catalog,
    generated: GeneratedCacheDerivatives,
) -> Result<CacheDerivativeProfile> {
    register_cached_derivatives_with_manifest_rewrite(catalog, generated, true)
}

pub fn register_cached_derivatives_session_only_profiled(
    catalog: &Catalog,
    generated: GeneratedCacheDerivatives,
) -> Result<CacheDerivativeProfile> {
    register_cached_derivatives_with_manifest_rewrite(catalog, generated, false)
}

fn register_cached_derivatives_with_manifest_rewrite(
    catalog: &Catalog,
    generated: GeneratedCacheDerivatives,
    rewrite_artwork_manifest: bool,
) -> Result<CacheDerivativeProfile> {
    let mut profile = generated.profile;
    for derivative in generated.derivatives {
        let db_started = Instant::now();
        let insert = DerivedAssetInsert {
            source_file_asset_id: Some(generated.file_asset_id),
            derivative_type: &derivative.derivative_type,
            format: &derivative.format,
            path: &derivative.path,
            width: derivative.width,
            height: derivative.height,
            image_role: None,
        };
        if rewrite_artwork_manifest {
            catalog.add_derived_asset(generated.artwork_id, insert)?;
        } else {
            catalog.add_derived_asset_session_only(generated.artwork_id, insert)?;
        }
        let db_ms = elapsed_ms(db_started);
        match derivative.derivative_type.as_str() {
            "thumbnail" => profile.thumbnail_db_ms += db_ms,
            "preview" => profile.preview_db_ms += db_ms,
            _ => {}
        }
        profile.total_ms += db_ms;
    }
    Ok(profile)
}

pub fn ensure_artwork_cache_derivatives(
    catalog: &Catalog,
    artwork_id: i64,
    cache_dir: &Path,
) -> Result<CacheDerivativeProfile> {
    let detail = catalog.artwork_detail(artwork_id)?;
    let mut profile = CacheDerivativeProfile::default();
    for asset in detail
        .file_assets
        .iter()
        .filter(|asset| asset.current_path.is_file() && is_supported_image(&asset.current_path))
    {
        let has_thumbnail = detail.derived_assets.iter().any(|derived| {
            derived.source_file_asset_id == Some(asset.id)
                && derived.derivative_type == "thumbnail"
                && derived.path.is_file()
        });
        let has_preview = detail.derived_assets.iter().any(|derived| {
            derived.source_file_asset_id == Some(asset.id)
                && derived.derivative_type == "preview"
                && derived.path.is_file()
        });
        if has_thumbnail && has_preview {
            continue;
        }
        let generated = build_cached_derivatives_profiled(
            artwork_id,
            asset.id,
            &asset.current_path,
            cache_dir,
            CacheDerivativeOptions {
                create_thumbnail: !has_thumbnail,
                create_preview: !has_preview,
            },
        )?;
        let generated_profile =
            register_cached_derivatives_session_only_profiled(catalog, generated)?;
        profile = combine_cache_profiles(profile, generated_profile);
    }
    Ok(profile)
}

pub fn ensure_artwork_cache_derivatives_with_warnings(
    catalog: &Catalog,
    artwork_id: i64,
    cache_dir: &Path,
) -> Result<(CacheDerivativeProfile, Vec<ArtworkCacheWarning>)> {
    let detail = catalog.artwork_detail(artwork_id)?;
    let mut profile = CacheDerivativeProfile::default();
    let mut warnings = Vec::new();
    for asset in detail
        .file_assets
        .iter()
        .filter(|asset| asset.current_path.is_file() && is_supported_image(&asset.current_path))
    {
        let has_thumbnail = detail.derived_assets.iter().any(|derived| {
            derived.source_file_asset_id == Some(asset.id)
                && derived.derivative_type == "thumbnail"
                && derived.path.is_file()
        });
        let has_preview = detail.derived_assets.iter().any(|derived| {
            derived.source_file_asset_id == Some(asset.id)
                && derived.derivative_type == "preview"
                && derived.path.is_file()
        });
        if has_thumbnail && has_preview {
            continue;
        }
        match build_cached_derivatives_profiled(
            artwork_id,
            asset.id,
            &asset.current_path,
            cache_dir,
            CacheDerivativeOptions {
                create_thumbnail: !has_thumbnail,
                create_preview: !has_preview,
            },
        ) {
            Ok(generated) => {
                let generated_profile =
                    register_cached_derivatives_session_only_profiled(catalog, generated)?;
                profile = combine_cache_profiles(profile, generated_profile);
            }
            Err(error) => warnings.push(cache_warning_for_error(
                asset.id,
                &asset.current_path,
                error,
            )),
        }
    }
    Ok((profile, warnings))
}

pub fn thumbnail_cache_work_items_for_collection(
    catalog: &Catalog,
    collection_id: i64,
) -> Result<Vec<ThumbnailCacheWorkItem>> {
    Ok(catalog
        .file_assets_missing_thumbnail_for_collection(collection_id)?
        .into_iter()
        .filter(|(_, _, source_path)| source_path.is_file() && is_supported_image(source_path))
        .map(|(artwork_id, file_asset_id, source_path)| {
            let estimated_decode_bytes =
                estimate_cache_decode_bytes(&source_path).unwrap_or(CACHE_DECODE_MAX_ALLOC_BYTES);
            ThumbnailCacheWorkItem {
                artwork_id,
                file_asset_id,
                source_path,
                estimated_decode_bytes,
            }
        })
        .collect())
}

pub fn preview_cache_work_items_for_collection(
    catalog: &Catalog,
    collection_id: i64,
) -> Result<Vec<ThumbnailCacheWorkItem>> {
    Ok(catalog
        .file_assets_missing_preview_for_collection(collection_id)?
        .into_iter()
        .filter(|(_, _, source_path)| source_path.is_file() && is_supported_image(source_path))
        .map(|(artwork_id, file_asset_id, source_path)| {
            let estimated_decode_bytes =
                estimate_cache_decode_bytes(&source_path).unwrap_or(CACHE_DECODE_MAX_ALLOC_BYTES);
            ThumbnailCacheWorkItem {
                artwork_id,
                file_asset_id,
                source_path,
                estimated_decode_bytes,
            }
        })
        .collect())
}

pub fn register_thumbnail_cache_work_result(
    catalog: &Catalog,
    result: ThumbnailCacheWorkResult,
) -> Result<()> {
    match result.result {
        Ok(generated) => {
            register_cached_derivatives_session_only_profiled(catalog, generated)?;
            Ok(())
        }
        Err(error) => Err(AppError::Message(error)),
    }
}

pub fn generate_thumbnail_cache_parallel<F>(
    cache_work_items: Vec<ThumbnailCacheWorkItem>,
    cache_dir: &Path,
    worker_count: usize,
    on_result: F,
) where
    F: FnMut(ThumbnailCacheWorkResult),
{
    generate_cache_derivatives_parallel(
        cache_work_items,
        cache_dir,
        worker_count,
        CacheDerivativeOptions {
            create_thumbnail: true,
            create_preview: false,
        },
        on_result,
    );
}

pub fn generate_cache_derivatives_parallel<F>(
    cache_work_items: Vec<ThumbnailCacheWorkItem>,
    cache_dir: &Path,
    worker_count: usize,
    options: CacheDerivativeOptions,
    mut on_result: F,
) where
    F: FnMut(ThumbnailCacheWorkResult),
{
    generate_cache_derivatives_parallel_with_cancellation(
        cache_work_items,
        cache_dir,
        worker_count,
        options,
        None,
        &mut on_result,
    );
}

pub fn generate_cache_derivatives_parallel_with_cancellation<F>(
    cache_work_items: Vec<ThumbnailCacheWorkItem>,
    cache_dir: &Path,
    worker_count: usize,
    options: CacheDerivativeOptions,
    cancellation: Option<JobCancellation>,
    mut on_result: F,
) where
    F: FnMut(ThumbnailCacheWorkResult),
{
    let queue = Arc::new((
        Mutex::new(CacheWorkQueue {
            pending: VecDeque::from(cache_work_items),
            in_flight_bytes: 0,
        }),
        Condvar::new(),
    ));
    let cache_dir = cache_dir.to_path_buf();
    let (sender, receiver) = mpsc::channel();
    let memory_budget = cache_generation_memory_budget_bytes();

    thread::scope(|scope| {
        for _ in 0..worker_count {
            let queue = Arc::clone(&queue);
            let sender = sender.clone();
            let cache_dir = cache_dir.clone();
            let cancellation = cancellation.clone();
            scope.spawn(move || loop {
                if cancellation
                    .as_ref()
                    .is_some_and(JobCancellation::is_canceled)
                {
                    break;
                }
                let item = next_cache_work_item(&queue, memory_budget);
                let Some(item) = item else {
                    break;
                };
                if cancellation
                    .as_ref()
                    .is_some_and(JobCancellation::is_canceled)
                {
                    finish_cache_work_item(&queue, item.estimated_decode_bytes);
                    break;
                }
                let estimated_decode_bytes = item.estimated_decode_bytes;
                let result = build_cached_derivatives_profiled(
                    item.artwork_id,
                    item.file_asset_id,
                    &item.source_path,
                    &cache_dir,
                    options,
                )
                .map_err(|error| error.to_string());
                finish_cache_work_item(&queue, estimated_decode_bytes);
                let _ = sender.send(ThumbnailCacheWorkResult { item, result });
            });
        }
        drop(sender);
        for result in receiver {
            if cancellation
                .as_ref()
                .is_some_and(JobCancellation::is_canceled)
            {
                continue;
            }
            on_result(result);
        }
    });
}

struct CacheWorkQueue {
    pending: VecDeque<ThumbnailCacheWorkItem>,
    in_flight_bytes: u64,
}

fn next_cache_work_item(
    queue: &Arc<(Mutex<CacheWorkQueue>, Condvar)>,
    memory_budget: u64,
) -> Option<ThumbnailCacheWorkItem> {
    let (lock, available) = &**queue;
    let mut guard = lock.lock().expect("thumbnail queue mutex poisoned");
    loop {
        if guard.pending.is_empty() {
            return None;
        }
        let next_index = guard.pending.iter().position(|item| {
            cache_work_item_fits_budget(
                item.estimated_decode_bytes,
                guard.in_flight_bytes,
                memory_budget,
            )
        });
        if let Some(index) = next_index {
            let item = guard
                .pending
                .remove(index)
                .expect("pending cache work item");
            guard.in_flight_bytes = guard
                .in_flight_bytes
                .saturating_add(item.estimated_decode_bytes);
            return Some(item);
        }
        guard = available
            .wait(guard)
            .expect("thumbnail queue mutex poisoned");
    }
}

fn finish_cache_work_item(
    queue: &Arc<(Mutex<CacheWorkQueue>, Condvar)>,
    estimated_decode_bytes: u64,
) {
    let (lock, available) = &**queue;
    let mut guard = lock.lock().expect("thumbnail queue mutex poisoned");
    guard.in_flight_bytes = guard.in_flight_bytes.saturating_sub(estimated_decode_bytes);
    available.notify_all();
}

fn cache_work_item_fits_budget(
    estimated_decode_bytes: u64,
    in_flight_bytes: u64,
    memory_budget: u64,
) -> bool {
    in_flight_bytes == 0 || in_flight_bytes.saturating_add(estimated_decode_bytes) <= memory_budget
}

fn cache_generation_memory_budget_bytes() -> u64 {
    env::var("OACURATOR_CACHE_MEMORY_BUDGET_MB")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .and_then(|megabytes| megabytes.checked_mul(1024 * 1024))
        .filter(|value| *value > 0)
        .unwrap_or(CACHE_GENERATION_MEMORY_BUDGET_BYTES)
}

pub fn estimate_cache_decode_bytes(source: &Path) -> Result<u64> {
    let (width, height) = image::image_dimensions(source)?;
    Ok(u64::from(width)
        .saturating_mul(u64::from(height))
        .saturating_mul(8))
}

pub fn thumbnail_cache_worker_count(total_jobs: usize, provider_env_var: &str) -> usize {
    if total_jobs == 0 {
        return 0;
    }
    let configured = env::var(provider_env_var)
        .ok()
        .or_else(|| env::var("OACURATOR_IMPORT_CACHE_WORKERS").ok())
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0);
    let default_workers = thread::available_parallelism()
        .map(|value| value.get())
        .unwrap_or(2)
        .clamp(1, 4);
    configured.unwrap_or(default_workers).min(total_jobs)
}

fn elapsed_ms(started: Instant) -> u128 {
    started.elapsed().as_millis()
}

fn combine_cache_profiles(
    mut left: CacheDerivativeProfile,
    right: CacheDerivativeProfile,
) -> CacheDerivativeProfile {
    left.total_ms += right.total_ms;
    left.open_ms += right.open_ms;
    left.thumbnail_resize_ms += right.thumbnail_resize_ms;
    left.thumbnail_write_ms += right.thumbnail_write_ms;
    left.thumbnail_db_ms += right.thumbnail_db_ms;
    left.preview_resize_ms += right.preview_resize_ms;
    left.preview_write_ms += right.preview_write_ms;
    left.preview_db_ms += right.preview_db_ms;
    left.thumbnail_bytes += right.thumbnail_bytes;
    left.preview_bytes += right.preview_bytes;
    left
}

fn cache_warning_for_error(
    file_asset_id: i64,
    path: &Path,
    error: AppError,
) -> ArtworkCacheWarning {
    ArtworkCacheWarning {
        file_asset_id,
        path: path.to_path_buf(),
        message: format!(
            "Preview could not be generated for {}. The source file was not changed. {}",
            path.file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("this file"),
            error
        ),
    }
}

fn decode_cache_source(source: &Path) -> Result<DynamicImage> {
    let mut reader = ImageReader::open(source)?;
    reader.limits(cache_decode_limits());
    Ok(reader.decode()?)
}

fn cache_decode_limits() -> Limits {
    let mut limits = Limits::default();
    limits.max_alloc = Some(CACHE_DECODE_MAX_ALLOC_BYTES);
    limits
}

enum CacheImage<'a> {
    Borrowed(&'a DynamicImage),
    Owned(DynamicImage),
}

impl CacheImage<'_> {
    fn as_image(&self) -> &DynamicImage {
        match self {
            Self::Borrowed(image) => image,
            Self::Owned(image) => image,
        }
    }

    #[cfg(test)]
    fn is_borrowed(&self) -> bool {
        matches!(self, Self::Borrowed(_))
    }
}

impl std::ops::Deref for CacheImage<'_> {
    type Target = DynamicImage;

    fn deref(&self) -> &Self::Target {
        self.as_image()
    }
}

fn cache_preview_image(image: &DynamicImage) -> CacheImage<'_> {
    if image.height() <= CACHE_PREVIEW_MAX_HEIGHT {
        CacheImage::Borrowed(image)
    } else {
        CacheImage::Owned(cache_resize_to_height(image, CACHE_PREVIEW_MAX_HEIGHT))
    }
}

fn cache_thumbnail_image(image: &DynamicImage) -> DynamicImage {
    cache_resize_to_fit(image, CACHE_THUMBNAIL_MAX_DIMENSION)
}

fn cache_resize_to_height(image: &DynamicImage, max_height: u32) -> DynamicImage {
    if image.height() <= max_height {
        return image.clone();
    }
    let ratio = f64::from(max_height) / f64::from(image.height());
    let width = (f64::from(image.width()) * ratio).round().max(1.0) as u32;
    image.resize_exact(width, max_height, CACHE_RESIZE_FILTER)
}

fn cache_resize_to_fit(image: &DynamicImage, max_dimension: u32) -> DynamicImage {
    if image.width() <= max_dimension && image.height() <= max_dimension {
        return image.clone();
    }
    image.resize(max_dimension, max_dimension, CACHE_RESIZE_FILTER)
}

#[derive(Debug, Clone, Copy)]
enum CacheImageFormat {
    Jpeg,
    Png,
}

impl CacheImageFormat {
    fn extension(self) -> &'static str {
        match self {
            Self::Jpeg => "jpg",
            Self::Png => "png",
        }
    }

    fn format(self) -> &'static str {
        match self {
            Self::Jpeg => "jpg",
            Self::Png => "png",
        }
    }
}

fn cache_format_for_image(image: &DynamicImage) -> CacheImageFormat {
    if image_contains_transparency(image) {
        CacheImageFormat::Png
    } else {
        CacheImageFormat::Jpeg
    }
}

fn write_cache_image(image: &DynamicImage, path: &Path, format: CacheImageFormat) -> Result<()> {
    match format {
        CacheImageFormat::Png => {
            image.save_with_format(path, image::ImageFormat::Png)?;
        }
        CacheImageFormat::Jpeg => {
            let file = File::create(path)?;
            let mut writer = BufWriter::new(file);
            let mut encoder = JpegEncoder::new_with_quality(&mut writer, 82);
            let rgb = image.to_rgb8();
            encoder.encode(
                rgb.as_raw(),
                rgb.width(),
                rgb.height(),
                image::ColorType::Rgb8.into(),
            )?;
        }
    }
    Ok(())
}

fn image_contains_transparency(image: &DynamicImage) -> bool {
    match image {
        DynamicImage::ImageLumaA8(buffer) => buffer.pixels().any(|pixel| pixel.0[1] < u8::MAX),
        DynamicImage::ImageRgba8(buffer) => buffer.pixels().any(|pixel| pixel.0[3] < u8::MAX),
        DynamicImage::ImageLumaA16(buffer) => buffer.pixels().any(|pixel| pixel.0[1] < u16::MAX),
        DynamicImage::ImageRgba16(buffer) => buffer.pixels().any(|pixel| pixel.0[3] < u16::MAX),
        DynamicImage::ImageRgba32F(buffer) => buffer.pixels().any(|pixel| pixel.0[3] < 1.0),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{FileAssetKnownMetadataInsert, FileAssetMetadata};
    use tempfile::TempDir;

    #[test]
    fn cache_decode_limits_allow_large_preservation_scan_allocations() {
        let limits = cache_decode_limits();

        assert!(limits
            .max_alloc
            .is_some_and(|max_alloc| max_alloc >= 768 * 1024 * 1024));
    }

    #[test]
    fn cache_work_budget_allows_oversized_item_only_when_running_alone() {
        assert!(cache_work_item_fits_budget(600, 0, 512));
        assert!(cache_work_item_fits_budget(128, 256, 512));
        assert!(!cache_work_item_fits_budget(300, 256, 512));
    }

    #[test]
    fn cache_preview_and_thumbnail_keep_expected_bounds() {
        let image = DynamicImage::ImageRgba8(image::RgbaImage::new(2000, 3000));

        let preview = cache_preview_image(&image);
        let thumbnail = cache_thumbnail_image(&preview);

        assert_eq!((preview.width(), preview.height()), (1333, 2000));
        assert_eq!((thumbnail.width(), thumbnail.height()), (213, 320));
    }

    #[test]
    fn cache_preview_uses_height_limit_without_width_limit() {
        let image = DynamicImage::ImageRgba8(image::RgbaImage::new(3000, 1800));

        let preview = cache_preview_image(&image);

        assert_eq!((preview.width(), preview.height()), (3000, 1800));
    }

    #[test]
    fn cache_preview_borrows_images_that_do_not_need_resizing() {
        let image = DynamicImage::ImageRgba8(image::RgbaImage::new(1200, 2000));

        let preview = cache_preview_image(&image);

        assert!(preview.is_borrowed());
    }

    #[test]
    fn deferred_cache_generation_skips_non_image_file_assets() {
        let dir = TempDir::new().unwrap();
        let catalog = Catalog::open(dir.path().join("oa-curator.sqlite3")).unwrap();
        catalog.init().unwrap();
        let gallery = catalog
            .create_gallery("Manual", &dir.path().join("Manual.oagallery"))
            .unwrap();
        let artwork = catalog
            .create_artwork_in_gallery(gallery.id, "Manual Artwork", None)
            .unwrap();
        let receipt_path = dir.path().join("receipt.pdf");
        fs::write(&receipt_path, b"%PDF-1.7\nsupporting document").unwrap();
        catalog
            .upsert_file_asset_with_known_metadata(
                artwork.id,
                FileAssetKnownMetadataInsert {
                    original_path: &receipt_path,
                    root: dir.path(),
                    path: &receipt_path,
                    is_primary: false,
                    source_kind: "copied",
                    metadata: FileAssetMetadata {
                        width: None,
                        height: None,
                        dpi_x: None,
                        dpi_y: None,
                    },
                },
            )
            .unwrap();

        let profile =
            ensure_artwork_cache_derivatives(&catalog, artwork.id, &dir.path().join("cache"))
                .unwrap();

        assert_eq!(profile.thumbnail_bytes, 0);
        assert_eq!(profile.preview_bytes, 0);
        assert!(catalog
            .artwork_detail(artwork.id)
            .unwrap()
            .derived_assets
            .is_empty());
    }

    #[test]
    fn local_file_indexing_rejects_webp_as_renderable_image() {
        let dir = TempDir::new().unwrap();
        let catalog = Catalog::open(dir.path().join("oa-curator.sqlite3")).unwrap();
        catalog.init().unwrap();
        let gallery = catalog
            .create_gallery("Manual", &dir.path().join("Manual.oagallery"))
            .unwrap();
        let artwork = catalog
            .create_artwork_in_gallery(gallery.id, "Manual Artwork", None)
            .unwrap();
        let webp_path = dir.path().join("scan.webp");
        fs::write(&webp_path, b"RIFF----WEBPunsupported").unwrap();

        let error = index_local_file_asset_with_role_profiled_options(
            &catalog,
            artwork.id,
            &webp_path,
            &dir.path().join("cache"),
            true,
            None,
            CacheDerivativeOptions::default(),
        )
        .unwrap_err();

        assert!(error.to_string().contains("Unsupported image type"));
        assert!(catalog
            .artwork_detail(artwork.id)
            .unwrap()
            .file_assets
            .is_empty());
    }
}
