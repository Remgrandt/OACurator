// Copyright (c) 2026 Remgrandt Works. All rights reserved.

use crate::image_render::error::RenderError;
use crate::image_render::scheduler::RenderPlan;
use crate::image_render::service::RenderRequest;
use std::env;

pub mod image_rs;
mod old_jpeg_tiff;
pub mod vips;
#[cfg(any(target_os = "windows", target_os = "macos"))]
pub mod vips_linked;

const DEFAULT_IMAGE_RS_FAST_PATH_MAX_BYTES: u64 = 128 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct BackendRenderResult {
    pub width: u32,
    pub height: u32,
    pub format: String,
    pub renderer: String,
    pub renderer_version: String,
    pub renderer_options_json: String,
}

pub fn render(
    request: &RenderRequest,
    plan: &RenderPlan,
) -> std::result::Result<BackendRenderResult, RenderError> {
    if image_rs_fast_path_allowed(request, plan) {
        match image_rs::render(request, plan, image_rs::ImageRsMode::SmallFileFastPath) {
            Ok(result) => return Ok(result),
            Err(RenderError::DecodeFailed { .. }) | Err(RenderError::EncodeFailed { .. }) => {}
            Err(error) => return Err(error),
        }
    }

    match render_with_primary_vips_backend(request, plan) {
        Ok(result) => Ok(result),
        Err(RenderError::RendererUnavailable { .. }) if image_rs_fallback_allowed() => {
            image_rs::render(request, plan, image_rs::ImageRsMode::DebugFallback)
        }
        Err(error) => render_old_jpeg_tiff_payload_fallback(request, plan, error),
    }
}

fn render_old_jpeg_tiff_payload_fallback(
    request: &RenderRequest,
    plan: &RenderPlan,
    primary_error: RenderError,
) -> std::result::Result<BackendRenderResult, RenderError> {
    if !should_try_old_jpeg_tiff_payload_fallback(request, &primary_error) {
        return Err(primary_error);
    }
    let Some(payload) = old_jpeg_tiff::extract_standalone_jpeg_payload(&request.source_path)?
    else {
        return Err(primary_error);
    };
    let fallback_request = RenderRequest {
        source_path: payload.path().to_path_buf(),
        destination_path: request.destination_path.clone(),
        purpose: request.purpose,
        recipe: request.recipe.clone(),
        limits: request.limits,
    };
    let mut result =
        render(&fallback_request, plan).map_err(|fallback_error| RenderError::DecodeFailed {
            path: request.source_path.clone(),
            detail: format!("old-style JPEG TIFF payload fallback failed: {fallback_error}"),
        })?;
    result.renderer = format!("{}+old-jpeg-tiff-payload", result.renderer);
    result.renderer_options_json =
        renderer_options_with_old_jpeg_tiff_fallback(&result.renderer_options_json, payload.path());
    Ok(result)
}

fn should_try_old_jpeg_tiff_payload_fallback(request: &RenderRequest, error: &RenderError) -> bool {
    request
        .source_path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| matches!(extension.to_ascii_lowercase().as_str(), "tif" | "tiff"))
        && render_error_detail(error).is_some_and(is_old_jpeg_tiff_error_detail)
}

fn render_error_detail(error: &RenderError) -> Option<&str> {
    match error {
        RenderError::DecodeFailed { detail, .. }
        | RenderError::EncodeFailed { detail, .. }
        | RenderError::RendererUnavailable { detail, .. } => Some(detail),
        _ => None,
    }
}

fn is_old_jpeg_tiff_error_detail(detail: &str) -> bool {
    let detail = detail.to_ascii_lowercase();
    detail.contains("old-style jpeg")
        || detail.contains("requested compression method is not configured")
}

fn renderer_options_with_old_jpeg_tiff_fallback(
    renderer_options_json: &str,
    payload_path: &std::path::Path,
) -> String {
    let inner = serde_json::from_str::<serde_json::Value>(renderer_options_json)
        .unwrap_or_else(|_| serde_json::Value::String(renderer_options_json.to_string()));
    serde_json::json!({
        "fallback": "old_jpeg_tiff_payload",
        "source_container": "TIFF",
        "payload_format": "JPEG",
        "payload_extension": payload_path.extension().and_then(|value| value.to_str()),
        "inner": inner
    })
    .to_string()
}

