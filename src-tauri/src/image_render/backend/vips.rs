// Copyright (c) 2026 Remgrandt Works. All rights reserved.

use crate::image_metadata::read_image_metadata;
use crate::image_render::backend::BackendRenderResult;
use crate::image_render::error::RenderError;
use crate::image_render::recipe::{AlphaPolicy, OutputFormat};
use crate::image_render::scheduler::RenderPlan;
use crate::image_render::service::RenderRequest;
use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

pub fn render(
    request: &RenderRequest,
    plan: &RenderPlan,
) -> std::result::Result<BackendRenderResult, RenderError> {
    let vips = find_vips_tool("vips").ok_or_else(|| RenderError::RendererUnavailable {
        renderer: "libvips".to_string(),
        detail: "could not find vips executable; set OACURATOR_VIPS_BIN_DIR or bundle libvips"
            .to_string(),
    })?;
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
    let output_arg = output_argument(&render_path, &output_format);
    let output = vips_command(&vips)
        .arg("thumbnail")
        .arg(&request.source_path)
        .arg(&output_arg)
        .arg(plan.target_width.to_string())
        .arg("--height")
        .arg(plan.target_height.to_string())
        .arg("--size")
        .arg("down")
        .output()
        .map_err(|error| RenderError::RendererUnavailable {
            renderer: "libvips".to_string(),
            detail: error.to_string(),
        })?;
    if !output.status.success() {
        return Err(RenderError::DecodeFailed {
            path: request.source_path.clone(),
            detail: command_output_detail(&output),
        });
    }
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
        renderer: "libvips-cli".to_string(),
        renderer_version: vips_version(&vips),
        renderer_options_json: serde_json::json!({
            "tool": vips,
            "operation": "thumbnail",
            "size": "down",
            "autorotate": true
        })
        .to_string(),
    })
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

fn output_argument(path: &Path, output_format: &OutputFormat) -> OsString {
    match output_format {
        OutputFormat::Png => path.as_os_str().to_os_string(),
        OutputFormat::Jpeg { quality } => {
            let mut value = path.to_string_lossy().to_string();
            value.push_str(&format!("[Q={quality}]"));
            OsString::from(value)
        }
    }
}

fn source_has_transparency(path: &Path) -> bool {
    let Some(vipsheader) = find_vips_tool("vipsheader") else {
        return false;
    };
    let Some(bands) = vipsheader_value(&vipsheader, "bands", path)
        .and_then(|value| value.trim().parse::<i32>().ok())
    else {
        return false;
    };
    if !matches!(bands, 2 | 4) {
        return false;
    }
    let Some(opaque_alpha_value) = vipsheader_value(&vipsheader, "format", path)
        .as_deref()
        .and_then(opaque_alpha_value_for_format)
    else {
        return true;
    };
    alpha_min(path, bands - 1)
        .map(|minimum| minimum < opaque_alpha_value)
        .unwrap_or(true)
}

fn source_may_have_alpha(path: &Path) -> bool {
    !path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| matches!(extension.to_ascii_lowercase().as_str(), "jpg" | "jpeg"))
}

fn vipsheader_value(vipsheader: &Path, field: &str, path: &Path) -> Option<String> {
    let output = vips_command(vipsheader)
        .arg("-f")
        .arg(field)
        .arg(path)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn opaque_alpha_value_for_format(format: &str) -> Option<f64> {
    let normalized = format.to_ascii_uppercase();
    if normalized.contains("UCHAR") {
        Some(255.0)
    } else if normalized.contains("USHORT") {
        Some(65_535.0)
    } else if normalized.contains("UINT") {
        Some(4_294_967_295.0)
    } else if normalized.contains("FLOAT") || normalized.contains("DOUBLE") {
        Some(1.0)
    } else {
        None
    }
}

fn alpha_min(path: &Path, alpha_band: i32) -> Option<f64> {
    let vips = find_vips_tool("vips")?;
    let temporary_path = env::temp_dir().join(format!(
        "oac-alpha-{}-{}.v",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .ok()?
            .as_nanos()
    ));
    let extract_output = vips_command(&vips)
        .arg("extract_band")
        .arg(path)
        .arg(&temporary_path)
        .arg(alpha_band.to_string())
        .output()
        .ok()?;
    if !extract_output.status.success() {
        let _ = fs::remove_file(&temporary_path);
        return None;
    }
    let min_output = vips_command(&vips)
        .arg("min")
        .arg(&temporary_path)
        .output()
        .ok()?;
    let _ = fs::remove_file(&temporary_path);
    if !min_output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&min_output.stdout)
        .trim()
        .parse::<f64>()
        .ok()
}

