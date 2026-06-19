// Copyright (c) 2026 Remgrandt Works. All rights reserved.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct JobId(pub u64);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobProgress {
    pub job_id: JobId,
    pub label: String,
    pub phase: String,
    pub message: String,
    pub total: usize,
    pub completed: usize,
    pub done: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", tag = "status", content = "detail")]
pub enum JobResult {
    Succeeded,
    Failed(String),
    Canceled,
}

#[derive(Clone, Default)]
pub struct JobCancellation {
    canceled: Arc<AtomicBool>,
}

impl JobCancellation {
    pub fn cancel(&self) {
        self.canceled.store(true, Ordering::SeqCst);
    }

    pub fn is_canceled(&self) -> bool {
        self.canceled.load(Ordering::SeqCst)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobSnapshot {
    pub job_id: JobId,
    pub label: String,
    pub latest_progress: Option<JobProgress>,
    pub result: Option<JobResult>,
}

#[derive(Clone, Default)]
pub struct JobService {
    inner: Arc<JobServiceInner>,
}

#[derive(Default)]
struct JobServiceInner {
    next_id: AtomicU64,
    jobs: Mutex<BTreeMap<JobId, JobRecord>>,
}

#[derive(Clone)]
struct JobRecord {
    label: String,
    cancellation: JobCancellation,
    latest_progress: Option<JobProgress>,
    result: Option<JobResult>,
}

#[derive(Clone)]
pub struct JobHandle {
    id: JobId,
    label: String,
    service: JobService,
    cancellation: JobCancellation,
}

impl JobService {
    pub fn start(&self, label: impl Into<String>) -> JobHandle {
        let id = JobId(self.inner.next_id.fetch_add(1, Ordering::SeqCst) + 1);
        let label = label.into();
        let cancellation = JobCancellation::default();
        let record = JobRecord {
            label: label.clone(),
            cancellation: cancellation.clone(),
            latest_progress: None,
            result: None,
        };
        self.inner
            .jobs
            .lock()
            .expect("job registry lock")
            .insert(id, record);
        JobHandle {
            id,
            label,
            service: self.clone(),
            cancellation,
        }
    }

    pub fn cancel(&self, job_id: JobId) -> bool {
        let jobs = self.inner.jobs.lock().expect("job registry lock");
        if let Some(record) = jobs.get(&job_id) {
            record.cancellation.cancel();
            true
        } else {
            false
        }
    }

    pub fn snapshot(&self, job_id: JobId) -> Option<JobSnapshot> {
        self.inner
            .jobs
            .lock()
            .expect("job registry lock")
            .get(&job_id)
            .map(|record| JobSnapshot {
                job_id,
                label: record.label.clone(),
                latest_progress: record.latest_progress.clone(),
                result: record.result.clone(),
            })
    }

    pub fn snapshots(&self) -> Vec<JobSnapshot> {
        self.inner
            .jobs
            .lock()
            .expect("job registry lock")
            .iter()
            .map(|(job_id, record)| JobSnapshot {
                job_id: *job_id,
                label: record.label.clone(),
                latest_progress: record.latest_progress.clone(),
                result: record.result.clone(),
            })
            .collect()
    }

    fn update(&self, progress: JobProgress) {
        if let Some(record) = self
            .inner
            .jobs
            .lock()
            .expect("job registry lock")
            .get_mut(&progress.job_id)
        {
            record.latest_progress = Some(progress);
        }
    }

    fn finish(&self, job_id: JobId, result: JobResult) {
        if let Some(record) = self
            .inner
            .jobs
            .lock()
            .expect("job registry lock")
            .get_mut(&job_id)
        {
            record.result = Some(result);
        }
    }
}

impl JobHandle {
    pub fn id(&self) -> JobId {
        self.id
    }

    pub fn cancellation(&self) -> JobCancellation {
        self.cancellation.clone()
    }

    pub fn is_canceled(&self) -> bool {
        self.cancellation.is_canceled()
    }

    pub fn update(&self, phase: &str, message: impl Into<String>, completed: usize, total: usize) {
        self.service.update(JobProgress {
            job_id: self.id,
            label: self.label.clone(),
            phase: phase.to_string(),
            message: message.into(),
            total,
            completed,
            done: completed >= total,
        });
    }

    pub fn finish(&self, result: JobResult) {
        self.service.finish(self.id, result);
    }
}

#[cfg(test)]
mod tests {
    use super::{JobResult, JobService};

    #[test]
    fn job_service_tracks_progress_result_and_cancellation() {
        let service = JobService::default();
        let job = service.start("cache");
        job.update("thumbnail", "Generating thumbnails", 1, 3);
        assert_eq!(
            service
                .snapshot(job.id())
                .and_then(|snapshot| snapshot.latest_progress)
                .map(|progress| progress.completed),
            Some(1)
        );

        assert!(service.cancel(job.id()));
        assert!(job.is_canceled());

        job.finish(JobResult::Canceled);
        assert_eq!(
            service
                .snapshot(job.id())
                .and_then(|snapshot| snapshot.result),
            Some(JobResult::Canceled)
        );
    }
}
