// Copyright (c) 2026 Remgrandt Works. All rights reserved.

use super::*;

pub(super) fn start_thumbnail_cache_generation(
    app: tauri::AppHandle,
    catalog: Catalog,
    cache_dir: PathBuf,
    jobs: JobService,
    collection_id: i64,
) {
    tauri::async_runtime::spawn_blocking(move || {
        let job = jobs.start(format!("cache collection {collection_id}"));
        let cancellation = job.cancellation();
        let context = CacheGenerationContext {
            app: &app,
            catalog: &catalog,
            cache_dir: &cache_dir,
            job: &job,
            cancellation: &cancellation,
            collection_id,
        };
        job.update("prepare", "Preparing cache generation", 0, 0);
        let Some((thumbnail_completed, thumbnail_succeeded, thumbnail_failed)) =
            run_collection_cache_generation_phase(
                &context,
                CacheGenerationPhase {
                    phase: "thumbnail",
                    label: "thumbnails",
                    options: CacheDerivativeOptions {
                        create_thumbnail: true,
                        create_preview: false,
                    },
                },
                thumbnail_cache_work_items_for_collection,
            )
        else {
            job.finish(JobResult::Canceled);
            return;
        };

        let Some((preview_completed, preview_succeeded, preview_failed)) =
            run_collection_cache_generation_phase(
                &context,
                CacheGenerationPhase {
                    phase: "preview",
                    label: "previews",
                    options: CacheDerivativeOptions {
                        create_thumbnail: false,
                        create_preview: true,
                    },
                },
                preview_cache_work_items_for_collection,
            )
        else {
            job.finish(JobResult::Canceled);
            return;
        };

        let total = thumbnail_completed + preview_completed;
        let succeeded = thumbnail_succeeded + preview_succeeded;
        let failed = thumbnail_failed + preview_failed;
        let _ = app.emit(
            "thumbnail-cache-progress",
            ThumbnailCacheProgress {
                phase: "complete".to_string(),
                message: format!(
                    "Generated {thumbnail_succeeded} thumbnails and {preview_succeeded} previews"
                ),
                total,
                completed: total,
                succeeded,
                failed,
                current_path: None,
                done: true,
            },
        );
        job.finish(if failed == 0 {
            JobResult::Succeeded
        } else {
            JobResult::Failed(format!("{failed} cache items failed"))
        });
    });
}

#[derive(Debug, Clone, Copy)]
struct CacheGenerationPhase {
    phase: &'static str,
    label: &'static str,
    options: CacheDerivativeOptions,
}

struct CacheGenerationContext<'a> {
    app: &'a tauri::AppHandle,
    catalog: &'a Catalog,
    cache_dir: &'a Path,
    job: &'a crate::jobs::JobHandle,
    cancellation: &'a JobCancellation,
    collection_id: i64,
}

fn run_collection_cache_generation_phase<F>(
    context: &CacheGenerationContext<'_>,
    phase: CacheGenerationPhase,
    work_items_for_collection: F,
) -> Option<(usize, usize, usize)>
where
    F: FnOnce(&Catalog, i64) -> crate::Result<Vec<ThumbnailCacheWorkItem>>,
{
    let label = phase.label;
    if context.cancellation.is_canceled() {
        return None;
    }
    let work_items = match work_items_for_collection(context.catalog, context.collection_id) {
        Ok(work_items) => work_items,
        Err(error) => {
            let _ = context.app.emit(
                "thumbnail-cache-progress",
                ThumbnailCacheProgress {
                    phase: "error".to_string(),
                    message: format!("{label} generation could not start: {error}"),
                    total: 0,
                    completed: 0,
                    succeeded: 0,
                    failed: 1,
                    current_path: None,
                    done: true,
                },
            );
            return None;
        }
    };
    let total = work_items.len();
    context.job.update(
        phase.phase,
        format!("Preparing {label} cache work"),
        0,
        total,
    );
    let _ = context.app.emit(
        "thumbnail-cache-progress",
        ThumbnailCacheProgress {
            phase: "prepare".to_string(),
            message: format!("Generating {label} 0 of {total}"),
            total,
            completed: 0,
            succeeded: 0,
            failed: 0,
            current_path: None,
            done: false,
        },
    );
    if total == 0 {
        return Some((0, 0, 0));
    }

    let worker_count = thumbnail_cache_worker_count(total, "OACURATOR_OAA_CACHE_WORKERS");
    let mut completed = 0usize;
    let mut succeeded = 0usize;
    let mut failed = 0usize;
    generate_cache_derivatives_parallel_with_cancellation(
        work_items,
        context.cache_dir,
        worker_count,
        phase.options,
        Some(context.cancellation.clone()),
        |result| {
            if context.cancellation.is_canceled() {
                return;
            }
            completed += 1;
            let current_path = result.item.source_path.clone();
            if register_thumbnail_cache_work_result(context.catalog, result).is_ok() {
                succeeded += 1;
            } else {
                failed += 1;
            }
            context.job.update(
                phase.phase,
                format!("Generating {label} {completed} of {total}"),
                completed,
                total,
            );
            let _ = context.app.emit(
                "thumbnail-cache-progress",
                ThumbnailCacheProgress {
                    phase: phase.phase.to_string(),
                    message: format!("Generating {label} {completed} of {total}"),
                    total,
                    completed,
                    succeeded,
                    failed,
                    current_path: Some(current_path),
                    done: false,
                },
            );
        },
    );
    if context.cancellation.is_canceled() {
        return None;
    }
    Some((completed, succeeded, failed))
}
