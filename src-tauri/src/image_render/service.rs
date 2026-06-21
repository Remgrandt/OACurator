// Copyright (c) 2026 Remgrandt Works. All rights reserved.

use crate::image_metadata::read_image_metadata;
use crate::image_render::backend::{self, BackendRenderResult};
use crate::image_render::error::RenderError;
use crate::image_render::recipe::RenderRecipe;
use crate::image_render::scheduler::{compile_render_plan, global_render_scheduler, RenderLimits};
use crate::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

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