fn vips_version(vips: &Path) -> String {
    static VERSION: OnceLock<String> = OnceLock::new();
    VERSION.get_or_init(|| query_vips_version(vips)).clone()
}

fn query_vips_version(vips: &Path) -> String {
    if let Ok(output) = vips_command(vips).arg("--version").output() {
        if output.status.success() {
            let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !value.is_empty() {
                return value;
            }
        }
    }
    "unknown".to_string()
}

fn vips_command(tool: &Path) -> Command {
    let mut command = Command::new(tool);
    configure_vips_runtime_environment(&mut command, tool);
    command
}

#[cfg(target_os = "macos")]
fn configure_vips_runtime_environment(command: &mut Command, tool: &Path) {
    if let Some(libvips_dir) = tool.parent() {
        command.env(
            "DYLD_LIBRARY_PATH",
            prepend_env_paths(
                vips_library_paths(libvips_dir),
                env::var_os("DYLD_LIBRARY_PATH"),
            ),
        );
        if let Some(module_dir) = find_vips_module_dir(libvips_dir) {
            command.env("VIPS_MODULEDIR", module_dir);
        }
    }
}

#[cfg(not(target_os = "macos"))]
fn configure_vips_runtime_environment(_command: &mut Command, _tool: &Path) {}

#[cfg(target_os = "macos")]
fn vips_library_paths(libvips_dir: &Path) -> Vec<PathBuf> {
    let mut paths = vec![libvips_dir.to_path_buf()];
    let lib_dir = libvips_dir.join("lib");
    if lib_dir.is_dir() {
        paths.insert(0, lib_dir);
    }
    paths
}

#[cfg(target_os = "macos")]
fn prepend_env_paths(mut paths: Vec<PathBuf>, current: Option<OsString>) -> OsString {
    if let Some(current) = current {
        paths.extend(env::split_paths(&current));
    }
    env::join_paths(&paths).unwrap_or_else(|_| {
        paths
            .first()
            .map(|path| path.as_os_str().to_os_string())
            .unwrap_or_default()
    })
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

fn find_vips_tool(tool: &str) -> Option<PathBuf> {
    let executable = executable_name(tool);
    if let Some(dir) = env::var_os("OACURATOR_VIPS_BIN_DIR")
        .map(PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty())
    {
        let candidate = dir.join(&executable);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    if let Ok(current_exe) = env::current_exe() {
        if let Some(exe_dir) = current_exe.parent() {
            let candidate = exe_dir.join(&executable);
            if candidate.is_file() {
                return Some(candidate);
            }
            let resources_candidate = exe_dir.join("resources").join("libvips").join(&executable);
            if resources_candidate.is_file() {
                return Some(resources_candidate);
            }
            for ancestor in exe_dir.ancestors().take(6) {
                let source_resource_candidate =
                    ancestor.join("resources").join("libvips").join(&executable);
                if source_resource_candidate.is_file() {
                    return Some(source_resource_candidate);
                }
            }
            let macos_resources_candidate = exe_dir.parent().map(|contents_dir| {
                contents_dir
                    .join("Resources")
                    .join("libvips")
                    .join(&executable)
            });
            if let Some(candidate) = macos_resources_candidate {
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
            let macos_tauri_resource_candidate = exe_dir.parent().map(|contents_dir| {
                contents_dir
                    .join("Resources")
                    .join("resources")
                    .join("libvips")
                    .join(&executable)
            });
            if let Some(candidate) = macos_tauri_resource_candidate {
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
    }
    Some(PathBuf::from(executable))
}

fn executable_name(tool: &str) -> String {
    if cfg!(windows) {
        format!("{tool}.exe")
    } else {
        tool.to_string()
    }
}

fn command_output_detail(output: &std::process::Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if !stderr.is_empty() {
        return stderr;
    }
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if !stdout.is_empty() {
        return stdout;
    }
    format!("libvips exited with {}", output.status)
}
