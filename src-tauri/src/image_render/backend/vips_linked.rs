// Copyright (c) 2026 Remgrandt Works. All rights reserved.

use crate::image_metadata::read_image_metadata;
use crate::image_render::backend::BackendRenderResult;
use crate::image_render::error::RenderError;
use crate::image_render::recipe::{AlphaPolicy, OutputFormat};
use crate::image_render::scheduler::RenderPlan;
use crate::image_render::service::RenderRequest;
use std::env;
use std::ffi::{CStr, CString};
use std::fs;
use std::os::raw::{c_char, c_double, c_int, c_void};
use std::path::{Path, PathBuf};
use std::ptr;
use std::sync::OnceLock;

const VIPS_SIZE_DOWN: c_int = 2;
const VIPS_FORMAT_UCHAR: c_int = 0;
const VIPS_FORMAT_USHORT: c_int = 2;
const VIPS_FORMAT_UINT: c_int = 4;
const VIPS_FORMAT_FLOAT: c_int = 6;
const VIPS_FORMAT_DOUBLE: c_int = 8;

pub fn renderer_name() -> &'static str {
    "libvips-linked"
}

pub fn render(
    request: &RenderRequest,
    plan: &RenderPlan,
) -> std::result::Result<BackendRenderResult, RenderError> {
    linked_vips_runtime()?;
    linked_vips_error_clear();

    let needs_alpha_probe = matches!(request.recipe.output, OutputFormat::Jpeg { .. })
        && matches!(request.recipe.alpha, AlphaPolicy::Preserve);
    let has_alpha = needs_alpha_probe
        && source_may_have_alpha(&request.source_path)
        && source_has_transparency(&request.source_path);
    let output_format = match (&request.recipe.output, &request.recipe.alpha) {
        (OutputFormat::Jpeg { .. }, AlphaPolicy::Preserve) if has_alpha => OutputFormat::Png,
        (format, _) => format.clone(),
    };

    let render_path = path_with_output_extension(&request.destination_path, &output_format);
    let source_path =
        path_to_c_string(&request.source_path).map_err(|detail| RenderError::DecodeFailed {
            path: request.source_path.clone(),
            detail,
        })?;

    let height_name = c_string_literal("height");
    let size_name = c_string_literal("size");
    let mut rendered: *mut ffi::VipsImage = ptr::null_mut();
    let status = unsafe {
        ffi::vips_thumbnail(
            source_path.as_ptr(),
            &mut rendered,
            plan.target_width as c_int,
            height_name.as_ptr(),
            plan.target_height as c_int,
            size_name.as_ptr(),
            VIPS_SIZE_DOWN,
            ptr::null::<c_void>(),
        )
    };
    if status != 0 || rendered.is_null() {
        return Err(RenderError::DecodeFailed {
            path: request.source_path.clone(),
            detail: linked_vips_error_detail("thumbnail"),
        });
    }
    let rendered = LinkedVipsImage::new(rendered);

    save_rendered_image(&rendered, &render_path, &output_format)?;

    let output_metadata =
        read_image_metadata(&render_path).map_err(|error| RenderError::VerificationFailed {
            path: render_path.clone(),
            detail: error.to_string(),
        })?;

    if render_path != request.destination_path {
        if request.destination_path.exists() {
            fs::remove_file(&request.destination_path).map_err(|error| {
                RenderError::VerificationFailed {
                    path: request.destination_path.clone(),
                    detail: error.to_string(),
                }
            })?;
        }
        fs::rename(&render_path, &request.destination_path).map_err(|error| {
            RenderError::VerificationFailed {
                path: request.destination_path.clone(),
                detail: error.to_string(),
            }
        })?;
    }

    Ok(BackendRenderResult {
        width: output_metadata.width as u32,
        height: output_metadata.height as u32,
        format: output_format.format_name().to_string(),
        renderer: renderer_name().to_string(),
        renderer_version: linked_vips_version(),
        renderer_options_json: serde_json::json!({
            "backend": "linked",
            "operation": "thumbnail",
            "size": "down",
            "autorotate": true
        })
        .to_string(),
    })
}

fn linked_vips_runtime() -> std::result::Result<(), RenderError> {
    static VIPS_INIT: OnceLock<std::result::Result<(), String>> = OnceLock::new();
    VIPS_INIT
        .get_or_init(|| {
            configure_linked_vips_runtime_environment()?;
            let app_name = c_string_literal("OA Curator");
            let status = unsafe { ffi::vips_init(app_name.as_ptr()) };
            if status == 0 {
                Ok(())
            } else {
                Err(linked_vips_error_detail("vips_init"))
            }
        })
        .clone()
        .map_err(|detail| RenderError::RendererUnavailable {
            renderer: renderer_name().to_string(),
            detail,
        })
}

