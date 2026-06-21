// Copyright (c) 2026 Remgrandt Works. All rights reserved.

use crate::image_render::backend::BackendRenderResult;
use crate::image_render::error::RenderError;
use crate::image_render::recipe::{
    AlphaPolicy, ColorPolicy, OutputFormat, RenderRecipe, ResizeFilter,
};
use crate::image_render::scheduler::RenderPlan;
use crate::image_render::service::RenderRequest;
use image::codecs::jpeg::JpegEncoder;
use image::imageops::FilterType;
use image::{DynamicImage, ImageReader, Limits};
use std::fs::File;
use std::io::BufWriter;
use std::path::Path;

#[derive(Debug, Clone, Copy)]
pub enum ImageRsMode {
    SmallFileFastPath,
    DebugFallback,
}

impl ImageRsMode {
    fn renderer(self) -> &'static str {
        match self {
            Self::SmallFileFastPath => "image-rs-capped",
            Self::DebugFallback => "image-rs-debug-fallback",
        }
    }

    fn reason(self) -> &'static str {
        match self {
            Self::SmallFileFastPath => "small_jpeg_png_fast_path",
            Self::DebugFallback => "renderer_unavailable_debug_fallback",
        }
    }
}

pub fn render(
    request: &RenderRequest,
    plan: &RenderPlan,
    mode: ImageRsMode,
) -> std::result::Result<BackendRenderResult, RenderError> {
    let mut reader =
        ImageReader::open(&request.source_path).map_err(|error| RenderError::DecodeFailed {
            path: request.source_path.clone(),
            detail: error.to_string(),
        })?;
    let mut limits = Limits::default();
    limits.max_alloc = Some(request.limits.image_rs_max_alloc_bytes);
    reader.limits(limits);
    let image = reader.decode().map_err(|error| RenderError::DecodeFailed {
        path: request.source_path.clone(),
        detail: error.to_string(),
    })?;
    let resized = resize_to_plan(image, request.recipe.filter, plan);
    let (prepared, output_format) = prepare_output(resized, &request.recipe);
    write_image(&prepared, &request.destination_path, &output_format)?;

    Ok(BackendRenderResult {
        width: prepared.width(),
        height: prepared.height(),
        format: output_format.format_name().to_string(),
        renderer: mode.renderer().to_string(),
        renderer_version: env!("CARGO_PKG_VERSION").to_string(),
        renderer_options_json: serde_json::json!({
            "max_alloc_bytes": request.limits.image_rs_max_alloc_bytes,
            "mode": mode.reason()
        })
        .to_string(),
    })
}

fn resize_to_plan(image: DynamicImage, filter: ResizeFilter, plan: &RenderPlan) -> DynamicImage {
    if image.width() == plan.target_width && image.height() == plan.target_height {
        return image;
    }
    image.resize_exact(
        plan.target_width,
        plan.target_height,
        image_filter_to_filter_type(filter),
    )
}

fn image_filter_to_filter_type(filter: ResizeFilter) -> FilterType {
    match filter {
        ResizeFilter::Nearest => FilterType::Nearest,
        ResizeFilter::Triangle => FilterType::Triangle,
        ResizeFilter::Lanczos3 => FilterType::Lanczos3,
    }
}

fn prepare_output(image: DynamicImage, recipe: &RenderRecipe) -> (DynamicImage, OutputFormat) {
    let has_transparency = image_contains_transparency(&image);
    let output_format = match (&recipe.output, &recipe.alpha) {
        (OutputFormat::Jpeg { .. }, AlphaPolicy::Preserve) if has_transparency => OutputFormat::Png,
        (format, _) => format.clone(),
    };
    let image = match recipe.alpha {
        AlphaPolicy::Preserve => image,
        AlphaPolicy::FlattenOnBackground { r, g, b } => flatten_on_background(&image, r, g, b),
    };
    let image = match recipe.color {
        ColorPolicy::Preserve => image,
        ColorPolicy::ConvertToSrgb8 => match output_format {
            OutputFormat::Png if image_contains_transparency(&image) => {
                DynamicImage::ImageRgba8(image.to_rgba8())
            }
            OutputFormat::Png => DynamicImage::ImageRgb8(image.to_rgb8()),
            OutputFormat::Jpeg { .. } => DynamicImage::ImageRgb8(image.to_rgb8()),
        },
    };
    (image, output_format)
}

fn flatten_on_background(image: &DynamicImage, r: u8, g: u8, b: u8) -> DynamicImage {
    let rgba = image.to_rgba8();
    let mut rgb = image::RgbImage::new(rgba.width(), rgba.height());
    for (x, y, pixel) in rgba.enumerate_pixels() {
        let alpha = f32::from(pixel.0[3]) / 255.0;
        let inv_alpha = 1.0 - alpha;
        let blended = image::Rgb([
            (f32::from(pixel.0[0]) * alpha + f32::from(r) * inv_alpha).round() as u8,
            (f32::from(pixel.0[1]) * alpha + f32::from(g) * inv_alpha).round() as u8,
            (f32::from(pixel.0[2]) * alpha + f32::from(b) * inv_alpha).round() as u8,
        ]);
        rgb.put_pixel(x, y, blended);
    }
    DynamicImage::ImageRgb8(rgb)
}

fn write_image(
    image: &DynamicImage,
    path: &Path,
    output_format: &OutputFormat,
) -> std::result::Result<(), RenderError> {
    match output_format {
        OutputFormat::Png => image
            .save_with_format(path, image::ImageFormat::Png)
            .map_err(|error| RenderError::EncodeFailed {
                path: path.to_path_buf(),
                detail: error.to_string(),
            }),
        OutputFormat::Jpeg { quality } => {
            let file = File::create(path).map_err(|error| RenderError::EncodeFailed {
                path: path.to_path_buf(),
                detail: error.to_string(),
            })?;
            let mut writer = BufWriter::new(file);
            let mut encoder = JpegEncoder::new_with_quality(&mut writer, *quality);
            let rgb = image.to_rgb8();
            encoder
                .encode(
                    rgb.as_raw(),
                    rgb.width(),
                    rgb.height(),
                    image::ColorType::Rgb8.into(),
                )
                .map_err(|error| RenderError::EncodeFailed {
                    path: path.to_path_buf(),
                    detail: error.to_string(),
                })
        }
    }
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
