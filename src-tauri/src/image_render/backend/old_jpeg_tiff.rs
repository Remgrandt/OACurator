// Copyright (c) 2026 Remgrandt Works. All rights reserved.

use crate::image_render::error::RenderError;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use tiff::decoder::Decoder as TiffDecoder;
use tiff::tags::Tag;

const OLD_STYLE_JPEG_COMPRESSION: u16 = 6;
const JPEG_INTERCHANGE_FORMAT_TAG: u16 = 513;
const JPEG_INTERCHANGE_FORMAT_LENGTH_TAG: u16 = 514;

pub struct ExtractedJpegPayload {
    path: PathBuf,
}

impl ExtractedJpegPayload {
    pub fn path(&self) -> &Path {
        &self.path
    }
}

pub fn extract_standalone_jpeg_payload(
    source_path: &Path,
) -> std::result::Result<Option<ExtractedJpegPayload>, RenderError> {
    if !is_tiff_path(source_path) {
        return Ok(None);
    }
    let Some(location) = read_old_jpeg_payload_location(source_path) else {
        return Ok(None);
    };
    if !payload_bounds_are_valid(source_path, location)? {
        return Ok(None);
    }
    if !payload_has_standalone_jpeg_markers(source_path, location)? {
        return Ok(None);
    }

    let path = temporary_payload_path();
    copy_payload(source_path, &path, location)?;
    Ok(Some(ExtractedJpegPayload { path }))
}

impl Drop for ExtractedJpegPayload {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

#[derive(Clone, Copy)]
struct OldJpegPayloadLocation {
    offset: u64,
    length: u64,
}

fn read_old_jpeg_payload_location(source_path: &Path) -> Option<OldJpegPayloadLocation> {
    let file = File::open(source_path).ok()?;
    let mut decoder = TiffDecoder::new(std::io::BufReader::new(file)).ok()?;
    let compression = decoder
        .find_tag_unsigned::<u16>(Tag::Compression)
        .ok()
        .flatten()?;
    if compression != OLD_STYLE_JPEG_COMPRESSION {
        return None;
    }
    let offset = decoder
        .find_tag_unsigned::<u64>(Tag::Unknown(JPEG_INTERCHANGE_FORMAT_TAG))
        .ok()
        .flatten()?;
    let length = decoder
        .find_tag_unsigned::<u64>(Tag::Unknown(JPEG_INTERCHANGE_FORMAT_LENGTH_TAG))
        .ok()
        .flatten()?;
    Some(OldJpegPayloadLocation { offset, length })
}

fn is_tiff_path(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| matches!(extension.to_ascii_lowercase().as_str(), "tif" | "tiff"))
}

fn payload_bounds_are_valid(
    source_path: &Path,
    location: OldJpegPayloadLocation,
) -> std::result::Result<bool, RenderError> {
    if location.length < 4 {
        return Ok(false);
    }
    let Some(end) = location.offset.checked_add(location.length) else {
        return Ok(false);
    };
    let metadata = fs::metadata(source_path).map_err(|error| RenderError::DecodeFailed {
        path: source_path.to_path_buf(),
        detail: error.to_string(),
    })?;
    Ok(end <= metadata.len())
}

fn payload_has_standalone_jpeg_markers(
    source_path: &Path,
    location: OldJpegPayloadLocation,
) -> std::result::Result<bool, RenderError> {
    let mut file = File::open(source_path).map_err(|error| RenderError::DecodeFailed {
        path: source_path.to_path_buf(),
        detail: error.to_string(),
    })?;
    file.seek(SeekFrom::Start(location.offset))
        .map_err(|error| RenderError::DecodeFailed {
            path: source_path.to_path_buf(),
            detail: error.to_string(),
        })?;
    let mut start = [0; 2];
    file.read_exact(&mut start)
        .map_err(|error| RenderError::DecodeFailed {
            path: source_path.to_path_buf(),
            detail: error.to_string(),
        })?;
    file.seek(SeekFrom::Start(location.offset + location.length - 2))
        .map_err(|error| RenderError::DecodeFailed {
            path: source_path.to_path_buf(),
            detail: error.to_string(),
        })?;
    let mut end = [0; 2];
    file.read_exact(&mut end)
        .map_err(|error| RenderError::DecodeFailed {
            path: source_path.to_path_buf(),
            detail: error.to_string(),
        })?;
    Ok(start == [0xff, 0xd8] && end == [0xff, 0xd9])
}

fn copy_payload(
    source_path: &Path,
    target_path: &Path,
    location: OldJpegPayloadLocation,
) -> std::result::Result<(), RenderError> {
    let mut source = File::open(source_path).map_err(|error| RenderError::DecodeFailed {
        path: source_path.to_path_buf(),
        detail: error.to_string(),
    })?;
    source
        .seek(SeekFrom::Start(location.offset))
        .map_err(|error| RenderError::DecodeFailed {
            path: source_path.to_path_buf(),
            detail: error.to_string(),
        })?;
    let mut target = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(target_path)
        .map_err(|error| RenderError::DecodeFailed {
            path: source_path.to_path_buf(),
            detail: format!("could not create extracted JPEG payload: {error}"),
        })?;
    io::copy(&mut source.take(location.length), &mut target).map_err(|error| {
        RenderError::DecodeFailed {
            path: source_path.to_path_buf(),
            detail: format!("could not extract JPEG payload: {error}"),
        }
    })?;
    Ok(())
}

fn temporary_payload_path() -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "oac-old-jpeg-tiff-{}-{timestamp}-{counter}.jpg",
        std::process::id()
    ))
}