fn save_rendered_image(
    image: &LinkedVipsImage,
    path: &Path,
    output_format: &OutputFormat,
) -> std::result::Result<(), RenderError> {
    let output_path = path_to_vips_write_string(path, output_format).map_err(|detail| {
        RenderError::EncodeFailed {
            path: path.to_path_buf(),
            detail,
        }
    })?;
    let status = unsafe {
        ffi::vips_image_write_to_file(image.as_ptr(), output_path.as_ptr(), ptr::null::<c_void>())
    };
    if status == 0 {
        Ok(())
    } else {
        Err(RenderError::EncodeFailed {
            path: path.to_path_buf(),
            detail: linked_vips_error_detail("image_write_to_file"),
        })
    }
}

fn path_to_vips_write_string(
    path: &Path,
    output_format: &OutputFormat,
) -> std::result::Result<CString, String> {
    let mut path = path.to_string_lossy().to_string();
    if let OutputFormat::Jpeg { quality } = output_format {
        path.push_str(&format!("[Q={quality}]"));
    }
    let display_path = path.clone();

    CString::new(path.into_bytes())
        .map_err(|_| format!("path contains an interior NUL byte: {display_path}"))
}

fn source_has_transparency(path: &Path) -> bool {
    let Ok(path) = path_to_c_string(path) else {
        return false;
    };
    let image = unsafe { ffi::vips_image_new_from_file(path.as_ptr(), ptr::null::<c_void>()) };
    if image.is_null() {
        linked_vips_error_clear();
        return false;
    }
    let image = LinkedVipsImage::new(image);
    let bands = unsafe { ffi::vips_image_get_bands(image.as_ptr()) };
    if !matches!(bands, 2 | 4) {
        return false;
    }
    let Some(opaque_alpha_value) =
        opaque_alpha_value_for_format(unsafe { ffi::vips_image_get_format(image.as_ptr()) })
    else {
        return true;
    };

    let mut alpha: *mut ffi::VipsImage = ptr::null_mut();
    let status = unsafe {
        ffi::vips_extract_band(image.as_ptr(), &mut alpha, bands - 1, ptr::null::<c_void>())
    };
    if status != 0 || alpha.is_null() {
        linked_vips_error_clear();
        return true;
    }
    let alpha = LinkedVipsImage::new(alpha);
    let mut minimum = 0.0;
    let status = unsafe { ffi::vips_min(alpha.as_ptr(), &mut minimum, ptr::null::<c_void>()) };
    if status != 0 {
        linked_vips_error_clear();
        return true;
    }
    minimum < opaque_alpha_value
}

fn source_may_have_alpha(path: &Path) -> bool {
    !path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| matches!(extension.to_ascii_lowercase().as_str(), "jpg" | "jpeg"))
}

fn opaque_alpha_value_for_format(format: c_int) -> Option<f64> {
    match format {
        VIPS_FORMAT_UCHAR => Some(255.0),
        VIPS_FORMAT_USHORT => Some(65_535.0),
        VIPS_FORMAT_UINT => Some(4_294_967_295.0),
        VIPS_FORMAT_FLOAT | VIPS_FORMAT_DOUBLE => Some(1.0),
        _ => None,
    }
}

fn path_with_output_extension(path: &Path, output_format: &OutputFormat) -> PathBuf {
    let expected = output_format.extension();
    if path
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case(expected))
    {
        return path.to_path_buf();
    }
    let mut value = path.as_os_str().to_os_string();
    value.push(".");
    value.push(expected);
    PathBuf::from(value)
}

fn path_to_c_string(path: &Path) -> std::result::Result<CString, String> {
    CString::new(path.to_string_lossy().as_bytes())
        .map_err(|_| format!("path contains an interior NUL byte: {}", path.display()))
}

fn c_string_literal(value: &str) -> CString {
    CString::new(value).expect("static libvips string must not contain NUL")
}

fn linked_vips_version() -> String {
    let version = unsafe { ffi::vips_version_string() };
    if version.is_null() {
        "unknown".to_string()
    } else {
        unsafe { CStr::from_ptr(version) }
            .to_string_lossy()
            .to_string()
    }
}

fn linked_vips_error_detail(operation: &str) -> String {
    let buffer = linked_vips_error_buffer();
    if buffer.is_empty() {
        format!("{operation} failed")
    } else {
        format!("{operation}: {buffer}")
    }
}

fn linked_vips_error_buffer() -> String {
    let buffer = unsafe { ffi::vips_error_buffer() };
    if buffer.is_null() {
        return String::new();
    }
    unsafe { CStr::from_ptr(buffer) }
        .to_string_lossy()
        .trim()
        .to_string()
}

fn linked_vips_error_clear() {
    unsafe { ffi::vips_error_clear() }
}

struct LinkedVipsImage {
    image: *mut ffi::VipsImage,
}

impl LinkedVipsImage {
    fn new(image: *mut ffi::VipsImage) -> Self {
        Self { image }
    }

    fn as_ptr(&self) -> *mut ffi::VipsImage {
        self.image
    }
}

impl Drop for LinkedVipsImage {
    fn drop(&mut self) {
        if !self.image.is_null() {
            unsafe { ffi::g_object_unref(self.image.cast::<c_void>()) }
        }
    }
}

