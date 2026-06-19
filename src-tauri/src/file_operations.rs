// Copyright (c) 2026 Remgrandt Works. All rights reserved.

use crate::catalog::{
    AssetKind, Catalog, FileRenameExecution, FileRenamePlan, FileRenameResult, OperationLog,
};
use crate::file_ops::{execute_move_plan, plan_artwork_move, MovePlan, MoveResult};
use crate::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileOperationRecoveryReport {
    pub artwork_id: i64,
    pub failed_operations: Vec<OperationLog>,
    pub rollback_attention_count: usize,
    pub messages: Vec<String>,
}

pub struct FileOperationService<'a> {
    catalog: &'a Catalog,
}

impl<'a> FileOperationService<'a> {
    pub fn new(catalog: &'a Catalog) -> Self {
        Self { catalog }
    }

    pub fn preview_rename(
        &self,
        asset_kind: AssetKind,
        asset_id: i64,
        new_name: &str,
    ) -> Result<FileRenamePlan> {
        self.catalog
            .preview_rename_artwork_file_item(asset_kind, asset_id, new_name)
    }

    pub fn execute_rename(&self, execution: FileRenameExecution) -> Result<FileRenameResult> {
        self.catalog.execute_file_rename(execution)
    }

    pub fn plan_artwork_move(&self, artwork_id: i64, destination_root: &Path) -> Result<MovePlan> {
        plan_artwork_move(self.catalog, artwork_id, destination_root)
    }

    pub fn execute_move_plan(&self, plan: &MovePlan) -> Result<MoveResult> {
        execute_move_plan(self.catalog, plan)
    }

    pub fn recovery_report_for_artwork(
        &self,
        artwork_id: i64,
    ) -> Result<FileOperationRecoveryReport> {
        let failed_operations = self
            .catalog
            .operation_logs_for_artwork(artwork_id)?
            .into_iter()
            .filter(|log| log.result != "success")
            .collect::<Vec<_>>();
        let rollback_attention_count = failed_operations
            .iter()
            .filter(|log| {
                log.message
                    .as_deref()
                    .is_some_and(|message| message.to_ascii_lowercase().contains("rollback"))
            })
            .count();
        let messages = failed_operations
            .iter()
            .filter_map(|log| log.message.clone())
            .collect();
        Ok(FileOperationRecoveryReport {
            artwork_id,
            failed_operations,
            rollback_attention_count,
            messages,
        })
    }
}
