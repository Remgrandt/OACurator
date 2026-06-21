// Copyright (c) 2026 Remgrandt Works. All rights reserved.

use crate::image_render::error::RenderError;
use crate::image_render::scheduler::RenderPlan;
use crate::image_render::service::RenderRequest;
use std::env;

pub mod image_rs;
pub mod vips;

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

    match vips::render(request, plan) {
        Ok(result) => Ok(result),
        Err(RenderError::RendererUnavailable { .. }) if image_rs_fallback_allowed() => {
            image_rs::render(request, plan, image_rs::ImageRsMode::DebugFallback)
        }
        Err(error) => Err(error),
    }
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
    use crate::image_render::recipe::basic_png_export_recipe;
    use crate::image_render::scheduler::RenderLimits;
    use crate::image_render::service::RenderPurpose;
    use std::path::PathBuf;

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
}
