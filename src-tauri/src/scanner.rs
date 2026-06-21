// Copyright (c) 2026 Remgrandt Works. All rights reserved.

use crate::catalog::{
    ArtworkCacheWarning, ArtworkDetail, AssetKind, Catalog, DerivedAssetInsert,
    DerivedAssetRenderInsert, FileAssetImageProbeInsert, FileAssetKnownMetadataInsert,
    FileAssetMetadata,
};
use crate::image_render::{
    estimate_decode_weight_bytes, preview_recipe, probe_source, render_image_to_file,
    thumbnail_recipe, ImageProbeResult, RenderLimits, RenderPurpose, RenderRequest, RenderedImage,
};
use crate::jobs::JobCancellation;
use crate::{AppError, Result};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::{Instant, UNIX_EPOCH};

const CACHE_DECODE_FALLBACK_ESTIMATE_BYTES: u64 = 2 * 1024 * 1024 * 1024;

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
    pub render_purpose: Option<RenderPurpose>,
    pub rendered: Option<RenderedImage>,
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
        let (file_asset_id, probe) = if is_supported_image(&current_path) {
            upsert_known_image_file_asset_with_probe(
                catalog,
                artwork_id,
                path,
                root,
                &current_path,
                is_primary,
                source_kind,
            )?
        } else {
            let file_asset_id = catalog.upsert_file_asset_with_known_metadata(
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
            )?;
            (file_asset_id, None)
        };
        if probe
            .as_ref()
            .is_some_and(ImageProbeResult::should_attempt_render)
        {
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
    let probe = probe_source(request.path);
    profile.dimensions_check_ms = elapsed_ms(dimensions_started);
    let root = request.path.parent().unwrap_or_else(|| Path::new(""));
    let upsert_started = Instant::now();
    let file_asset_id = upsert_known_image_file_asset_from_probe(
        catalog,
        artwork_id,
        FileAssetKnownMetadataInsert {
            original_path: request.path,
            root,
            path: request.path,
            is_primary: request.is_primary,
            source_kind: request.source_kind,
            metadata: probe.metadata_for_catalog(),
        },
        &probe,
    )?;
    profile.file_asset_upsert_ms = elapsed_ms(upsert_started);
    if probe.should_attempt_render() {
        profile.cache_derivatives = generate_cached_derivatives_profiled(
            catalog,
            artwork_id,
            file_asset_id,
            request.path,
            request.cache_dir,
            request.cache_options,
        )
        .unwrap_or_default();
    }
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

fn upsert_known_image_file_asset_with_probe(
    catalog: &Catalog,
    artwork_id: i64,
    original_path: &Path,
    root: &Path,
    path: &Path,
    is_primary: bool,
    source_kind: &str,
) -> Result<(i64, Option<ImageProbeResult>)> {
    let probe = probe_source(path);
    let file_asset_id = upsert_known_image_file_asset_from_probe(
        catalog,
        artwork_id,
        FileAssetKnownMetadataInsert {
            original_path,
            root,
            path,
            is_primary,
            source_kind,
            metadata: probe.metadata_for_catalog(),
        },
        &probe,
    )?;
    Ok((file_asset_id, Some(probe)))
}

fn upsert_known_image_file_asset_from_probe(
    catalog: &Catalog,
    artwork_id: i64,
    insert: FileAssetKnownMetadataInsert<'_>,
    probe: &ImageProbeResult,
) -> Result<i64> {
    let file_asset_id = catalog.upsert_file_asset_with_known_metadata(artwork_id, insert)?;
    catalog.upsert_file_asset_image_probe(file_asset_probe_insert(file_asset_id, probe))?;
    Ok(file_asset_id)
}

fn file_asset_probe_insert(
    file_asset_id: i64,
    probe: &ImageProbeResult,
) -> FileAssetImageProbeInsert {
    FileAssetImageProbeInsert {
        file_asset_id,
        probe_status: probe.probe_status.as_str().to_string(),
        render_status: probe.render_status.as_str().to_string(),
        width: probe.width,
        height: probe.height,
        dpi_x: probe.dpi_x,
        dpi_y: probe.dpi_y,
        container_format: probe.container_format.clone(),
        detected_mime: probe.detected_mime.clone(),
        compression: probe.compression.clone(),
        photometric: probe.photometric.clone(),
        bits_per_sample: probe.bits_per_sample,
        samples_per_pixel: probe.samples_per_pixel,
        has_alpha: probe.has_alpha,
        preferred_renderer: probe.preferred_renderer.clone(),
        renderer_version: probe.renderer_version.clone(),
        error_code: probe.error_code.clone(),
        error_message: probe.error_message.clone(),
    }
}

pub fn is_supported_image(path: &Path) -> bool {
    has_known_image_extension(path)
}

fn is_supported_image_extension(path: &Path) -> bool {
    has_known_image_extension(path)
}

fn has_known_image_extension(path: &Path) -> bool {
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

    let mut thumbnail_write_ms = 0;
    let mut thumbnail_bytes = 0;
    if create_thumbnail {
        let thumbnail_write_started = Instant::now();
        let derivative = render_cache_derivative(
            source,
            &folder,
            "thumbnail",
            RenderPurpose::Thumbnail,
            thumbnail_recipe(),
        )?;
        thumbnail_write_ms = elapsed_ms(thumbnail_write_started);
        thumbnail_bytes = derivative
            .rendered
            .as_ref()
            .map(|rendered| rendered.bytes)
            .unwrap_or(0);
        derivatives.push(derivative);
    }

    let mut preview_bytes = 0;
    let mut preview_write_ms = 0;
    if create_preview {
        let preview_write_started = Instant::now();
        let derivative = render_cache_derivative(
            source,
            &folder,
            "preview",
            RenderPurpose::Preview,
            preview_recipe(),
        )?;
        preview_write_ms = elapsed_ms(preview_write_started);
        preview_bytes = derivative
            .rendered
            .as_ref()
            .map(|rendered| rendered.bytes)
            .unwrap_or(0);
        derivatives.push(derivative);
    }

    Ok(GeneratedCacheDerivatives {
        artwork_id,
        file_asset_id,
        derivatives,
        profile: CacheDerivativeProfile {
            total_ms: elapsed_ms(total_started),
            open_ms: 0,
            thumbnail_resize_ms: 0,
            thumbnail_write_ms,
            thumbnail_db_ms: 0,
            preview_resize_ms: 0,
            preview_write_ms,
            preview_db_ms: 0,
            thumbnail_bytes,
            preview_bytes,
        },
    })
}

fn render_cache_derivative(
    source: &Path,
    folder: &Path,
    derivative_type: &str,
    purpose: RenderPurpose,
    recipe: crate::image_render::RenderRecipe,
) -> Result<GeneratedCacheDerivative> {
    let temporary_path = folder.join(format!("{derivative_type}.rendering"));
    let mut rendered = render_image_to_file(RenderRequest {
        source_path: source.to_path_buf(),
        destination_path: temporary_path,
        purpose,
        recipe,
        limits: RenderLimits::default(),
    })?;
    let final_path = folder.join(format!("{derivative_type}.{}", rendered.format));
    if final_path != rendered.path {
        if final_path.exists() {
            fs::remove_file(&final_path)?;
        }
        fs::rename(&rendered.path, &final_path)?;
        rendered.path = final_path;
    }
    Ok(GeneratedCacheDerivative {
        derivative_type: derivative_type.to_string(),
        format: rendered.format.clone(),
        path: rendered.path.clone(),
        width: i64::from(rendered.width),
        height: i64::from(rendered.height),
        render_purpose: Some(purpose),
        rendered: Some(rendered),
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
        let metadata = crate::image_metadata::read_image_metadata(&path)?;
        return Ok(Some(GeneratedCacheDerivative {
            derivative_type: derivative_type.to_string(),
            format: extension.to_string(),
            path,
            width: metadata.width,
            height: metadata.height,
            render_purpose: None,
            rendered: None,
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
        let asset = if rewrite_artwork_manifest {
            catalog.add_derived_asset(generated.artwork_id, insert)?
        } else {
            catalog.add_derived_asset_session_only(generated.artwork_id, insert)?
        };
        if let (Some(purpose), Some(rendered)) =
            (derivative.render_purpose, derivative.rendered.as_ref())
        {
            catalog
                .add_derived_asset_render(render_metadata_insert(asset.id, purpose, rendered))?;
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

fn render_metadata_insert(
    derived_asset_id: i64,
    purpose: RenderPurpose,
    rendered: &RenderedImage,
) -> DerivedAssetRenderInsert {
    DerivedAssetRenderInsert {
        derived_asset_id,
        purpose: purpose.as_str().to_string(),
        recipe_key: rendered.recipe_key.clone(),
        recipe_json: rendered.recipe_json.clone(),
        source_path: rendered.source_fingerprint.path.clone(),
        source_size_bytes: rendered.source_fingerprint.size_bytes,
        source_modified_at: rendered.source_fingerprint.modified_at.clone(),
        source_width: i64::from(rendered.source_fingerprint.width),
        source_height: i64::from(rendered.source_fingerprint.height),
        output_width: i64::from(rendered.width),
        output_height: i64::from(rendered.height),
        output_size_bytes: rendered.bytes,
        renderer: rendered.renderer.clone(),
        renderer_version: rendered.renderer_version.clone(),
        renderer_options_json: rendered.renderer_options_json.clone(),
    }
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
            let estimated_decode_bytes = estimate_decode_weight_bytes(&source_path)
                .unwrap_or(CACHE_DECODE_FALLBACK_ESTIMATE_BYTES);
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
            let estimated_decode_bytes = estimate_decode_weight_bytes(&source_path)
                .unwrap_or(CACHE_DECODE_FALLBACK_ESTIMATE_BYTES);
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
    let queue = Arc::new(Mutex::new(VecDeque::from(cache_work_items)));
    let cache_dir = cache_dir.to_path_buf();
    let (sender, receiver) = mpsc::channel();

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
                let item = next_cache_work_item(&queue);
                let Some(item) = item else {
                    break;
                };
                if cancellation
                    .as_ref()
                    .is_some_and(JobCancellation::is_canceled)
                {
                    break;
                }
                let result = build_cached_derivatives_profiled(
                    item.artwork_id,
                    item.file_asset_id,
                    &item.source_path,
                    &cache_dir,
                    options,
                )
                .map_err(|error| error.to_string());
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

fn next_cache_work_item(
    queue: &Arc<Mutex<VecDeque<ThumbnailCacheWorkItem>>>,
) -> Option<ThumbnailCacheWorkItem> {
    queue
        .lock()
        .expect("thumbnail queue mutex poisoned")
        .pop_front()
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{FileAssetKnownMetadataInsert, FileAssetMetadata};
    use tempfile::TempDir;

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

    #[test]
    fn attach_accepts_known_image_extension_when_image_probe_fails() {
        let dir = TempDir::new().unwrap();
        let catalog = Catalog::open(dir.path().join("oa-curator.sqlite3")).unwrap();
        catalog.init().unwrap();
        let gallery = catalog
            .create_gallery("Manual", &dir.path().join("Manual.oagallery"))
            .unwrap();
        let artwork = catalog
            .create_artwork_in_gallery(gallery.id, "Legacy TIFF", None)
            .unwrap();
        let tiff_path = dir.path().join("legacy-ycbcr-jpeg.tif");
        fs::write(&tiff_path, b"II*\0legacy jpeg-in-tiff fixture placeholder").unwrap();

        let detail = attach_files_to_artwork(
            &catalog,
            artwork.id,
            std::slice::from_ref(&tiff_path),
            &dir.path().join("cache"),
            AttachMode::Link,
        )
        .unwrap();

        assert_eq!(detail.file_assets.len(), 1);
        assert_eq!(detail.file_assets[0].current_path, tiff_path);
        assert_eq!(detail.file_assets[0].width, None);
        assert_eq!(detail.file_assets[0].height, None);
        assert!(
            detail
                .cache_warnings
                .iter()
                .any(|warning| warning.message.contains("The source file was not changed")),
            "probe/render failures should be recorded as warnings, not attachment blockers"
        );
    }
}
