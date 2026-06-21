// Copyright (c) 2026 Remgrandt Works. All rights reserved.

use crate::image_render::error::RenderError;
use crate::image_render::recipe::ResizeMode;
use crate::image_render::service::RenderRequest;
use std::env;
use std::sync::{Condvar, Mutex, OnceLock};

const DEFAULT_IMAGE_RENDER_MEMORY_BUDGET_BYTES: u64 = 512 * 1024 * 1024;
const DEFAULT_IMAGE_RS_MAX_ALLOC_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const DEFAULT_MAX_SOURCE_PIXELS: u64 = 180_000_000;
const DEFAULT_MAX_OUTPUT_PIXELS: u64 = 80_000_000;

static GLOBAL_RENDER_SCHEDULER: OnceLock<RenderScheduler> = OnceLock::new();

#[derive(Debug, Clone, Copy)]
pub struct RenderLimits {
    pub max_source_pixels: u64,
    pub max_output_pixels: u64,
    pub image_rs_max_alloc_bytes: u64,
}

impl Default for RenderLimits {
    fn default() -> Self {
        Self {
            max_source_pixels: DEFAULT_MAX_SOURCE_PIXELS,
            max_output_pixels: DEFAULT_MAX_OUTPUT_PIXELS,
            image_rs_max_alloc_bytes: DEFAULT_IMAGE_RS_MAX_ALLOC_BYTES,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct RenderPlan {
    pub source_width: u32,
    pub source_height: u32,
    pub target_width: u32,
    pub target_height: u32,
    pub estimated_scheduler_weight_bytes: u64,
}

impl RenderPlan {
    pub fn output_pixels(self) -> u64 {
        u64::from(self.target_width).saturating_mul(u64::from(self.target_height))
    }
}

pub struct RenderScheduler {
    memory_budget_bytes: u64,
    active_estimated_bytes: Mutex<u64>,
    available: Condvar,
}

impl RenderScheduler {
    pub fn new(memory_budget_bytes: u64) -> Self {
        Self {
            memory_budget_bytes,
            active_estimated_bytes: Mutex::new(0),
            available: Condvar::new(),
        }
    }

    pub fn with_permit<T>(
        &self,
        plan: &RenderPlan,
        operation: impl FnOnce() -> Result<T, RenderError>,
    ) -> Result<T, RenderError> {
        let _permit = self.acquire(plan.estimated_scheduler_weight_bytes);
        operation()
    }

    fn acquire(&self, estimated_bytes: u64) -> RenderPermit<'_> {
        let mut guard = self
            .active_estimated_bytes
            .lock()
            .expect("render scheduler mutex poisoned");
        loop {
            if *guard == 0 || guard.saturating_add(estimated_bytes) <= self.memory_budget_bytes {
                *guard = guard.saturating_add(estimated_bytes);
                return RenderPermit {
                    scheduler: self,
                    estimated_bytes,
                };
            }
            guard = self
                .available
                .wait(guard)
                .expect("render scheduler mutex poisoned");
        }
    }
}

struct RenderPermit<'a> {
    scheduler: &'a RenderScheduler,
    estimated_bytes: u64,
}

impl Drop for RenderPermit<'_> {
    fn drop(&mut self) {
        let mut guard = self
            .scheduler
            .active_estimated_bytes
            .lock()
            .expect("render scheduler mutex poisoned");
        *guard = guard.saturating_sub(self.estimated_bytes);
        self.scheduler.available.notify_all();
    }
}

pub fn global_render_scheduler() -> &'static RenderScheduler {
    GLOBAL_RENDER_SCHEDULER.get_or_init(|| RenderScheduler::new(render_memory_budget_bytes()))
}

