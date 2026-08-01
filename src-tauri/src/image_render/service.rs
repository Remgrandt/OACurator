use crate::image_metadata::read_image_metadata;
use crate::image_render::backend::{self, BackendRenderResult};
use crate::image_render::error::RenderError;
use crate::image_render::recipe::{
    AlphaPolicy, ColorPolicy, OutputFormat, RenderRecipe, ResizeFilter, ResizeMode,
};
use crate::image_render::scheduler::{compile_render_plan, global_render_scheduler, RenderLimits};
use crate::Result;
use chrono::{DateTime, Utc};
use image::{Rgba, RgbaImage};
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::BufReader;
use std::path::PathBuf;
use tiff::decoder::{ChunkType, Decoder, DecodingResult};
use tiff::ColorType;

const OVERSIZED_TIFF_PROXY_MAX_DIMENSION: u32 = 2048;
const PIXEL_STEP_TIFF_SOURCE_BYTES: u64 = 128 * 1024 * 1024;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum RenderPurpose {
    Thumbnail,
    Preview,
    ExportBasicPng,
    ExportPremiumPng,
    FutureWebDerivative,
    RaremarqUploadImage,
}

impl RenderPurpose {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Thumbnail => "thumbnail",
            Self::Preview => "preview",
            Self::ExportBasicPng => "export_basic_png",
            Self::ExportPremiumPng => "export_premium_png",
            Self::FutureWebDerivative => "future_web_derivative",
            Self::RaremarqUploadImage => "raremarq_upload_image",
        }
    }
}

