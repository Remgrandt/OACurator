use std::fs;
use std::path::{Path, PathBuf};

use crate::{AppError, Result};

const WINDOWS_RESERVED_NAMES: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

pub fn validate_file_name_component(value: &str) -> Result<String> {
    let name = value.trim();
    if name.is_empty() {
        return Err(AppError::Message("File name is required".to_string()));
    }
    if name.ends_with(' ') || name.ends_with('.') {
        return Err(AppError::Message(format!(
            "Unsafe trailing character in file name: {name}"
        )));
    }
    if name.chars().any(|ch| {
        ch.is_control() || matches!(ch, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*')
    }) {
        return Err(AppError::Message(format!(
            "Unsafe path character in file name: {name}"
        )));
    }
    let stem = Path::new(name)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or(name)
        .to_ascii_uppercase();
    if WINDOWS_RESERVED_NAMES.contains(&stem.as_str()) {
        return Err(AppError::Message(format!(
            "Reserved Windows file name: {name}"
        )));
    }
    Ok(name.to_string())
}

pub fn is_safe_archive_path_component(value: &str) -> bool {
    if value.is_empty() || value == "." || value == ".." {
        return false;
    }
    if value.ends_with(' ') || value.ends_with('.') {
        return false;
    }
    if value.chars().any(|ch| {
        ch.is_control() || matches!(ch, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*')
    }) {
        return false;
    }
    !is_windows_reserved_name(value)
}

pub fn unique_child_folder(parent: &Path, name: &str, fallback: &str) -> Result<PathBuf> {
    let base = safe_path_component(name, fallback);
    for index in 0..10_000 {
        let candidate_name = if index == 0 {
            base.clone()
        } else {
            format!("{base} {index}")
        };
        let candidate = parent.join(candidate_name);
        if !candidate.exists() {
            fs::create_dir_all(&candidate)?;
            return Ok(candidate);
        }
    }
    Err(AppError::Message(format!(
        "Could not choose a unique folder under {}",
        parent.display()
    )))
}

pub fn expand_user_path(value: &str) -> PathBuf {
    PathBuf::from(expand_percent_environment_variables(value.trim()))
}

fn expand_percent_environment_variables(value: &str) -> String {
    let mut expanded = String::with_capacity(value.len());
    let mut index = 0usize;
    while let Some(start_offset) = value[index..].find('%') {
        let start = index + start_offset;
        expanded.push_str(&value[index..start]);
        let name_start = start + 1;
        let Some(end_offset) = value[name_start..].find('%') else {
            expanded.push_str(&value[start..]);
            return expanded;
        };
        let end = name_start + end_offset;
        let name = &value[name_start..end];
        if !name.is_empty() {
            if let Some(replacement) = std::env::var_os(name) {
                expanded.push_str(&replacement.to_string_lossy());
                index = end + 1;
                continue;
            }
        }
        expanded.push('%');
        index = name_start;
    }
    expanded.push_str(&value[index..]);
    expanded
}

pub fn safe_path_component(value: &str, fallback: &str) -> String {
    let cleaned = value
        .chars()
        .map(|character| {
            if character.is_control()
                || matches!(
                    character,
                    '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*'
                )
            {
                ' '
            } else {
                character
            }
        })
        .collect::<String>();
    let cleaned = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    let cleaned = cleaned.trim_matches([' ', '.']).trim();
    if cleaned.is_empty() || is_windows_reserved_name(cleaned) {
        fallback.to_string()
    } else {
        cleaned.to_string()
    }
}

fn is_windows_reserved_name(value: &str) -> bool {
    let device_candidate = value
        .split('.')
        .next()
        .unwrap_or(value)
        .trim_end_matches([' ', '.'])
        .to_ascii_uppercase();
    WINDOWS_RESERVED_NAMES.contains(&device_candidate.as_str())
}

#[cfg(test)]
mod tests {
    use super::{expand_user_path, is_safe_archive_path_component, safe_path_component};
    use std::path::MAIN_SEPARATOR;

    #[test]
    fn safe_archive_path_component_rejects_windows_special_names() {
        for value in [
            "image.png:payload",
            "CON",
            "NUL.txt",
            "COM1",
            "LPT9.jpg",
            "trailing-dot.",
            "trailing-space ",
        ] {
            assert!(!is_safe_archive_path_component(value), "{value}");
        }
    }

    #[test]
    fn safe_archive_path_component_accepts_portable_oaa_names() {
        for value in [
            ".oacollection",
            ".oaartwork",
            ".oagallery",
            "artworks",
            "OAA-00001",
            "moonlit-main.png",
        ] {
            assert!(is_safe_archive_path_component(value), "{value}");
        }
    }

    #[test]
    fn safe_path_component_falls_back_for_windows_special_names() {
        assert_eq!(safe_path_component("CON", "Untitled"), "Untitled");
        assert_eq!(safe_path_component("NUL.txt", "Untitled"), "Untitled");
    }

    #[test]
    fn expand_user_path_expands_percent_environment_variable() {
        let root = std::env::current_dir().unwrap();
        std::env::set_var("OACURATOR_TEST_EXPAND_ROOT", &root);

        let expanded = expand_user_path(&format!(
            "%OACURATOR_TEST_EXPAND_ROOT%{MAIN_SEPARATOR}child"
        ));

        assert_eq!(expanded, root.join("child"));
    }
}
