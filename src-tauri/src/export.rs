use crate::catalog::{AssetKind, Catalog, FileAsset};
use crate::image_render::{
    basic_png_export_recipe, premium_png_export_recipe, render_image_to_file, RenderLimits,
    RenderPurpose, RenderRequest,
};
use crate::path_safety::safe_path_component;
use crate::scanner::{attach_files_to_artwork, AttachMode};
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
    cache_dir: &Path,
    variant: PngExportVariant,
) -> Result<FileAsset> {
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
    let detail = attach_files_to_artwork(
        catalog,
        artwork_id,
        std::slice::from_ref(&rendered.path),
        cache_dir,
        AttachMode::Copy,
    )?;
    let file_asset_id = detail
        .file_assets
        .iter()
        .find(|asset| asset.original_path == rendered.path)
        .map(|asset| asset.id)
        .ok_or_else(|| AppError::Message("PNG export was created but not attached".to_string()))?;
    catalog.update_image_role(AssetKind::File, file_asset_id, Some(variant.image_role()))?;
    catalog.file_asset(file_asset_id)
}

fn is_renderable_png_export_source(extension: &str) -> bool {
    matches!(
        extension.trim().to_ascii_lowercase().as_str(),
        "jpg" | "jpeg" | "png" | "tif" | "tiff"
    )
}

fn unique_export_path(folder: &Path, canonical_id: &str, title: &str) -> PathBuf {
    let safe_title = safe_path_component(title, "Untitled Artwork");
    let base = format!("{canonical_id} - {safe_title}");
    let mut candidate = folder.join(format!("{base}.png"));
    let mut index = 2;
    while candidate.exists() {
        candidate = folder.join(format!("{base} - {index}.png"));
        index += 1;
    }
    candidate
}
