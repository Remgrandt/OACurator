// Copyright (c) 2026 Remgrandt Works. All rights reserved.

use super::*;
use crate::jobs::{JobId, JobSnapshot};

#[tauri::command]
pub fn list_jobs_command(state: tauri::State<'_, AppState>) -> Vec<JobSnapshot> {
    state.jobs.snapshots()
}

#[tauri::command]
pub fn cancel_job_command(state: tauri::State<'_, AppState>, job_id: u64) -> bool {
    state.jobs.cancel(JobId(job_id))
}
