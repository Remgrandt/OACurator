// Copyright (c) 2026 Remgrandt Works. All rights reserved.

use crate::catalog::FileAssetMetadata;
use crate::image_metadata::read_image_metadata;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum SourceAssetKind {
    Image,
    NonImage,
    Unknown,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ProbeStatus {
    Succeeded,
    Failed,
}

impl ProbeStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum RenderStatus {
    Renderable,
    UnsupportedEncoding,
    RendererUnavailable,
    SourceTooLarge,
    Corrupt,
    Unknown,
}

impl RenderStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Renderable => "renderable",
            Self::UnsupportedEncoding => "unsupported_encoding",
            Self::RendererUnavailable => "renderer_unavailable",
            Self::SourceTooLarge => "source_too_large",
            Self::Corrupt => "corrupt",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageProbeResult {
    pub path: PathBuf,
    pub source_kind: SourceAssetKind,
    pub probe_status: ProbeStatus,
    pub render_status: RenderStatus,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub dpi_x: Option<f64>,
    pub dpi_y: Option<f64>,
    pub container_format: Option<String>,
    pub detected_mime: Option<String>,
    pub compression: Option<String>,
    pub photometric: Option<String>,
    pub bits_per_sample: Option<i64>,
    pub samples_per_pixel: Option<i64>,
    pub has_alpha: Option<bool>,
    pub preferred_renderer: Option<String>,
    pub renderer_version: Option<String>,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
}

impl ImageProbeResult {
    pub fn metadata_for_catalog(&self) -> FileAssetMetadata {
        FileAssetMetadata {
            width: self.width,
            height: self.height,
            dpi_x: self.dpi_x,
            dpi_y: self.dpi_y,
        }
    }

    pub fn should_attempt_render(&self) -> bool {
        matches!(
            self.source_kind,
            SourceAssetKind::Image | SourceAssetKind::Unknown
        )
    }
}

pub fn probe_source(path: &Path) -> ImageProbeResult {
    let container_format = container_format(path);
    let detected_mime = detected_mime(container_format.as_deref());
    if container_format.is_none() {
        return ImageProbeResult {
            path: path.to_path_buf(),
            source_kind: SourceAssetKind::NonImage,
            probe_status: ProbeStatus::Failed,
            render_status: RenderStatus::Unknown,
            width: None,
            height: None,
            dpi_x: None,
            dpi_y: None,
            container_format: None,
            detected_mime: None,
            compression: None,
            photometric: None,
            bits_per_sample: None,
            samples_per_pixel: None,
            has_alpha: None,
            preferred_renderer: None,
            renderer_version: None,
            error_code: Some("not_an_image".to_string()),
            error_message: None,
        };
    }

    match read_image_metadata(path) {
        Ok(metadata) => ImageProbeResult {
            path: path.to_path_buf(),
            source_kind: SourceAssetKind::Image,
            probe_status: ProbeStatus::Succeeded,
            render_status: RenderStatus::Renderable,
            width: Some(metadata.width),
            height: Some(metadata.height),
            dpi_x: metadata.dpi_x,
            dpi_y: metadata.dpi_y,
            container_format,
            detected_mime,
            compression: None,
            photometric: None,
            bits_per_sample: None,
            samples_per_pixel: None,
            has_alpha: None,
            preferred_renderer: Some("libvips".to_string()),
            renderer_version: None,
            error_code: None,
            error_message: None,
        },
        Err(error) => ImageProbeResult {
            path: path.to_path_buf(),
            source_kind: SourceAssetKind::Image,
            probe_status: ProbeStatus::Failed,
            render_status: RenderStatus::Unknown,
            width: None,
            height: None,
            dpi_x: None,
            dpi_y: None,
            container_format,
            detected_mime,
            compression: None,
            photometric: None,
            bits_per_sample: None,
            samples_per_pixel: None,
            has_alpha: None,
            preferred_renderer: Some("libvips".to_string()),
            renderer_version: None,
            error_code: Some("probe_failed".to_string()),
            error_message: Some(error.to_string()),
        },
    }
}

fn container_format(path: &Path) -> Option<String> {
    match path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "jpg" | "jpeg" => Some("JPEG".to_string()),
        "png" => Some("PNG".to_string()),
        "tif" | "tiff" => Some("TIFF".to_string()),
        _ => None,
    }
}

fn detected_mime(container_format: Option<&str>) -> Option<String> {
    match container_format {
        Some("JPEG") => Some("image/jpeg".to_string()),
        Some("PNG") => Some("image/png".to_string()),
        Some("TIFF") => Some("image/tiff".to_string()),
        _ => None,
    }
}
