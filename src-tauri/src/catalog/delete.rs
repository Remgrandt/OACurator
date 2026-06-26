use std::path::Path;

use super::{
    normalized_path_prefix, DeleteFilePreview, DeletePreview, DeleteResult, DeleteTrashFailure,
    DerivedAsset,
};

pub(crate) fn delete_candidate_for_derived_asset(
    asset: &DerivedAsset,
) -> Option<DeleteFilePreview> {
    (asset.derivative_type == "png_export").then(|| DeleteFilePreview {
        path: asset.path.clone(),
        label: asset
            .path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("PNG export")
            .to_string(),
        reason: "PNG export".to_string(),
    })
}

pub(crate) fn push_unique_delete_candidate(
    candidates: &mut Vec<DeleteFilePreview>,
    candidate: DeleteFilePreview,
) {
    if candidates.iter().any(|existing| {
        normalized_path_prefix(&existing.path) == normalized_path_prefix(&candidate.path)
    }) {
        return;
    }
    candidates.push(candidate);
}

pub(crate) fn file_source_kind_delete_reason(source_kind: &str) -> &'static str {
    match source_kind {
        "copied" => "Copied image",
        "imported" => "Imported image",
        _ => "OAC-managed image",
    }
}

pub(crate) fn pretrash_managed_files_or_abort<F>(
    preview: &DeletePreview,
    trash_file: &mut F,
    result: &mut DeleteResult,
) where
    F: FnMut(&Path) -> std::result::Result<(), String>,
{
    for candidate in &preview.files_to_trash {
        trash_delete_candidate_if_exists(candidate, trash_file, result);
    }
}

pub(crate) fn trash_delete_candidate_if_exists<F>(
    candidate: &DeleteFilePreview,
    trash_file: &mut F,
    result: &mut DeleteResult,
) where
    F: FnMut(&Path) -> std::result::Result<(), String>,
{
    if !candidate.path.exists() {
        return;
    }
    if let Err(error) = trash_file(&candidate.path) {
        result.trash_failures.push(DeleteTrashFailure {
            path: candidate.path.clone(),
            error,
        });
    } else {
        push_unique_delete_candidate(&mut result.trashed_files, candidate.clone());
    }
}

pub(crate) fn trash_file_if_exists<F>(path: &Path, trash_file: &mut F, result: &mut DeleteResult)
where
    F: FnMut(&Path) -> std::result::Result<(), String>,
{
    if !path.exists() {
        return;
    }
    let candidate = DeleteFilePreview {
        path: path.to_path_buf(),
        label: path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("Managed file")
            .to_string(),
        reason: "OAC-managed file".to_string(),
    };
    trash_delete_candidate_if_exists(&candidate, trash_file, result);
}

pub(crate) fn move_path_to_trash(path: &Path) -> std::result::Result<(), String> {
    trash::delete(path).map_err(|error| error.to_string())
}
