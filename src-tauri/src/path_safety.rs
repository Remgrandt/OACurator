// Copyright (c) 2026 Remgrandt Works. All rights reserved.

use std::fs;
use std::path::{Path, PathBuf};

use crate::{AppError, Result};

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
    let reserved = [
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
        "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];
    if reserved.contains(&stem.as_str()) {
        return Err(AppError::Message(format!(
            "Reserved Windows file name: {name}"
        )));
    }
    Ok(name.to_string())
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
    if cleaned.is_empty() {
        fallback.to_string()
    } else {
        cleaned.to_string()
    }
}
