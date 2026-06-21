// Copyright (c) 2026 Remgrandt Works. All rights reserved.

use crate::catalog::{Catalog, DerivedAsset, DerivedAssetInsert, DerivedAssetRenderInsert};
use crate::file_ops::PlanStatus;
use crate::image_render::{
    basic_png_export_recipe, premium_png_export_recipe, render_image_to_file, RenderLimits,
    RenderPurpose, RenderRequest, RenderedImage,
};
use crate::{AppError, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PngExportVariant {
    #[serde(alias = "caf_basic")]
    Basic,
    #[serde(alias = "caf_premium")]
    Premium,
}

impl PngExportVariant {
    fn image_role(self) -> &'static str {
        match self {
            PngExportVariant::Basic => "basic",
            PngExportVariant::Premium => "premium",
        }
    }

    fn render_purpose(self) -> RenderPurpose {
        match self {
            PngExportVariant::Basic => RenderPurpose::ExportBasicPng,
            PngExportVariant::Premium => RenderPurpose::ExportPremiumPng,
        }
    }
}

pub fn create_png_derivative(
    catalog: &Catalog,
    artwork_id: i64,
    source_file_asset_id: i64,
    export_root: &Path,
    variant: PngExportVariant,
) -> Result<DerivedAsset> {
    if export_root.as_os_str().is_empty() || !export_root.is_absolute() {
        return Err(AppError::Message(
            "Export destination must be an absolute folder path".to_string(),
        ));
    }
    let detail = catalog.artwork_detail(artwork_id)?;
    let source = catalog.file_asset(source_file_asset_id)?;
    if source.artwork_id != artwork_id {
        return Err(AppError::Message(
            "File asset does not belong to artwork".to_string(),
        ));
    }
    if !is_renderable_png_export_source(&source.extension) {
        return Err(AppError::Message(
            "PNG export requires a JPG, PNG, or TIFF source file".to_string(),
        ));
    }
    let folder = export_root.join(&detail.canonical_id);
    fs::create_dir_all(&folder)?;
    let path = unique_export_path(&folder, &detail.canonical_id, &detail.title);
    let rendered = render_image_to_file(RenderRequest {
        source_path: source.current_path.clone(),
        destination_path: path,
        purpose: variant.render_purpose(),
        recipe: match variant {
            PngExportVariant::Basic => basic_png_export_recipe(),
            PngExportVariant::Premium => premium_png_export_recipe(),
        },
        limits: RenderLimits::default(),
    })?;
    let derived = catalog.add_derived_asset(
        artwork_id,
        DerivedAssetInsert {
            source_file_asset_id: Some(source_file_asset_id),
            derivative_type: "png_export",
            format: &rendered.format,
            path: &rendered.path,
            width: i64::from(rendered.width),
            height: i64::from(rendered.height),
            image_role: Some(variant.image_role()),
        },
    )?;
    catalog.add_derived_asset_render(render_metadata_insert(
        derived.id,
        variant.render_purpose(),
        &rendered,
    ))?;
    Ok(derived)
}

fn is_renderable_png_export_source(extension: &str) -> bool {
    matches!(
        extension.trim().to_ascii_lowercase().as_str(),
        "jpg" | "jpeg" | "png" | "tif" | "tiff"
    )
}

fn unique_export_path(folder: &Path, canonical_id: &str, title: &str) -> PathBuf {
    let safe_title = sanitize_filename::sanitize(title.trim());
    let safe_title = if safe_title.is_empty() {
        "Untitled Artwork".to_string()
    } else {
        safe_title
    };
    let base = format!("{canonical_id} - {safe_title}");
    let mut candidate = folder.join(format!("{base}.png"));
    let mut index = 2;
    while candidate.exists() {
        candidate = folder.join(format!("{base} - {index}.png"));
        index += 1;
    }
    candidate
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

#[allow(dead_code)]
fn _plan_status_used(status: PlanStatus) -> PlanStatus {
    status
}
