//! Bounded ZIP reads. Verify actual Deflate output rather than trusting ZIP sizes.
use super::OaaValidationLimits;
use flate2::{Decompress, FlushDecompress, Status};
use std::{
    collections::BTreeSet,
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::Path,
};
use zip::{CompressionMethod, ZipArchive};

#[derive(Debug)]
pub(super) enum ReadError {
    Capacity,
    Unsupported,
    Malformed,
    Io,
}
impl From<std::io::Error> for ReadError {
    fn from(error: std::io::Error) -> Self {
        if error.kind() == std::io::ErrorKind::UnexpectedEof {
            Self::Malformed
        } else {
            Self::Io
        }
    }
}

fn u16_at(bytes: &[u8], n: usize) -> u16 {
    u16::from_le_bytes(bytes[n..n + 2].try_into().unwrap())
}
fn u32_at(bytes: &[u8], n: usize) -> u32 {
    u32::from_le_bytes(bytes[n..n + 4].try_into().unwrap())
}
fn u64_at(bytes: &[u8], n: usize) -> u64 {
    u64::from_le_bytes(bytes[n..n + 8].try_into().unwrap())
}

/// Bound the central directory before ZipArchive allocates it, including ZIP64.
pub(super) fn preflight(
    path: &Path,
    limits: OaaValidationLimits,
) -> Result<BTreeSet<String>, ReadError> {
    let mut file = File::open(path)?;
    let length = file.metadata()?.len();
    if length > limits.max_archive_size {
        return Err(ReadError::Capacity);
    }
    let tail_len = length.min(65557);
    file.seek(SeekFrom::End(-(tail_len as i64)))?;
    let mut tail = vec![0; tail_len as usize];
    file.read_exact(&mut tail)?;
    let end = (0..tail.len().saturating_sub(21))
        .rev()
        .find(|&n| {
            tail[n..].starts_with(b"PK\x05\x06")
                && n + 22 + u16_at(&tail, n + 20) as usize == tail.len()
        })
        .ok_or(ReadError::Malformed)?;
    if u16_at(&tail, end + 4) != 0 || u16_at(&tail, end + 6) != 0 {
        return Err(ReadError::Unsupported);
    }
    let mut count = u16_at(&tail, end + 10) as u64;
    let mut size = u32_at(&tail, end + 12) as u64;
    let mut central_end = length - tail_len + end as u64;
    let mut locator = [0; 20];
    if central_end >= 20 {
        file.seek(SeekFrom::Start(central_end - 20))?;
        file.read_exact(&mut locator)?;
    }
    if &locator[..4] == b"PK\x06\x07"
        || count == 65535
        || size == u32::MAX as u64
        || u32_at(&tail, end + 16) == u32::MAX
    {
        if central_end < 20 {
            return Err(ReadError::Malformed);
        }
        if &locator[..4] != b"PK\x06\x07" || u32_at(&locator, 4) != 0 || u32_at(&locator, 16) != 1 {
            return Err(ReadError::Malformed);
        }
        let offset = u64_at(&locator, 8);
        file.seek(SeekFrom::Start(offset))?;
        let mut record = [0; 56];
        file.read_exact(&mut record)?;
        if &record[..4] != b"PK\x06\x06" || u32_at(&record, 16) != 0 || u32_at(&record, 20) != 0 {
            return Err(ReadError::Malformed);
        }
        count = u64_at(&record, 32);
        size = u64_at(&record, 40);
        central_end = offset;
    }
    if count > limits.max_entries as u64 || size > limits.max_directory_size {
        return Err(ReadError::Capacity);
    }
    let start = central_end.checked_sub(size).ok_or(ReadError::Malformed)?;
    file.seek(SeekFrom::Start(start))?;
    let mut names = BTreeSet::new();
    let mut duplicates = BTreeSet::new();
    let mut actual = 0;
    while file.stream_position()? < central_end {
        actual += 1;
        if actual > limits.max_entries {
            return Err(ReadError::Capacity);
        }
        let mut header = [0; 46];
        file.read_exact(&mut header)?;
        if &header[..4] != b"PK\x01\x02" {
            return Err(ReadError::Malformed);
        }
        let mut name = vec![0; u16_at(&header, 28) as usize];
        file.read_exact(&mut name)?;
        if !names.insert(name.clone()) {
            duplicates.insert(String::from_utf8_lossy(&name).into_owned());
        }
        file.seek(SeekFrom::Current(
            i64::from(u16_at(&header, 30)) + i64::from(u16_at(&header, 32)),
        ))?;
    }
    if actual as u64 != count || file.stream_position()? != central_end {
        return Err(ReadError::Malformed);
    }
    Ok(duplicates)
}