#[cfg(target_os = "windows")]
fn configure_linked_vips_runtime_environment() -> std::result::Result<(), String> {
    let Some(libvips_dir) = find_bundled_vips_dir() else {
        return Err(
            "could not find bundled libvips directory; run npm run setup:libvips:windows"
                .to_string(),
        );
    };
    prepend_env_path("PATH", vec![libvips_dir.clone()]);
    set_windows_dll_directory(&libvips_dir)?;
    Ok(())
}

#[cfg(target_os = "macos")]
fn configure_linked_vips_runtime_environment() -> std::result::Result<(), String> {
    let Some(libvips_dir) = find_bundled_vips_dir() else {
        return Err(
            "could not find bundled libvips directory; run npm run setup:libvips:macos".to_string(),
        );
    };
    if let Some(module_dir) = find_vips_module_dir(&libvips_dir) {
        env::set_var("VIPS_MODULEDIR", module_dir);
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn set_windows_dll_directory(path: &Path) -> std::result::Result<(), String> {
    use std::os::windows::ffi::OsStrExt;

    unsafe extern "system" {
        fn SetDllDirectoryW(lpPathName: *const u16) -> i32;
    }

    let wide_path: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let result = unsafe { SetDllDirectoryW(wide_path.as_ptr()) };
    if result == 0 {
        Err(format!(
            "could not add libvips DLL directory to the Windows loader search path: {}",
            path.display()
        ))
    } else {
        Ok(())
    }
}

fn find_bundled_vips_dir() -> Option<PathBuf> {
    if let Some(dir) = env::var_os("OACURATOR_VIPS_BIN_DIR")
        .map(PathBuf::from)
        .filter(|path| path.is_dir())
    {
        return Some(dir);
    }

    let current_exe = env::current_exe().ok()?;
    let exe_dir = current_exe.parent()?;
    for ancestor in exe_dir.ancestors().take(6) {
        let source_resource_candidate = ancestor.join("resources").join("libvips");
        if source_resource_candidate.is_dir() {
            return Some(source_resource_candidate);
        }
    }

    let macos_resources_candidate = exe_dir.parent().map(|contents_dir| {
        contents_dir
            .join("Resources")
            .join("resources")
            .join("libvips")
    });
    if let Some(candidate) = macos_resources_candidate {
        if candidate.is_dir() {
            return Some(candidate);
        }
    }

    None
}

#[cfg(target_os = "macos")]
fn find_vips_module_dir(libvips_dir: &Path) -> Option<PathBuf> {
    fs::read_dir(libvips_dir).ok()?.find_map(|entry| {
        let entry = entry.ok()?;
        let path = entry.path();
        let name = path.file_name()?.to_str()?;
        if path.is_dir() && name.starts_with("vips-modules-") {
            Some(path)
        } else {
            None
        }
    })
}

#[cfg(target_os = "windows")]
fn prepend_env_path(name: &str, paths: Vec<PathBuf>) {
    let Some(value) = prepend_env_paths(paths, env::var_os(name)) else {
        return;
    };
    env::set_var(name, value);
}

#[cfg(target_os = "windows")]
fn prepend_env_paths(
    mut paths: Vec<PathBuf>,
    current: Option<std::ffi::OsString>,
) -> Option<std::ffi::OsString> {
    if let Some(current) = current {
        paths.extend(env::split_paths(&current));
    }
    env::join_paths(paths).ok()
}

mod ffi {
    use super::{c_char, c_double, c_int, c_void};

    #[repr(C)]
    pub struct VipsImage {
        _private: [u8; 0],
    }

    #[cfg_attr(target_os = "macos", link(name = "vips-cpp"))]
    #[cfg_attr(not(target_os = "macos"), link(name = "vips"))]
    #[cfg_attr(not(target_os = "macos"), link(name = "glib-2.0"))]
    #[cfg_attr(not(target_os = "macos"), link(name = "gobject-2.0"))]
    unsafe extern "C" {
        pub fn vips_init(argv0: *const c_char) -> c_int;
        pub fn vips_version_string() -> *const c_char;
        pub fn vips_error_buffer() -> *const c_char;
        pub fn vips_error_clear();
        pub fn vips_thumbnail(
            filename: *const c_char,
            out: *mut *mut VipsImage,
            width: c_int,
            ...
        ) -> c_int;
        pub fn vips_image_write_to_file(in_: *mut VipsImage, filename: *const c_char, ...)
            -> c_int;
        pub fn vips_image_new_from_file(name: *const c_char, ...) -> *mut VipsImage;
        pub fn vips_image_get_bands(image: *const VipsImage) -> c_int;
        pub fn vips_image_get_format(image: *const VipsImage) -> c_int;
        pub fn vips_extract_band(
            in_: *mut VipsImage,
            out: *mut *mut VipsImage,
            band: c_int,
            ...
        ) -> c_int;
        pub fn vips_min(in_: *mut VipsImage, out: *mut c_double, ...) -> c_int;
        pub fn g_object_unref(object: *mut c_void);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn linked_renderer_reports_in_process_backend_name() {
        assert_eq!(super::renderer_name(), "libvips-linked");
    }
}
