use super::*;

pub(super) fn start_thumbnail_cache_generation(
    app: tauri::AppHandle,
    catalog: Catalog,
    cache_dir: PathBuf,
    collection_id: i64,
) {
    tauri::async_runtime::spawn_blocking(move || {
        let context = CacheGenerationContext {
            app: &app,
            catalog: &catalog,
            cache_dir: &cache_dir,
            collection_id,
        };
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
    generate_cache_derivatives_parallel(
        work_items,
        context.cache_dir,
        worker_count,
        phase.options,
        |result| {
            completed += 1;
            let current_path = result.item.source_path.clone();
            if register_thumbnail_cache_work_result(context.catalog, result).is_ok() {
                succeeded += 1;
            } else {
                failed += 1;
            }
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
    Some((completed, succeeded, failed))
}
