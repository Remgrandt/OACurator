// Copyright (c) 2026 Remgrandt Works. All rights reserved.

use crate::catalog::{Catalog, DerivedAsset, DerivedAssetInsert};
use crate::file_ops::PlanStatus;
use crate::{AppError, Result};
use image::imageops::FilterType;
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
    fn max_height(self) -> u32 {
        match self {
            PngExportVariant::Basic => 800,
            PngExportVariant::Premium => 2000,
        }
    }

    fn image_role(self) -> &'static str {
        match self {
            PngExportVariant::Basic => "basic",
            PngExportVariant::Premium => "premium",
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
    let image = image::open(&source.current_path)?;
    let output_image = if image.height() > variant.max_height() {
        let height = variant.max_height();
        let width = scaled_width_for_height(image.width(), image.height(), height);
        image.resize_exact(width, height, FilterType::Lanczos3)
    } else {
        image
    };
    let folder = export_root.join(&detail.canonical_id);
    fs::create_dir_all(&folder)?;
    let path = unique_export_path(&folder, &detail.canonical_id, &detail.title);
    output_image.save_with_format(&path, image::ImageFormat::Png)?;
    catalog.add_derived_asset(
        artwork_id,
        DerivedAssetInsert {
            source_file_asset_id: Some(source_file_asset_id),
            derivative_type: "png_export",
            format: "png",
            path: &path,
            width: output_image.width() as i64,
            height: output_image.height() as i64,
            image_role: Some(variant.image_role()),
        },
    )
}

fn is_renderable_png_export_source(extension: &str) -> bool {
    matches!(
        extension.trim().to_ascii_lowercase().as_str(),
        "jpg" | "jpeg" | "png" | "tif" | "tiff"
    )
}

fn scaled_width_for_height(width: u32, height: u32, target_height: u32) -> u32 {
    let numerator = u64::from(width) * u64::from(target_height) + (u64::from(height) / 2);
    ((numerator / u64::from(height)) as u32).max(1)
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

#[allow(dead_code)]
fn _plan_status_used(status: PlanStatus) -> PlanStatus {
    status
}