pub(super) fn read_entry(
    zip: &mut ZipArchive<File>,
    path: &Path,
    index: usize,
    limit: u64,
    collect: bool,
) -> Result<(u64, Vec<u8>), ReadError> {
    let entry = zip.by_index_raw(index).map_err(|_| ReadError::Malformed)?;
    if entry.encrypted() {
        return Err(ReadError::Malformed);
    }
    let (start, compressed, expected, crc, method) = (
        entry.data_start(),
        entry.compressed_size(),
        entry.size(),
        entry.crc32(),
        entry.compression(),
    );
    if expected > limit {
        return Err(ReadError::Capacity);
    }
    if !matches!(
        method,
        CompressionMethod::Stored | CompressionMethod::Deflated
    ) {
        return Err(ReadError::Unsupported);
    }
    // Also check local headers: ZipArchive exposes raw bytes without decompression.
    let local = entry.header_start();
    let name = entry.name_raw().to_vec();
    drop(entry);
    if start
        .checked_add(compressed)
        .is_none_or(|end| end > zip.central_directory_start())
    {
        return Err(ReadError::Malformed);
    }
    let mut input = File::open(path)?;
    input.seek(SeekFrom::Start(local))?;
    let mut header = [0; 30];
    input.read_exact(&mut header)?;
    if &header[..4] != b"PK\x03\x04" || u16_at(&header, 6) & 1 != 0 {
        return Err(ReadError::Malformed);
    }
    let mut local_name = vec![0; u16_at(&header, 26) as usize];
    input.read_exact(&mut local_name)?;
    if local_name != name
        || u16_at(&header, 8)
            != if method == CompressionMethod::Stored {
                0
            } else {
                8
            }
        || (u16_at(&header, 6) & 8 == 0 && u32_at(&header, 14) != crc)
    {
        return Err(ReadError::Malformed);
    }
    input.seek(SeekFrom::Start(start))?;
    let mut input = input.take(compressed);
    let mut buffer = [0; 65536];
    let mut output = [0; 65536];
    let mut decoder = Decompress::new(false);
    let mut total = 0u64;
    let mut hash = crc32fast::Hasher::new();
    let mut bytes = Vec::new();
    let mut ended = false;
    loop {
        let len = input.read(&mut buffer)?;
        if len == 0 {
            break;
        }
        if ended {
            return Err(ReadError::Malformed);
        }
        let mut pending = &buffer[..len];
        loop {
            let (used, written, end) = if method == CompressionMethod::Stored {
                output[..pending.len()].copy_from_slice(pending);
                (pending.len(), pending.len(), false)
            } else {
                let before_in = decoder.total_in();
                let before_out = decoder.total_out();
                let status = decoder
                    .decompress(pending, &mut output, FlushDecompress::None)
                    .map_err(|_| ReadError::Malformed)?;
                (
                    (decoder.total_in() - before_in) as usize,
                    (decoder.total_out() - before_out) as usize,
                    status == Status::StreamEnd,
                )
            };
            total = total
                .checked_add(written as u64)
                .ok_or(ReadError::Capacity)?;
            if total > limit {
                return Err(ReadError::Capacity);
            }
            hash.update(&output[..written]);
            if collect {
                bytes.extend_from_slice(&output[..written]);
            }
            pending = &pending[used..];
            if end {
                ended = true;
                if !pending.is_empty() || input.limit() != 0 {
                    return Err(ReadError::Malformed);
                }
                break;
            }
            if method == CompressionMethod::Stored || (pending.is_empty() && written < output.len())
            {
                break;
            }
            if used == 0 && written == 0 {
                return Err(ReadError::Malformed);
            }
        }
    }
    if input.limit() != 0
        || (method == CompressionMethod::Deflated && !ended)
        || total != expected
        || hash.finalize() != crc
    {
        return Err(ReadError::Malformed);
    }
    Ok((total, bytes))
}
