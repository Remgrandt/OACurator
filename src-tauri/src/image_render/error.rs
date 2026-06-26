use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum RenderError {
    #[error("Unsupported image format: {path}")]
    UnsupportedFormat { path: PathBuf },
    #[error("Source image does not exist: {path}")]
    SourceMissing { path: PathBuf },
    #[error(
        "Source image is too large: {path} is {width}x{height}, over the {max_pixels} pixel limit"
    )]
    SourceTooLarge {
        path: PathBuf,
        width: u32,
        height: u32,
        max_pixels: u64,
    },
    #[error("Requested output is too large: {width}x{height}, over the {max_pixels} pixel limit")]
    OutputTooLarge {
        width: u32,
        height: u32,
        max_pixels: u64,
    },
    #[error("Image renderer {renderer} is unavailable: {detail}")]
    RendererUnavailable { renderer: String, detail: String },
    #[error("Could not render image from {path}: {detail}")]
    DecodeFailed { path: PathBuf, detail: String },
    #[error("Could not encode image to {path}: {detail}")]
    EncodeFailed { path: PathBuf, detail: String },
    #[error("Rendered output could not be verified at {path}: {detail}")]
    VerificationFailed { path: PathBuf, detail: String },
}
