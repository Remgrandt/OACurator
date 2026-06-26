use crate::{AppError, Result};
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;
use tiff::decoder::{ifd::Value as TiffValue, Decoder as TiffDecoder};
use tiff::tags::{ResolutionUnit, Tag};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ImageMetadata {
    pub width: i64,
    pub height: i64,
    pub dpi_x: Option<f64>,
    pub dpi_y: Option<f64>,
}

pub fn read_image_metadata(path: &Path) -> Result<ImageMetadata> {
    match path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "jpg" | "jpeg" => read_jpeg_metadata(path),
        "png" => read_png_metadata(path),
        "tif" | "tiff" => read_tiff_metadata(path),
        _ => Err(AppError::Message(format!(
            "Unsupported image type: {}",
            path.display()
        ))),
    }
}

fn read_tiff_dpi(path: &Path) -> Option<(Option<f64>, Option<f64>)> {
    let file = File::open(path).ok()?;
    let mut decoder = TiffDecoder::new(BufReader::new(file)).ok()?;
    let unit = decoder
        .find_tag_unsigned::<u16>(Tag::ResolutionUnit)
        .ok()
        .flatten()
        .and_then(ResolutionUnit::from_u16)
        .unwrap_or(ResolutionUnit::Inch);
    let scale = resolution_unit_to_dpi_scale(unit)?;
    let x = decoder
        .find_tag(Tag::XResolution)
        .ok()
        .flatten()
        .and_then(tiff_value_to_f64)
        .map(|value| value * scale);
    let y = decoder
        .find_tag(Tag::YResolution)
        .ok()
        .flatten()
        .and_then(tiff_value_to_f64)
        .map(|value| value * scale);
    Some((x, y))
}

fn read_tiff_metadata(path: &Path) -> Result<ImageMetadata> {
    let file = File::open(path)?;
    let mut decoder = TiffDecoder::new(BufReader::new(file))
        .map_err(|error| AppError::Message(format!("Could not read TIFF metadata: {error}")))?;
    let (width, height) = decoder
        .dimensions()
        .map_err(|error| AppError::Message(format!("Could not read TIFF dimensions: {error}")))?;
    let (dpi_x, dpi_y) = read_tiff_dpi(path).unwrap_or((None, None));
    Ok(ImageMetadata {
        width: i64::from(width),
        height: i64::from(height),
        dpi_x,
        dpi_y,
    })
}

fn tiff_value_to_f64(value: TiffValue) -> Option<f64> {
    match value {
        TiffValue::Rational(numerator, denominator) if denominator != 0 => {
            Some(f64::from(numerator) / f64::from(denominator))
        }
        #[allow(deprecated)]
        TiffValue::RationalBig(numerator, denominator) if denominator != 0 => {
            Some(numerator as f64 / denominator as f64)
        }
        TiffValue::Unsigned(value) => Some(f64::from(value)),
        TiffValue::UnsignedBig(value) => Some(value as f64),
        TiffValue::Short(value) => Some(f64::from(value)),
        _ => None,
    }
}

fn resolution_unit_to_dpi_scale(unit: ResolutionUnit) -> Option<f64> {
    match unit {
        ResolutionUnit::Inch => Some(1.0),
        ResolutionUnit::Centimeter => Some(2.54),
        ResolutionUnit::None => None,
        _ => None,
    }
}

fn read_png_dpi(path: &Path) -> Option<(Option<f64>, Option<f64>)> {
    let mut file = File::open(path).ok()?;
    let mut signature = [0; 8];
    file.read_exact(&mut signature).ok()?;
    if signature != *b"\x89PNG\r\n\x1a\n" {
        return None;
    }

    loop {
        let mut length_bytes = [0; 4];
        file.read_exact(&mut length_bytes).ok()?;
        let length = u32::from_be_bytes(length_bytes) as usize;
        let mut chunk_type = [0; 4];
        file.read_exact(&mut chunk_type).ok()?;
        if &chunk_type == b"pHYs" {
            if length < 9 {
                return None;
            }
            let mut data = vec![0; length];
            file.read_exact(&mut data).ok()?;
            let unit = data[8];
            if unit != 1 {
                return None;
            }
            let x = u32::from_be_bytes(data[0..4].try_into().ok()?);
            let y = u32::from_be_bytes(data[4..8].try_into().ok()?);
            return Some((Some(f64::from(x) * 0.0254), Some(f64::from(y) * 0.0254)));
        }
        if &chunk_type == b"IDAT" || &chunk_type == b"IEND" {
            return None;
        }
        skip_exact(&mut file, length + 4)?;
    }
}

