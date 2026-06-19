// Copyright (c) 2026 Remgrandt Works. All rights reserved.

use super::AppState;
use base64::Engine;
use std::path::{Path, PathBuf};
use std::process::Command;

#[tauri::command]
pub fn show_path_in_file_manager_command(path: String) -> std::result::Result<(), String> {
    let path = PathBuf::from(path);
    if !path.exists() {
        return Err(format!("Path does not exist: {}", path.display()));
    }

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;

        let mut command = Command::new("explorer.exe");
        if path.is_file() {
            command.raw_arg(windows_explorer_select_raw_arg(&path));
        } else {
            command.arg(&path);
        }
        command.spawn().map_err(|error| error.to_string())?;
    }

    #[cfg(target_os = "macos")]
    {
        let mut command = Command::new("open");
        if path.is_file() {
            command.arg("-R").arg(&path);
        } else {
            command.arg(&path);
        }
        command.spawn().map_err(|error| error.to_string())?;
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let target = if path.is_file() {
            path.parent().unwrap_or_else(|| Path::new("/"))
        } else {
            path.as_path()
        };
        Command::new("xdg-open")
            .arg(target)
            .spawn()
            .map_err(|error| error.to_string())?;
    }

    Ok(())
}

#[cfg(target_os = "windows")]
fn windows_explorer_select_raw_arg(path: &Path) -> String {
    format!("/select,\"{}\"", path.display())
}

const CACHE_IMAGE_DATA_URL_MAX_BYTES: u64 = 32 * 1024 * 1024;
const ASSET_IMAGE_DATA_URL_MAX_BYTES: u64 = 32 * 1024 * 1024;

#[tauri::command]
pub fn cache_image_data_url_command(
    state: tauri::State<'_, AppState>,
    path: String,
) -> std::result::Result<String, String> {
    cache_image_data_url_for_path(&PathBuf::from(path), &state.cache_dir)
}

#[tauri::command]
pub fn file_asset_image_data_url_command(
    state: tauri::State<'_, AppState>,
    file_asset_id: i64,
) -> std::result::Result<String, String> {
    let asset = state
        .catalog
        .file_asset(file_asset_id)
        .map_err(|error| error.to_string())?;
    read_preview_image_data_url_with_limit(&asset.current_path, ASSET_IMAGE_DATA_URL_MAX_BYTES)
}

#[tauri::command]
pub fn derived_asset_image_data_url_command(
    state: tauri::State<'_, AppState>,
    derived_asset_id: i64,
) -> std::result::Result<String, String> {
    let asset = state
        .catalog
        .derived_asset(derived_asset_id)
        .map_err(|error| error.to_string())?;
    read_preview_image_data_url_with_limit(&asset.path, ASSET_IMAGE_DATA_URL_MAX_BYTES)
}

fn cache_image_data_url_for_path(
    path: &Path,
    cache_dir: &Path,
) -> std::result::Result<String, String> {
    let canonical_cache_dir = cache_dir.canonicalize().map_err(|error| {
        format!(
            "Could not resolve app-managed cache folder {}: {error}",
            cache_dir.display()
        )
    })?;
    let canonical_path = path.canonicalize().map_err(|error| error.to_string())?;
    if !canonical_path.starts_with(&canonical_cache_dir) {
        return Err(format!(
            "Image preview path is outside the app-managed cache: {}",
            path.display()
        ));
    }
    read_preview_image_data_url_with_limit(&canonical_path, CACHE_IMAGE_DATA_URL_MAX_BYTES)
}

fn read_preview_image_data_url_with_limit(
    path: &Path,
    max_bytes: u64,
) -> std::result::Result<String, String> {
    if !path_has_previewable_image_extension(path) {
        return Err(format!(
            "Unsupported image file for preview: {}",
            path.display()
        ));
    }
    let metadata = std::fs::metadata(path).map_err(|error| error.to_string())?;
    if metadata.len() > max_bytes {
        return Err(format!(
            "Image file is too large to preview safely: {}",
            path.display()
        ));
    }
    let bytes = std::fs::read(path).map_err(|error| error.to_string())?;
    let image_format = image::guess_format(&bytes)
        .map_err(|_| format!("Unsupported image file for preview: {}", path.display()))?;
    let mime = match image_format {
        image::ImageFormat::Jpeg => "image/jpeg",
        image::ImageFormat::Png => "image/png",
        image::ImageFormat::Tiff => "image/tiff",
        _ => {
            return Err(format!(
                "Unsupported image file for preview: {}",
                path.display()
            ))
        }
    };
    let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
    Ok(format!("data:{mime};base64,{encoded}"))
}

fn path_has_previewable_image_extension(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|extension| extension.to_str())
            .map(|extension| extension.to_ascii_lowercase())
            .as_deref(),
        Some("jpg" | "jpeg" | "png" | "tif" | "tiff")
    )
}

#[cfg(test)]
mod tests {
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn cache_image_data_url_rejects_non_cache_paths() {
        let dir = TempDir::new().unwrap();
        let cache_dir = dir.path().join("cache");
        fs::create_dir_all(&cache_dir).unwrap();
        let outside_path = dir.path().join("outside.png");
        fs::write(&outside_path, b"not a real png").unwrap();

        let result = super::cache_image_data_url_for_path(&outside_path, &cache_dir);

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .contains("outside the app-managed cache"));
    }

    #[test]
    fn image_data_url_reader_rejects_oversized_files_before_reading() {
        let dir = TempDir::new().unwrap();
        let image_path = dir.path().join("huge.png");
        fs::write(&image_path, vec![0; 4]).unwrap();

        let result = super::read_preview_image_data_url_with_limit(&image_path, 3);

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("too large to preview"));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_explorer_select_arg_quotes_only_the_path_after_the_comma() {
        let path = std::path::Path::new(
            r"C:\Collections\Example Collection\artworks\OAC-00001\scan one.jpg",
        );

        assert_eq!(
            super::windows_explorer_select_raw_arg(path),
            r#"/select,"C:\Collections\Example Collection\artworks\OAC-00001\scan one.jpg""#
        );
    }
}
