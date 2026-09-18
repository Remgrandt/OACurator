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