fn read_png_metadata(path: &Path) -> Result<ImageMetadata> {
    let mut file = File::open(path)?;
    let mut signature = [0; 8];
    file.read_exact(&mut signature)?;
    if signature != *b"\x89PNG\r\n\x1a\n" {
        return Err(AppError::Message(format!(
            "Could not read PNG metadata: invalid signature in {}",
            path.display()
        )));
    }

    let mut length_bytes = [0; 4];
    file.read_exact(&mut length_bytes)?;
    let length = u32::from_be_bytes(length_bytes);
    let mut chunk_type = [0; 4];
    file.read_exact(&mut chunk_type)?;
    if &chunk_type != b"IHDR" || length < 8 {
        return Err(AppError::Message(format!(
            "Could not read PNG metadata: missing IHDR in {}",
            path.display()
        )));
    }
    let mut dimensions = [0; 8];
    file.read_exact(&mut dimensions)?;
    let width = u32::from_be_bytes(dimensions[0..4].try_into().expect("width bytes"));
    let height = u32::from_be_bytes(dimensions[4..8].try_into().expect("height bytes"));
    let (dpi_x, dpi_y) = read_png_dpi(path).unwrap_or((None, None));
    Ok(ImageMetadata {
        width: i64::from(width),
        height: i64::from(height),
        dpi_x,
        dpi_y,
    })
}

fn read_jpeg_metadata(path: &Path) -> Result<ImageMetadata> {
    let mut file = File::open(path)?;
    let mut start = [0; 2];
    file.read_exact(&mut start)?;
    if start != [0xff, 0xd8] {
        return Err(AppError::Message(format!(
            "Could not read JPEG metadata: invalid signature in {}",
            path.display()
        )));
    }

    let mut dpi = (None, None);
    loop {
        let marker = read_jpeg_marker(&mut file).ok_or_else(|| {
            AppError::Message(format!(
                "Could not read JPEG metadata: missing marker in {}",
                path.display()
            ))
        })?;
        if marker == 0xda || marker == 0xd9 {
            return Err(AppError::Message(format!(
                "Could not read JPEG metadata: dimensions not found in {}",
                path.display()
            )));
        }
        let length = read_be_u16(&mut file).ok_or_else(|| {
            AppError::Message(format!(
                "Could not read JPEG metadata: invalid segment length in {}",
                path.display()
            ))
        })? as usize;
        if length < 2 {
            return Err(AppError::Message(format!(
                "Could not read JPEG metadata: invalid segment length in {}",
                path.display()
            )));
        }
        let data_length = length - 2;
        if marker == 0xe0 {
            let mut data = vec![0; data_length];
            file.read_exact(&mut data)?;
            if data.len() >= 14 && &data[0..5] == b"JFIF\0" {
                let unit = data[7];
                let x = u16::from_be_bytes([data[8], data[9]]);
                let y = u16::from_be_bytes([data[10], data[11]]);
                dpi = match unit {
                    1 => (Some(f64::from(x)), Some(f64::from(y))),
                    2 => (Some(f64::from(x) * 2.54), Some(f64::from(y) * 2.54)),
                    _ => (None, None),
                };
            }
            continue;
        }
        if is_jpeg_start_of_frame(marker) {
            let mut data = vec![0; data_length];
            file.read_exact(&mut data)?;
            if data.len() < 6 {
                return Err(AppError::Message(format!(
                    "Could not read JPEG metadata: short frame in {}",
                    path.display()
                )));
            }
            let height = u16::from_be_bytes([data[1], data[2]]);
            let width = u16::from_be_bytes([data[3], data[4]]);
            return Ok(ImageMetadata {
                width: i64::from(width),
                height: i64::from(height),
                dpi_x: dpi.0,
                dpi_y: dpi.1,
            });
        }
        skip_exact(&mut file, data_length).ok_or_else(|| {
            AppError::Message(format!(
                "Could not read JPEG metadata: truncated segment in {}",
                path.display()
            ))
        })?;
    }
}

fn is_jpeg_start_of_frame(marker: u8) -> bool {
    matches!(
        marker,
        0xc0 | 0xc1 | 0xc2 | 0xc3 | 0xc5 | 0xc6 | 0xc7 | 0xc9 | 0xca | 0xcb | 0xcd | 0xce | 0xcf
    )
}

fn read_jpeg_marker(file: &mut File) -> Option<u8> {
    let mut byte = [0; 1];
    loop {
        file.read_exact(&mut byte).ok()?;
        if byte[0] == 0xff {
            break;
        }
    }
    loop {
        file.read_exact(&mut byte).ok()?;
        if byte[0] != 0xff {
            return Some(byte[0]);
        }
    }
}

fn read_be_u16(file: &mut File) -> Option<u16> {
    let mut bytes = [0; 2];
    file.read_exact(&mut bytes).ok()?;
    Some(u16::from_be_bytes(bytes))
}

fn skip_exact(file: &mut File, byte_count: usize) -> Option<()> {
    let mut remaining = byte_count;
    let mut buffer = [0; 4096];
    while remaining > 0 {
        let chunk_size = remaining.min(buffer.len());
        file.read_exact(&mut buffer[..chunk_size]).ok()?;
        remaining -= chunk_size;
    }
    Some(())
}