pub fn render_memory_budget_bytes() -> u64 {
    env::var("OACURATOR_IMAGE_RENDER_MEMORY_BUDGET_MB")
        .ok()
        .or_else(|| env::var("OACURATOR_CACHE_MEMORY_BUDGET_MB").ok())
        .and_then(|value| value.parse::<u64>().ok())
        .and_then(|megabytes| megabytes.checked_mul(1024 * 1024))
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_IMAGE_RENDER_MEMORY_BUDGET_BYTES)
}

pub fn estimate_decode_weight_bytes_from_dimensions(width: u32, height: u32) -> u64 {
    u64::from(width)
        .saturating_mul(u64::from(height))
        .saturating_mul(8)
}

pub fn estimate_decode_weight_bytes(source: &std::path::Path) -> crate::Result<u64> {
    let metadata = crate::image_metadata::read_image_metadata(source)?;
    Ok(estimate_decode_weight_bytes_from_dimensions(
        metadata.width as u32,
        metadata.height as u32,
    ))
}

pub fn compile_render_plan(
    request: &RenderRequest,
    source_width: u32,
    source_height: u32,
) -> Result<RenderPlan, RenderError> {
    let limits = request.limits;
    let source_pixels = u64::from(source_width).saturating_mul(u64::from(source_height));
    if source_pixels > limits.max_source_pixels {
        return Err(RenderError::SourceTooLarge {
            path: request.source_path.clone(),
            width: source_width,
            height: source_height,
            max_pixels: limits.max_source_pixels,
        });
    }

    let (target_width, target_height) = target_dimensions(
        source_width,
        source_height,
        &request.recipe.resize,
        request.recipe.allow_upscale,
    );
    let output_pixels = u64::from(target_width).saturating_mul(u64::from(target_height));
    if output_pixels > limits.max_output_pixels {
        return Err(RenderError::OutputTooLarge {
            width: target_width,
            height: target_height,
            max_pixels: limits.max_output_pixels,
        });
    }

    Ok(RenderPlan {
        source_width,
        source_height,
        target_width,
        target_height,
        estimated_scheduler_weight_bytes: estimate_decode_weight_bytes_from_dimensions(
            source_width,
            source_height,
        ),
    })
}

fn target_dimensions(
    source_width: u32,
    source_height: u32,
    resize: &ResizeMode,
    allow_upscale: bool,
) -> (u32, u32) {
    match resize {
        ResizeMode::FitWithin {
            max_width,
            max_height,
        } => {
            let width_scale = max_width
                .map(|value| f64::from(value) / f64::from(source_width))
                .unwrap_or(f64::INFINITY);
            let height_scale = max_height
                .map(|value| f64::from(value) / f64::from(source_height))
                .unwrap_or(f64::INFINITY);
            let mut scale = width_scale.min(height_scale);
            if !allow_upscale {
                scale = scale.min(1.0);
            }
            if !scale.is_finite() {
                scale = 1.0;
            }
            let width = (f64::from(source_width) * scale).round().max(1.0) as u32;
            let height = (f64::from(source_height) * scale).round().max(1.0) as u32;
            (width, height)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::image_render::recipe::preview_recipe;
    use crate::image_render::service::{RenderPurpose, RenderRequest};
    use std::path::PathBuf;

    #[test]
    fn fit_within_height_preserves_ratio() {
        let request = RenderRequest {
            source_path: PathBuf::from("scan.tif"),
            destination_path: PathBuf::from("preview.jpg"),
            purpose: RenderPurpose::Preview,
            recipe: preview_recipe(),
            limits: RenderLimits::default(),
        };

        let plan = compile_render_plan(&request, 2000, 3000).unwrap();

        assert_eq!((plan.target_width, plan.target_height), (1333, 2000));
    }

    #[test]
    fn oversized_item_fits_scheduler_only_when_running_alone() {
        let scheduler = RenderScheduler::new(512);
        let plan = RenderPlan {
            source_width: 1,
            source_height: 1,
            target_width: 1,
            target_height: 1,
            estimated_scheduler_weight_bytes: 600,
        };

        scheduler.with_permit(&plan, || Ok(())).unwrap();
    }
}
