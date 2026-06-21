// Copyright (c) 2026 Remgrandt Works. All rights reserved.

pub mod backend;
pub mod cache;
pub mod error;
pub mod probe;
pub mod recipe;
pub mod scheduler;
pub mod service;

pub use error::RenderError;
pub use probe::{probe_source, ImageProbeResult, ProbeStatus, RenderStatus, SourceAssetKind};
pub use recipe::{
    basic_png_export_recipe, premium_png_export_recipe, preview_recipe,
    raremarq_upload_jpeg_recipe, thumbnail_recipe, AlphaPolicy, ColorPolicy, OutputFormat,
    RenderRecipe, ResizeFilter, ResizeMode,
};
pub use scheduler::{estimate_decode_weight_bytes, RenderLimits, RenderPlan};
pub use service::{
    render_image_to_file, RenderPurpose, RenderRequest, RenderedImage, SourceFingerprint,
};