#[derive(Debug, Clone)]
pub struct RenderRequest {
    pub source_path: PathBuf,
    pub destination_path: PathBuf,
    pub purpose: RenderPurpose,
    pub recipe: RenderRecipe,
    pub limits: RenderLimits,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceFingerprint {
    pub path: PathBuf,
    pub size_bytes: u64,
    pub modified_at: Option<String>,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderedImage {
    pub path: PathBuf,
    pub width: u32,
    pub height: u32,
    pub format: String,
    pub bytes: u64,
    pub source_fingerprint: SourceFingerprint,
    pub recipe_key: String,
    pub recipe_json: String,
    pub renderer: String,
    pub renderer_version: String,
    pub renderer_options_json: String,
}

pub fn render_image_to_file(request: RenderRequest) -> Result<RenderedImage> {
    let rendered = render_image_to_file_inner(request)?;
    Ok(rendered)
}

fn render_image_to_file_inner(
    request: RenderRequest,
) -> std::result::Result<RenderedImage, RenderError> {
    if !request.source_path.is_file() {
        return Err(RenderError::SourceMissing {
            path: request.source_path,
        });
    }
    if let Some(parent) = request.destination_path.parent() {
        fs::create_dir_all(parent).map_err(|error| RenderError::EncodeFailed {
            path: request.destination_path.clone(),
            detail: error.to_string(),
        })?;
    }

    let source_metadata =
        read_image_metadata(&request.source_path).map_err(|error| RenderError::DecodeFailed {
            path: request.source_path.clone(),
            detail: error.to_string(),
        })?;
    let source_width = source_metadata.width as u32;
    let source_height = source_metadata.height as u32;
    let fingerprint = source_fingerprint(&request, source_width, source_height)?;
    if should_use_oversized_tiff_proxy(
        &request,
        source_width,
        source_height,
        fingerprint.size_bytes,
    ) {
        return render_oversized_tiff_derivative(request, fingerprint);
    }
    let plan = compile_render_plan(&request, source_width, source_height)?;
    let recipe_json = serde_json::to_string(&request.recipe).map_err(|error| {
        RenderError::VerificationFailed {
            path: request.destination_path.clone(),
            detail: error.to_string(),
        }
    })?;
    let recipe_key = recipe_key(request.purpose, &recipe_json);

    let rendered =
        global_render_scheduler().with_permit(&plan, || backend::render(&request, &plan))?;
    verify_output(&request, rendered, fingerprint, recipe_key, recipe_json)
}

fn should_use_oversized_tiff_proxy(
    request: &RenderRequest,
    source_width: u32,
    source_height: u32,
    source_size_bytes: u64,
) -> bool {
    matches!(
        request.purpose,
        RenderPurpose::Thumbnail | RenderPurpose::Preview
    ) && request
        .source_path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| matches!(extension.to_ascii_lowercase().as_str(), "tif" | "tiff"))
        && (u64::from(source_width) * u64::from(source_height) > request.limits.max_source_pixels
            || source_size_bytes > PIXEL_STEP_TIFF_SOURCE_BYTES)
}

fn render_oversized_tiff_derivative(
    request: RenderRequest,
    source_fingerprint: SourceFingerprint,
) -> std::result::Result<RenderedImage, RenderError> {
    let parent = request
        .destination_path
        .parent()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    let fingerprint_json = serde_json::to_string(&source_fingerprint).map_err(|error| {
        RenderError::VerificationFailed {
            path: request.source_path.clone(),
            detail: error.to_string(),
        }
    })?;
    let proxy_name = format!(
        "oversized-source-{:016x}.png",
        fnv1a64(fingerprint_json.as_bytes())
    );
    let proxy_path = parent.join(proxy_name);
    let proxy_renderer = if valid_oversized_tiff_proxy(&proxy_path) {
        "cached-proxy".to_string()
    } else {
        render_oversized_tiff_proxy(&request, &proxy_path)?
    };

    let proxy_request = RenderRequest {
        source_path: proxy_path,
        destination_path: request.destination_path.clone(),
        purpose: request.purpose,
        recipe: request.recipe.clone(),
        limits: request.limits,
    };
    let mut rendered = render_image_to_file_inner(proxy_request)?;
    rendered.source_fingerprint = source_fingerprint;
    rendered.renderer = format!(
        "oversized-tiff-proxy+{proxy_renderer}+{}",
        rendered.renderer
    );
    let inner_options = serde_json::from_str::<serde_json::Value>(&rendered.renderer_options_json)
        .unwrap_or_else(|_| serde_json::Value::String(rendered.renderer_options_json.clone()));
    rendered.renderer_options_json = serde_json::json!({
        "proxy_max_dimension": OVERSIZED_TIFF_PROXY_MAX_DIMENSION,
        "proxy_renderer": proxy_renderer,
        "inner": inner_options
    })
    .to_string();
    Ok(rendered)
}

fn valid_oversized_tiff_proxy(path: &std::path::Path) -> bool {
    path.is_file()
        && read_image_metadata(path).is_ok_and(|metadata| {
            metadata.width <= i64::from(OVERSIZED_TIFF_PROXY_MAX_DIMENSION)
                && metadata.height <= i64::from(OVERSIZED_TIFF_PROXY_MAX_DIMENSION)
        })
}

fn render_oversized_tiff_proxy(
    request: &RenderRequest,
    proxy_path: &std::path::Path,
) -> std::result::Result<String, RenderError> {
    let temporary_path = proxy_path.with_extension("rendering.png");
    if temporary_path.is_file() {
        fs::remove_file(&temporary_path).map_err(|error| RenderError::EncodeFailed {
            path: temporary_path.clone(),
            detail: error.to_string(),
        })?;
    }
    let proxy_request = RenderRequest {
        source_path: request.source_path.clone(),
        destination_path: temporary_path.clone(),
        purpose: request.purpose,
        recipe: oversized_tiff_proxy_recipe(),
        limits: RenderLimits {
            max_source_pixels: u64::MAX,
            ..request.limits
        },
    };
    let source_metadata = read_image_metadata(&proxy_request.source_path).map_err(|error| {
        RenderError::DecodeFailed {
            path: proxy_request.source_path.clone(),
            detail: error.to_string(),
        }
    })?;
    let plan = compile_render_plan(
        &proxy_request,
        source_metadata.width as u32,
        source_metadata.height as u32,
    )?;
    global_render_scheduler().with_permit(&plan, || {
        render_tiff_pixel_step_proxy(
            &proxy_request.source_path,
            &temporary_path,
            plan.target_width,
            plan.target_height,
        )
    })?;
    if proxy_path.is_file() {
        fs::remove_file(proxy_path).map_err(|error| RenderError::EncodeFailed {
            path: proxy_path.to_path_buf(),
            detail: error.to_string(),
        })?;
    }
    fs::rename(&temporary_path, proxy_path).map_err(|error| RenderError::EncodeFailed {
        path: proxy_path.to_path_buf(),
        detail: error.to_string(),
    })?;
    Ok("tiff-pixel-step".to_string())
}

fn render_tiff_pixel_step_proxy(
    source: &std::path::Path,
    destination: &std::path::Path,
    output_width: u32,
    output_height: u32,
) -> std::result::Result<(), RenderError> {
    let file = File::open(source).map_err(|error| RenderError::DecodeFailed {
        path: source.to_path_buf(),
        detail: error.to_string(),
    })?;
    let mut decoder =
        Decoder::new(BufReader::new(file)).map_err(|error| RenderError::DecodeFailed {
            path: source.to_path_buf(),
            detail: error.to_string(),
        })?;
    let (source_width, source_height) =
        decoder
            .dimensions()
            .map_err(|error| RenderError::DecodeFailed {
                path: source.to_path_buf(),
                detail: error.to_string(),
            })?;
    let color_type = decoder
        .colortype()
        .map_err(|error| RenderError::DecodeFailed {
            path: source.to_path_buf(),
            detail: error.to_string(),
        })?;
    let channels = tiff_channel_count(color_type).ok_or_else(|| RenderError::DecodeFailed {
        path: source.to_path_buf(),
        detail: format!("pixel-step TIFF proxy does not support {color_type:?}"),
    })?;
    let (chunk_width, chunk_height) = decoder.chunk_dimensions();
    let chunks_across = source_width.div_ceil(chunk_width);
    let chunks_down = source_height.div_ceil(chunk_height);
    let chunk_count = match decoder.get_chunk_type() {
        ChunkType::Strip => decoder.strip_count(),
        ChunkType::Tile => decoder.tile_count(),
    }
    .map_err(|error| RenderError::DecodeFailed {
        path: source.to_path_buf(),
        detail: error.to_string(),
    })?;
    if chunk_count != chunks_across * chunks_down {
        return Err(RenderError::DecodeFailed {
            path: source.to_path_buf(),
            detail: "pixel-step TIFF proxy does not support planar image data".to_string(),
        });
    }

    let sample_x = pixel_step_positions(source_width, output_width);
    let sample_y = pixel_step_positions(source_height, output_height);
    let mut output = RgbaImage::new(output_width, output_height);
    for chunk_index in 0..chunk_count {
        let chunk_x = chunk_index % chunks_across;
        let chunk_y = chunk_index / chunks_across;
        let source_x = chunk_x * chunk_width;
        let source_y = chunk_y * chunk_height;
        let (data_width, data_height) = decoder.chunk_data_dimensions(chunk_index);
        let output_x_start = sample_x.partition_point(|&value| value < source_x);
        let output_x_end = sample_x.partition_point(|&value| value < source_x + data_width);
        let output_y_start = sample_y.partition_point(|&value| value < source_y);
        let output_y_end = sample_y.partition_point(|&value| value < source_y + data_height);
        if output_x_start == output_x_end || output_y_start == output_y_end {
            continue;
        }
        let chunk = decoder
            .read_chunk(chunk_index)
            .map_err(|error| RenderError::DecodeFailed {
                path: source.to_path_buf(),
                detail: error.to_string(),
            })?;
        for (output_y, &sampled_y) in sample_y
            .iter()
            .enumerate()
            .take(output_y_end)
            .skip(output_y_start)
        {
            let chunk_pixel_y = sampled_y - source_y;
            for (output_x, &sampled_x) in sample_x
                .iter()
                .enumerate()
                .take(output_x_end)
                .skip(output_x_start)
            {
                let chunk_pixel_x = sampled_x - source_x;
                let sample_index = ((chunk_pixel_y * data_width + chunk_pixel_x) as usize)
                    .checked_mul(channels)
                    .ok_or_else(|| RenderError::DecodeFailed {
                        path: source.to_path_buf(),
                        detail: "TIFF sample index overflow".to_string(),
                    })?;
                let pixel = tiff_pixel(&chunk, color_type, sample_index).ok_or_else(|| {
                    RenderError::DecodeFailed {
                        path: source.to_path_buf(),
                        detail: "TIFF chunk did not contain the expected pixel data".to_string(),
                    }
                })?;
                output.put_pixel(output_x as u32, output_y as u32, Rgba(pixel));
            }
        }
    }
    output
        .save_with_format(destination, image::ImageFormat::Png)
        .map_err(|error| RenderError::EncodeFailed {
            path: destination.to_path_buf(),
            detail: error.to_string(),
        })
}

fn pixel_step_positions(source_size: u32, output_size: u32) -> Vec<u32> {
    (0..output_size)
        .map(|index| {
            (((u64::from(index) * 2 + 1) * u64::from(source_size)) / (u64::from(output_size) * 2))
                .min(u64::from(source_size - 1)) as u32
        })
        .collect()
}

fn tiff_channel_count(color_type: ColorType) -> Option<usize> {
    match color_type {
        ColorType::Gray(8 | 16) => Some(1),
        ColorType::GrayA(8 | 16) => Some(2),
        ColorType::RGB(8 | 16) => Some(3),
        ColorType::RGBA(8 | 16) | ColorType::CMYK(8 | 16) => Some(4),
        ColorType::CMYKA(8 | 16) => Some(5),
        _ => None,
    }
}

fn tiff_pixel(
    chunk: &DecodingResult,
    color_type: ColorType,
    sample_index: usize,
) -> Option<[u8; 4]> {
    let sample = |offset| tiff_sample(chunk, sample_index + offset);
    match color_type {
        ColorType::Gray(8 | 16) => {
            let gray = sample(0)?;
            Some([gray, gray, gray, 255])
        }
        ColorType::GrayA(8 | 16) => {
            let gray = sample(0)?;
            Some([gray, gray, gray, sample(1)?])
        }
        ColorType::RGB(8 | 16) => Some([sample(0)?, sample(1)?, sample(2)?, 255]),
        ColorType::RGBA(8 | 16) => Some([sample(0)?, sample(1)?, sample(2)?, sample(3)?]),
        ColorType::CMYK(8 | 16) | ColorType::CMYKA(8 | 16) => {
            let cyan = u16::from(sample(0)?);
            let magenta = u16::from(sample(1)?);
            let yellow = u16::from(sample(2)?);
            let black = u16::from(sample(3)?);
            Some([
                255 - (cyan + black).min(255) as u8,
                255 - (magenta + black).min(255) as u8,
                255 - (yellow + black).min(255) as u8,
                if matches!(color_type, ColorType::CMYKA(_)) {
                    sample(4)?
                } else {
                    255
                },
            ])
        }
        _ => None,
    }
}

fn tiff_sample(chunk: &DecodingResult, index: usize) -> Option<u8> {
    match chunk {
        DecodingResult::U8(values) => values.get(index).copied(),
        DecodingResult::U16(values) => values.get(index).map(|value| (value >> 8) as u8),
        _ => None,
    }
}

fn oversized_tiff_proxy_recipe() -> RenderRecipe {
    RenderRecipe {
        version: 1,
        resize: ResizeMode::FitWithin {
            max_width: Some(OVERSIZED_TIFF_PROXY_MAX_DIMENSION),
            max_height: Some(OVERSIZED_TIFF_PROXY_MAX_DIMENSION),
        },
        output: OutputFormat::Png,
        color: ColorPolicy::ConvertToSrgb8,
        alpha: AlphaPolicy::Preserve,
        filter: ResizeFilter::Lanczos3,
        allow_upscale: false,
    }
}

fn source_fingerprint(
    request: &RenderRequest,
    width: u32,
    height: u32,
) -> std::result::Result<SourceFingerprint, RenderError> {
    let metadata =
        fs::metadata(&request.source_path).map_err(|error| RenderError::DecodeFailed {
            path: request.source_path.clone(),
            detail: error.to_string(),
        })?;
    let path =
        fs::canonicalize(&request.source_path).unwrap_or_else(|_| request.source_path.clone());
    let modified_at = metadata
        .modified()
        .ok()
        .map(|time| DateTime::<Utc>::from(time).to_rfc3339());
    Ok(SourceFingerprint {
        path,
        size_bytes: metadata.len(),
        modified_at,
        width,
        height,
    })
}

fn verify_output(
    request: &RenderRequest,
    backend_result: BackendRenderResult,
    source_fingerprint: SourceFingerprint,
    recipe_key: String,
    recipe_json: String,
) -> std::result::Result<RenderedImage, RenderError> {
    let metadata = fs::metadata(&request.destination_path).map_err(|error| {
        RenderError::VerificationFailed {
            path: request.destination_path.clone(),
            detail: error.to_string(),
        }
    })?;
    if metadata.len() == 0 {
        return Err(RenderError::VerificationFailed {
            path: request.destination_path.clone(),
            detail: "renderer created an empty file".to_string(),
        });
    }
    Ok(RenderedImage {
        path: request.destination_path.clone(),
        width: backend_result.width,
        height: backend_result.height,
        format: backend_result.format,
        bytes: metadata.len(),
        source_fingerprint,
        recipe_key,
        recipe_json,
        renderer: backend_result.renderer,
        renderer_version: backend_result.renderer_version,
        renderer_options_json: backend_result.renderer_options_json,
    })
}

fn recipe_key(purpose: RenderPurpose, recipe_json: &str) -> String {
    format!(
        "{}:{:016x}",
        purpose.as_str(),
        fnv1a64(recipe_json.as_bytes())
    )
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}