fn render_with_primary_vips_backend(
    request: &RenderRequest,
    plan: &RenderPlan,
) -> std::result::Result<BackendRenderResult, RenderError> {
    render_with_primary_vips_backend_impl(request, plan)
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
fn render_with_primary_vips_backend_impl(
    request: &RenderRequest,
    plan: &RenderPlan,
) -> std::result::Result<BackendRenderResult, RenderError> {
    match vips_linked::render(request, plan) {
        Ok(result) => Ok(result),
        Err(RenderError::RendererUnavailable { .. }) => vips::render(request, plan),
        Err(error) => Err(error),
    }
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn render_with_primary_vips_backend_impl(
    request: &RenderRequest,
    plan: &RenderPlan,
) -> std::result::Result<BackendRenderResult, RenderError> {
    vips::render(request, plan)
}

#[cfg(all(test, any(target_os = "windows", target_os = "macos")))]
fn primary_vips_backend_name() -> &'static str {
    vips_linked::renderer_name()
}

fn image_rs_fast_path_allowed(request: &RenderRequest, plan: &RenderPlan) -> bool {
    image_rs_fast_path_allowed_with_max(request, plan, image_rs_fast_path_max_bytes())
}

fn image_rs_fast_path_allowed_with_max(
    request: &RenderRequest,
    plan: &RenderPlan,
    max_bytes: u64,
) -> bool {
    has_image_rs_fast_path_extension(&request.source_path)
        && plan.estimated_scheduler_weight_bytes <= max_bytes
        && plan.estimated_scheduler_weight_bytes <= request.limits.image_rs_max_alloc_bytes
}

fn has_image_rs_fast_path_extension(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "jpg" | "jpeg" | "png"
            )
        })
}

fn image_rs_fast_path_max_bytes() -> u64 {
    env::var("OACURATOR_IMAGE_RS_FAST_PATH_MAX_MB")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .and_then(|megabytes| megabytes.checked_mul(1024 * 1024))
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_IMAGE_RS_FAST_PATH_MAX_BYTES)
}

fn image_rs_fallback_allowed() -> bool {
    image_rs_fallback_allowed_from_value(
        env::var("OACURATOR_IMAGE_RENDER_ALLOW_IMAGE_RS_FALLBACK")
            .ok()
            .as_deref(),
    )
}

