use crate::catalog::{Catalog, FileAsset};
use crate::path_safety::validate_file_name_component;
use crate::{AppError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlanStatus {
    Ready,
    Blocked,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MoveOperation {
    pub file_asset_id: i64,
    pub source: PathBuf,
    pub destination: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MovePlan {
    pub artwork_id: i64,
    pub status: PlanStatus,
    pub operations: Vec<MoveOperation>,
    pub issues: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MoveResult {
    pub succeeded: usize,
    pub failed: usize,
    pub messages: Vec<String>,
}

pub fn plan_artwork_move(
    catalog: &Catalog,
    artwork_id: i64,
    destination_root: &Path,
) -> Result<MovePlan> {
    let detail = catalog.artwork_detail(artwork_id)?;
    let artist = detail
        .artist_credits
        .first()
        .map(|credit| credit.name.as_str())
        .unwrap_or("Unknown Artist");
    let artist_component = safe_component(artist)?;
    let title_component = safe_component(&detail.title)?;
    let folder = destination_root.join(&artist_component);
    let mut issues = Vec::new();
    let mut operations = Vec::new();
    let mut planned_destinations = HashSet::new();

    for (index, asset) in detail.file_assets.iter().enumerate() {
        if !asset.current_path.exists() {
            issues.push(format!(
                "Source file is missing: {}",
                asset.current_path.display()
            ));
            continue;
        }
        let destination = destination_for_asset(
            &folder,
            &detail.canonical_id,
            &title_component,
            asset,
            index,
            &mut planned_destinations,
        );
        if destination.exists() && destination != asset.current_path {
            issues.push(format!(
                "Destination already exists: {}",
                destination.display()
            ));
        }
        operations.push(MoveOperation {
            file_asset_id: asset.id,
            source: asset.current_path.clone(),
            destination,
        });
    }

    Ok(MovePlan {
        artwork_id,
        status: if issues.is_empty() {
            PlanStatus::Ready
        } else {
            PlanStatus::Blocked
        },
        operations,
        issues,
    })
}

pub fn execute_move_plan(catalog: &Catalog, plan: &MovePlan) -> Result<MoveResult> {
    if plan.status != PlanStatus::Ready {
        return Err(AppError::Message(
            "Cannot execute a blocked move plan".to_string(),
        ));
    }

    let mut succeeded = 0usize;
    let mut failed = 0usize;
    let mut messages = Vec::new();

    for operation in &plan.operations {
        let result = (|| -> Result<()> {
            if let Some(parent) = operation.destination.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::rename(&operation.source, &operation.destination)?;
            catalog.update_file_asset_path(operation.file_asset_id, &operation.destination)?;
            Ok(())
        })();

        match result {
            Ok(()) => {
                succeeded += 1;
                catalog.write_operation_log(
                    plan.artwork_id,
                    Some(operation.file_asset_id),
                    &operation.source,
                    &operation.destination,
                    "success",
                    None,
                )?;
            }
            Err(error) => {
                failed += 1;
                let message = error.to_string();
                catalog.write_operation_log(
                    plan.artwork_id,
                    Some(operation.file_asset_id),
                    &operation.source,
                    &operation.destination,
                    "failed",
                    Some(&message),
                )?;
                messages.push(message);
            }
        }
    }

    Ok(MoveResult {
        succeeded,
        failed,
        messages,
    })
}

fn destination_for_asset(
    folder: &Path,
    canonical_id: &str,
    title: &str,
    asset: &FileAsset,
    index: usize,
    planned: &mut HashSet<PathBuf>,
) -> PathBuf {
    let extension = if asset.extension.is_empty() {
        String::new()
    } else {
        format!(".{}", asset.extension)
    };
    let base = if index == 0 {
        format!("{canonical_id} - {title}{extension}")
    } else {
        let stem = asset
            .current_path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .map(|stem| safe_component(stem).unwrap_or_else(|_| format!("file-{}", asset.id)))
            .unwrap_or_else(|| format!("file-{}", asset.id));
        format!("{canonical_id} - {title} - {stem}{extension}")
    };
    let mut destination = folder.join(base);
    if planned.contains(&destination) {
        destination = folder.join(format!(
            "{canonical_id} - {title} - file-{}{}",
            asset.id, extension
        ));
    }
    planned.insert(destination.clone());
    destination
}

fn safe_component(value: &str) -> Result<String> {
    validate_file_name_component(value)
}
