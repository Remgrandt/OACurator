// Copyright (c) 2026 Remgrandt Works. All rights reserved.

use crate::image_render::RenderError;
use crate::AppError;
use serde_json::json;
use std::fs::{self, OpenOptions};
use std::io::ErrorKind;
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UserErrorCode {
    RendererUnavailable,
    RenderSourceMissing,
    RenderTooLarge,
    RenderUnsupportedEncoding,
    RenderWriteAccess,
    RenderFailed,
}

impl UserErrorCode {
    fn as_str(self) -> &'static str {
        match self {
            Self::RendererUnavailable => "renderer_unavailable",
            Self::RenderSourceMissing => "render_source_missing",
            Self::RenderTooLarge => "render_too_large",
            Self::RenderUnsupportedEncoding => "render_unsupported_encoding",
            Self::RenderWriteAccess => "render_write_access",
            Self::RenderFailed => "render_failed",
        }
    }
}

#[derive(Debug, Clone)]
pub struct UserErrorPresentation {
    pub code: UserErrorCode,
    pub message: String,
    pub technical_detail: Option<String>,
}

#[derive(Debug, Clone, Copy)]
pub enum DiagnosticOperation {
    PreviewGeneration,
    PngExport,
}

impl DiagnosticOperation {
    fn as_str(self) -> &'static str {
        match self {
            Self::PreviewGeneration => "preview_generation",
            Self::PngExport => "png_export",
        }
    }
}

pub fn preview_generation_warning(filename: &str, error: &AppError) -> UserErrorPresentation {
    let code = classify_error(error);
    UserErrorPresentation {
        code,
        message: preview_message(filename, code),
        technical_detail: Some(technical_detail(error)),
    }
}

pub fn png_export_error(error: &AppError) -> UserErrorPresentation {
    let code = classify_error(error);
    UserErrorPresentation {
        code,
        message: png_export_message(code),
        technical_detail: Some(technical_detail(error)),
    }
}

fn preview_message(filename: &str, code: UserErrorCode) -> String {
    match code {
        UserErrorCode::RenderUnsupportedEncoding => format!(
            "Preview could not be generated for {filename}. OA Curator could not render this file type or image encoding. The source file was not changed."
        ),
        UserErrorCode::RenderWriteAccess => format!(
            "Preview could not be generated for {filename}. OA Curator could not write to the preview cache. The source file was not changed."
        ),
        UserErrorCode::RendererUnavailable => format!(
            "Preview could not be generated for {filename}. OA Curator's image renderer is not available. The source file was not changed."
        ),
        UserErrorCode::RenderSourceMissing => format!(
            "Preview could not be generated for {filename}. OA Curator could not find the source file. The source file was not changed."
        ),
        UserErrorCode::RenderTooLarge => format!(
            "Preview could not be generated for {filename}. The image is too large for the current render limits. The source file was not changed."
        ),
        UserErrorCode::RenderFailed => format!(
            "Preview could not be generated for {filename}. OA Curator could not render this image. The source file was not changed."
        ),
    }
}

fn png_export_message(code: UserErrorCode) -> String {
    match code {
        UserErrorCode::RenderUnsupportedEncoding => {
            "PNG export failed. OA Curator cannot render this file type or image encoding as a PNG export. The source file was not changed.".to_string()
        }
        UserErrorCode::RenderWriteAccess => {
            "PNG export failed. OA Curator could not write to the selected folder. Choose another folder or check folder permissions.".to_string()
        }
        UserErrorCode::RendererUnavailable => {
            "PNG export failed. OA Curator's image renderer is not available. The source file was not changed.".to_string()
        }
        UserErrorCode::RenderSourceMissing => {
            "PNG export failed. OA Curator could not find the selected source file. The source file was not changed.".to_string()
        }
        UserErrorCode::RenderTooLarge => {
            "PNG export failed. The image is too large for the current export limits. The source file was not changed.".to_string()
        }
        UserErrorCode::RenderFailed => {
            "PNG export failed. OA Curator could not render this image. The source file was not changed.".to_string()
        }
    }
}

fn classify_error(error: &AppError) -> UserErrorCode {
    if is_write_access_error(error) {
        return UserErrorCode::RenderWriteAccess;
    }
    if is_unsupported_encoding(error) {
        return UserErrorCode::RenderUnsupportedEncoding;
    }
    match error {
        AppError::Render(RenderError::RendererUnavailable { .. }) => {
            UserErrorCode::RendererUnavailable
        }
        AppError::Render(RenderError::SourceMissing { .. }) => UserErrorCode::RenderSourceMissing,
        AppError::Render(
            RenderError::SourceTooLarge { .. } | RenderError::OutputTooLarge { .. },
        ) => UserErrorCode::RenderTooLarge,
        AppError::Io(io_error) if io_error.kind() == ErrorKind::NotFound => {
            UserErrorCode::RenderSourceMissing
        }
        _ => UserErrorCode::RenderFailed,
    }
}

fn is_unsupported_encoding(error: &AppError) -> bool {
    if matches!(
        error,
        AppError::Render(RenderError::UnsupportedFormat { .. })
    ) {
        return true;
    }
    let detail = technical_detail(error).to_ascii_lowercase();
    [
        "old-style jpeg compression support is not configured",
        "requested compression method is not configured",
        "unsupported image format",
        "unsupported image type",
        "unknown color type",
        "png export requires a jpg, png, or tiff source file",
    ]
    .iter()
    .any(|needle| detail.contains(needle))
}

fn is_write_access_error(error: &AppError) -> bool {
    if matches!(error, AppError::Io(io_error) if io_error.kind() == ErrorKind::PermissionDenied) {
        return true;
    }
    let detail = technical_detail(error).to_ascii_lowercase();
    [
        "access is denied",
        "permission denied",
        "read-only file system",
        "readonly filesystem",
        "operation not permitted",
        "must be an absolute folder path",
    ]
    .iter()
    .any(|needle| detail.contains(needle))
}

fn technical_detail(error: &AppError) -> String {
    match error {
        AppError::Render(RenderError::DecodeFailed { detail, .. })
        | AppError::Render(RenderError::EncodeFailed { detail, .. })
        | AppError::Render(RenderError::RendererUnavailable { detail, .. })
        | AppError::Render(RenderError::VerificationFailed { detail, .. }) => detail.clone(),
        _ => error.to_string(),
    }
}

pub fn write_diagnostic_log(
    cache_dir: &Path,
    operation: DiagnosticOperation,
    subject_path: Option<&Path>,
    presentation: &UserErrorPresentation,
) -> std::io::Result<PathBuf> {
    let log_dir = cache_dir.join("logs");
    fs::create_dir_all(&log_dir)?;
    let log_path = log_dir.join("oa-curator-diagnostics.jsonl");
    let entry = json!({
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "operation": operation.as_str(),
        "code": presentation.code.as_str(),
        "subjectPath": subject_path.map(|path| path.to_string_lossy().to_string()),
        "userMessage": presentation.message,
        "technicalDetail": presentation.technical_detail,
    });
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)?;
    writeln!(file, "{entry}")?;
    Ok(log_path)
}