fn image_rs_fallback_allowed_from_value(value: Option<&str>) -> bool {
    value.is_some_and(|value| matches!(value, "1" | "true" | "TRUE" | "yes" | "YES"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::image_metadata::read_image_metadata;
    use crate::image_render::recipe::basic_png_export_recipe;
    use crate::image_render::scheduler::RenderLimits;
    use crate::image_render::service::RenderPurpose;
    use image::{ImageBuffer, Rgb};
    use std::fs;
    use std::io::Cursor;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn request(path: &str) -> RenderRequest {
        RenderRequest {
            source_path: PathBuf::from(path),
            destination_path: PathBuf::from("out.png"),
            purpose: RenderPurpose::ExportBasicPng,
            recipe: basic_png_export_recipe(),
            limits: RenderLimits::default(),
        }
    }

    fn plan(weight: u64) -> RenderPlan {
        RenderPlan {
            source_width: 1,
            source_height: 1,
            target_width: 1,
            target_height: 1,
            estimated_scheduler_weight_bytes: weight,
        }
    }

    fn precise_plan(width: u32, height: u32) -> RenderPlan {
        RenderPlan {
            source_width: width,
            source_height: height,
            target_width: width,
            target_height: height,
            estimated_scheduler_weight_bytes: u64::from(width) * u64::from(height) * 8,
        }
    }

    #[test]
    fn image_rs_fast_path_is_capped_to_small_jpeg_and_png_sources() {
        let small = plan(128 * 1024 * 1024);
        let too_large = plan((128 * 1024 * 1024) + 1);

        assert!(image_rs_fast_path_allowed_with_max(
            &request("scan.jpg"),
            &small,
            DEFAULT_IMAGE_RS_FAST_PATH_MAX_BYTES
        ));
        assert!(image_rs_fast_path_allowed_with_max(
            &request("scan.png"),
            &small,
            DEFAULT_IMAGE_RS_FAST_PATH_MAX_BYTES
        ));
        assert!(!image_rs_fast_path_allowed_with_max(
            &request("scan.tif"),
            &small,
            DEFAULT_IMAGE_RS_FAST_PATH_MAX_BYTES
        ));
        assert!(!image_rs_fast_path_allowed_with_max(
            &request("scan.jpg"),
            &too_large,
            DEFAULT_IMAGE_RS_FAST_PATH_MAX_BYTES
        ));
    }

    #[test]
    fn image_rs_large_fallback_requires_explicit_opt_in() {
        assert!(!image_rs_fallback_allowed_from_value(None));
        assert!(!image_rs_fallback_allowed_from_value(Some("false")));
        assert!(image_rs_fallback_allowed_from_value(Some("1")));
        assert!(image_rs_fallback_allowed_from_value(Some("true")));
        assert!(image_rs_fallback_allowed_from_value(Some("yes")));
    }

    #[test]
    fn old_jpeg_tiff_fallback_catches_lazy_encode_phase_decoder_errors() {
        let request = request("scan.tif");
        let error = RenderError::EncodeFailed {
            path: PathBuf::from("out.png"),
            detail: "pngsave: tiff2vips: Old-style JPEG compression support is not configured"
                .to_string(),
        };

        assert!(should_try_old_jpeg_tiff_payload_fallback(&request, &error));
    }

    #[test]
    fn old_jpeg_tiff_fallback_is_not_used_for_plain_write_errors() {
        let request = request("scan.tif");
        let error = RenderError::EncodeFailed {
            path: PathBuf::from("out.png"),
            detail: "access denied".to_string(),
        };

        assert!(!should_try_old_jpeg_tiff_payload_fallback(&request, &error));
    }

    #[test]
    fn old_jpeg_tiff_payload_fallback_renders_extracted_jpeg() {
        let dir = TempDir::new().unwrap();
        let source_path = dir.path().join("old-style-jpeg.tif");
        let destination_path = dir.path().join("out.png");
        let jpeg = test_jpeg_bytes(16, 10);
        write_old_style_jpeg_tiff_wrapper(&source_path, 16, 10, &jpeg);
        let request = RenderRequest {
            source_path: source_path.clone(),
            destination_path: destination_path.clone(),
            purpose: RenderPurpose::ExportBasicPng,
            recipe: basic_png_export_recipe(),
            limits: RenderLimits::default(),
        };
        let primary_error = RenderError::EncodeFailed {
            path: destination_path.clone(),
            detail: "pngsave: tiff2vips: Old-style JPEG compression support is not configured"
                .to_string(),
        };

        let rendered =
            render_old_jpeg_tiff_payload_fallback(&request, &precise_plan(16, 10), primary_error)
                .unwrap();

        assert_eq!(rendered.width, 16);
        assert_eq!(rendered.height, 10);
        assert_eq!(rendered.format, "png");
        assert!(rendered.renderer.contains("old-jpeg-tiff-payload"));
        assert!(destination_path.is_file());
        let metadata = read_image_metadata(&destination_path).unwrap();
        assert_eq!(metadata.width, 16);
        assert_eq!(metadata.height, 10);
    }

    fn test_jpeg_bytes(width: u32, height: u32) -> Vec<u8> {
        let image = ImageBuffer::from_fn(width, height, |x, y| {
            Rgb([(x * 13) as u8, (y * 19) as u8, ((x + y) * 7) as u8])
        });
        let mut bytes = Cursor::new(Vec::new());
        image
            .write_to(&mut bytes, image::ImageFormat::Jpeg)
            .unwrap();
        bytes.into_inner()
    }

    fn write_old_style_jpeg_tiff_wrapper(
        path: &std::path::Path,
        width: u32,
        height: u32,
        jpeg: &[u8],
    ) {
        const SHORT: u16 = 3;
        const LONG: u16 = 4;

        let entry_count = 12u16;
        let ifd_offset = 8u32;
        let bits_offset = ifd_offset + 2 + u32::from(entry_count) * 12 + 4;
        let jpeg_offset = bits_offset + 6;
        let jpeg_length = u32::try_from(jpeg.len()).unwrap();
        let mut bytes = Vec::new();

        bytes.extend_from_slice(b"II");
        bytes.extend_from_slice(&42u16.to_le_bytes());
        bytes.extend_from_slice(&ifd_offset.to_le_bytes());
        bytes.extend_from_slice(&entry_count.to_le_bytes());

        write_ifd_entry(&mut bytes, 256, LONG, 1, width);
        write_ifd_entry(&mut bytes, 257, LONG, 1, height);
        write_ifd_entry(&mut bytes, 258, SHORT, 3, bits_offset);
        write_ifd_entry(&mut bytes, 259, SHORT, 1, 6);
        write_ifd_entry(&mut bytes, 262, SHORT, 1, 6);
        write_ifd_entry(&mut bytes, 273, LONG, 1, jpeg_offset);
        write_ifd_entry(&mut bytes, 277, SHORT, 1, 3);
        write_ifd_entry(&mut bytes, 278, LONG, 1, height);
        write_ifd_entry(&mut bytes, 279, LONG, 1, jpeg_length);
        write_ifd_entry(&mut bytes, 284, SHORT, 1, 1);
        write_ifd_entry(&mut bytes, 513, LONG, 1, jpeg_offset);
        write_ifd_entry(&mut bytes, 514, LONG, 1, jpeg_length);
        bytes.extend_from_slice(&0u32.to_le_bytes());

        for bits in [8u16, 8, 8] {
            bytes.extend_from_slice(&bits.to_le_bytes());
        }
        bytes.extend_from_slice(jpeg);
        fs::write(path, bytes).unwrap();
    }

    fn write_ifd_entry(bytes: &mut Vec<u8>, tag: u16, kind: u16, count: u32, value: u32) {
        bytes.extend_from_slice(&tag.to_le_bytes());
        bytes.extend_from_slice(&kind.to_le_bytes());
        bytes.extend_from_slice(&count.to_le_bytes());
        if kind == 3 && count == 1 {
            let value = u16::try_from(value).unwrap();
            bytes.extend_from_slice(&value.to_le_bytes());
            bytes.extend_from_slice(&0u16.to_le_bytes());
        } else {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
    }

    #[cfg(any(target_os = "windows", target_os = "macos"))]
    #[test]
    fn primary_vips_backend_is_linked_on_desktop_release_platforms() {
        assert_eq!(primary_vips_backend_name(), "libvips-linked");
    }
}
