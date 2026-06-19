// Copyright (c) 2026 Remgrandt Works. All rights reserved.

use super::*;

#[tauri::command]
pub async fn create_gallery_command(
    state: tauri::State<'_, AppState>,
    request: CreateGalleryRequest,
) -> std::result::Result<GallerySummary, String> {
    let collection_id = request
        .collection_id
        .ok_or_else(|| "Open a Collection before creating a Gallery".to_string())?;
    let catalog = state.catalog.clone();
    catalog_blocking("Create gallery", move || {
        catalog
            .collection_summary(collection_id)
            .map_err(|error| error.to_string())?;
        let collection = catalog
            .create_gallery_with_provider_ids(
                &request.name,
                Path::new(&request.path),
                request.caf_gallery_room_id.as_deref(),
                request.raremarq_gallery_id.as_deref(),
                request.snikt_gallery_inherits_collection,
            )
            .map_err(|error| error.to_string())?;
        catalog
            .link_gallery_to_collection(collection_id, collection.id)
            .map_err(|error| error.to_string())?;
        Ok(collection)
    })
    .await
}

#[tauri::command]
pub async fn create_artwork_command(
    state: tauri::State<'_, AppState>,
    request: CreateArtworkRequest,
) -> std::result::Result<ArtworkSummary, String> {
    let catalog = state.catalog.clone();
    catalog_blocking("Create artwork", move || {
        catalog
            .create_artwork_in_gallery(request.gallery_id, &request.title, None)
            .map_err(|error| error.to_string())
    })
    .await
}

#[tauri::command]
pub async fn attach_file_assets_command(
    state: tauri::State<'_, AppState>,
    request: AttachFileAssetsRequest,
) -> std::result::Result<ArtworkDetail, String> {
    let catalog = state.catalog.clone();
    let cache_dir = state.cache_dir.clone();
    let paths = request
        .paths
        .into_iter()
        .map(PathBuf::from)
        .collect::<Vec<_>>();
    let mode = request.mode.unwrap_or(AttachMode::Copy);

    catalog_blocking("Attach file assets", move || {
        attach_files_to_artwork(&catalog, request.artwork_id, &paths, &cache_dir, mode)
            .map_err(|error| error.to_string())
    })
    .await
}

#[tauri::command]
pub fn preview_delete_collection_command(
    state: tauri::State<'_, AppState>,
    collection_id: i64,
) -> std::result::Result<DeletePreview, String> {
    state
        .catalog
        .delete_collection_preview(collection_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn preview_delete_gallery_command(
    state: tauri::State<'_, AppState>,
    request: DeleteGalleryRequest,
) -> std::result::Result<DeletePreview, String> {
    state
        .catalog
        .delete_gallery_preview(request.gallery_id, request.collection_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn preview_delete_artwork_command(
    state: tauri::State<'_, AppState>,
    request: DeleteArtworkRequest,
) -> std::result::Result<DeletePreview, String> {
    state
        .catalog
        .delete_artwork_from_gallery_preview(request.artwork_id, request.gallery_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn preview_delete_artwork_file_command(
    state: tauri::State<'_, AppState>,
    request: DeleteArtworkFileRequest,
) -> std::result::Result<DeletePreview, String> {
    state
        .catalog
        .delete_artwork_file_item_preview(request.asset_kind, request.asset_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn delete_collection_command(
    state: tauri::State<'_, AppState>,
    collection_id: i64,
) -> std::result::Result<DeleteResult, String> {
    let catalog = state.catalog.clone();
    catalog_blocking("Delete collection", move || {
        catalog
            .delete_collection(collection_id)
            .map_err(|error| error.to_string())
    })
    .await
}

#[tauri::command]
pub async fn delete_gallery_command(
    state: tauri::State<'_, AppState>,
    request: DeleteGalleryRequest,
) -> std::result::Result<DeleteResult, String> {
    let catalog = state.catalog.clone();
    catalog_blocking("Delete gallery", move || {
        if let Some(collection_id) = request.collection_id {
            catalog
                .delete_gallery_from_collection(collection_id, request.gallery_id)
                .map_err(|error| error.to_string())
        } else {
            catalog
                .delete_gallery(request.gallery_id)
                .map_err(|error| error.to_string())
        }
    })
    .await
}

#[tauri::command]
pub async fn delete_artwork_command(
    state: tauri::State<'_, AppState>,
    request: DeleteArtworkRequest,
) -> std::result::Result<DeleteResult, String> {
    let catalog = state.catalog.clone();
    catalog_blocking("Delete artwork", move || {
        if let Some(gallery_id) = request.gallery_id {
            catalog
                .delete_artwork_from_gallery(gallery_id, request.artwork_id)
                .map_err(|error| error.to_string())
        } else {
            catalog
                .delete_artwork(request.artwork_id)
                .map_err(|error| error.to_string())
        }
    })
    .await
}

#[tauri::command]
pub async fn delete_artwork_file_command(
    state: tauri::State<'_, AppState>,
    request: DeleteArtworkFileRequest,
) -> std::result::Result<DeleteArtworkFileResult, String> {
    let catalog = state.catalog.clone();
    catalog_blocking("Delete artwork file", move || {
        catalog
            .delete_artwork_file_item(request.asset_kind, request.asset_id)
            .map_err(|error| error.to_string())
    })
    .await
}

#[tauri::command]
pub async fn rename_collection_command(
    state: tauri::State<'_, AppState>,
    collection_id: i64,
    name: String,
) -> std::result::Result<CollectionSummary, String> {
    let catalog = state.catalog.clone();
    catalog_blocking("Rename collection", move || {
        catalog
            .rename_collection(collection_id, &name)
            .map_err(|error| error.to_string())
    })
    .await
}

#[tauri::command]
pub async fn rename_gallery_command(
    state: tauri::State<'_, AppState>,
    gallery_id: i64,
    name: String,
) -> std::result::Result<GallerySummary, String> {
    let catalog = state.catalog.clone();
    catalog_blocking("Rename gallery", move || {
        catalog
            .rename_gallery(gallery_id, &name)
            .map_err(|error| error.to_string())
    })
    .await
}

#[tauri::command]
pub async fn save_collection_provider_ids_command(
    state: tauri::State<'_, AppState>,
    request: SaveCollectionProviderIdsRequest,
) -> std::result::Result<CollectionSummary, String> {
    let catalog = state.catalog.clone();
    catalog_blocking("Save collection provider IDs", move || {
        catalog
            .save_collection_provider_ids(
                request.collection_id,
                request.caf_collection_id.as_deref(),
                request.snikt_collection_id.as_deref(),
                request.raremarq_collection_id.as_deref(),
            )
            .map_err(|error| error.to_string())
    })
    .await
}

#[tauri::command]
pub async fn save_gallery_provider_ids_command(
    state: tauri::State<'_, AppState>,
    request: SaveGalleryProviderIdsRequest,
) -> std::result::Result<GallerySummary, String> {
    let catalog = state.catalog.clone();
    catalog_blocking("Save gallery provider IDs", move || {
        catalog
            .save_gallery_provider_ids(
                request.gallery_id,
                request.caf_gallery_room_id.as_deref(),
                request.raremarq_gallery_id.as_deref(),
                request.snikt_gallery_inherits_collection,
            )
            .map_err(|error| error.to_string())
    })
    .await
}

#[tauri::command]
pub async fn merge_gallery_command(
    state: tauri::State<'_, AppState>,
    request: MergeGalleryRequest,
) -> std::result::Result<WorkspaceState, String> {
    let catalog = state.catalog.clone();
    catalog_blocking("Merge gallery", move || {
        catalog
            .merge_gallery_into(GalleryMergeUpdate {
                collection_id: request.collection_id,
                source_gallery_id: request.source_gallery_id,
                target_gallery_id: request.target_gallery_id,
                name: request.name,
                caf_gallery_room_id: request.caf_gallery_room_id,
                raremarq_gallery_id: request.raremarq_gallery_id,
                snikt_gallery_inherits_collection: request.snikt_gallery_inherits_collection,
            })
            .map_err(|error| error.to_string())?;
        catalog.workspace_state().map_err(|error| error.to_string())
    })
    .await
}

#[tauri::command]
pub async fn merge_artwork_command(
    state: tauri::State<'_, AppState>,
    request: MergeArtworkRequest,
) -> std::result::Result<WorkspaceState, String> {
    let metadata = metadata_update_from_request(request.metadata)?;
    let catalog = state.catalog.clone();
    catalog_blocking("Merge artwork", move || {
        catalog
            .merge_artwork_into(ArtworkMergeUpdate {
                collection_id: request.collection_id,
                source_gallery_id: request.source_gallery_id,
                source_artwork_id: request.source_artwork_id,
                target_artwork_id: request.target_artwork_id,
                metadata,
            })
            .map_err(|error| error.to_string())?;
        catalog.workspace_state().map_err(|error| error.to_string())
    })
    .await
}

#[tauri::command]
pub async fn rename_artwork_command(
    state: tauri::State<'_, AppState>,
    artwork_id: i64,
    title: String,
) -> std::result::Result<ArtworkDetail, String> {
    let catalog = state.catalog.clone();
    catalog_blocking("Rename artwork", move || {
        catalog
            .rename_artwork_title(artwork_id, &title)
            .map_err(|error| error.to_string())
    })
    .await
}

#[tauri::command]
pub async fn preview_rename_artwork_file_command(
    state: tauri::State<'_, AppState>,
    request: RenameArtworkFileRequest,
) -> std::result::Result<FileRenamePlan, String> {
    let catalog = state.catalog.clone();
    catalog_blocking("Preview artwork file rename", move || {
        FileOperationService::new(&catalog)
            .preview_rename(request.asset_kind, request.asset_id, &request.name)
            .map_err(|error| error.to_string())
    })
    .await
}

#[tauri::command]
pub async fn execute_file_rename_command(
    state: tauri::State<'_, AppState>,
    request: ExecuteFileRenameRequest,
) -> std::result::Result<FileRenameResult, String> {
    let catalog = state.catalog.clone();
    catalog_blocking("Execute file rename", move || {
        FileOperationService::new(&catalog)
            .execute_rename(FileRenameExecution {
                plan: request.plan,
                confirmed_physical_file_rename: request.confirmed_physical_file_rename,
            })
            .map_err(|error| error.to_string())
    })
    .await
}

#[tauri::command]
pub async fn select_gallery_command(
    state: tauri::State<'_, AppState>,
    gallery_id: i64,
) -> std::result::Result<(), String> {
    let catalog = state.catalog.clone();
    catalog_blocking("Select gallery", move || {
        catalog
            .select_gallery(gallery_id)
            .map_err(|error| error.to_string())
    })
    .await
}

#[tauri::command]
pub fn list_artworks_command(
    state: tauri::State<'_, AppState>,
) -> std::result::Result<Vec<ArtworkSummary>, String> {
    state
        .catalog
        .list_artworks()
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn artwork_detail_command(
    state: tauri::State<'_, AppState>,
    artwork_id: i64,
) -> std::result::Result<ArtworkDetail, String> {
    let catalog = state.catalog.clone();
    let cache_dir = state.cache_dir.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let (_, cache_warnings) =
            ensure_artwork_cache_derivatives_with_warnings(&catalog, artwork_id, &cache_dir)
                .map_err(|error| error.to_string())?;
        catalog
            .reconcile_artwork_manifest_from_catalog(artwork_id)
            .map_err(|error| error.to_string())?;
        let mut detail = catalog
            .artwork_detail(artwork_id)
            .map_err(|error| error.to_string())?;
        detail.cache_warnings = cache_warnings;
        Ok(detail)
    })
    .await
    .map_err(|error| format!("Artwork detail task failed: {error}"))?
}

#[tauri::command]
pub async fn ensure_artwork_thumbnail_command(
    state: tauri::State<'_, AppState>,
    artwork_id: i64,
) -> std::result::Result<Option<PathBuf>, String> {
    let catalog = state.catalog.clone();
    let cache_dir = state.cache_dir.clone();
    tauri::async_runtime::spawn_blocking(move || {
        ensure_artwork_cache_derivatives(&catalog, artwork_id, &cache_dir)
            .map_err(|error| error.to_string())?;
        let detail = catalog
            .artwork_detail(artwork_id)
            .map_err(|error| error.to_string())?;
        Ok(detail
            .derived_assets
            .into_iter()
            .find(|asset| asset.derivative_type == "thumbnail")
            .map(|asset| asset.path))
    })
    .await
    .map_err(|error| format!("Thumbnail generation task failed: {error}"))?
}

#[tauri::command]
pub async fn save_metadata_command(
    state: tauri::State<'_, AppState>,
    request: SaveMetadataRequest,
) -> std::result::Result<ArtworkDetail, String> {
    let update = metadata_update_from_request(request)?;
    let artwork_id = update.artwork_id;
    let catalog = state.catalog.clone();
    catalog_blocking("Save metadata", move || {
        catalog
            .save_metadata(update)
            .map_err(|error| error.to_string())?;
        catalog
            .artwork_detail(artwork_id)
            .map_err(|error| error.to_string())
    })
    .await
}

fn metadata_update_from_request(
    request: SaveMetadataRequest,
) -> std::result::Result<MetadataUpdate, String> {
    validate_artist_roles(&request.artist_credits)?;
    let artist_credits = request
        .artist_credits
        .iter()
        .map(artist_credit_update_from_request)
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(MetadataUpdate {
        artwork_id: request.artwork_id,
        title: request.title,
        description: request.description,
        for_sale_status: request.for_sale_status,
        media_type_id: request.media_type_id.or_else(|| {
            request
                .media
                .as_deref()
                .and_then(media_type_id_for_label)
                .map(str::to_string)
        }),
        art_type_id: request.art_type_id.or_else(|| {
            request
                .format
                .as_deref()
                .and_then(art_type_id_for_label)
                .map(str::to_string)
        }),
        publication_status_id: request.publication_status_id,
        active: request.active.unwrap_or(true),
        illustration_exchange: request.illustration_exchange.unwrap_or(false),
        ix_for_sale: request.ix_for_sale.unwrap_or(false),
        artist_credits,
        media: request.media,
        format: request.format,
        caf_url: request.caf_url,
        snikt_url: request.snikt_url,
        raremarq_url: request.raremarq_url,
        generic_url: request.generic_url,
        snikt_metadata: request.snikt_metadata,
        purchase_price: request.purchase_price,
        estimated_value: request.estimated_value,
        purchase_date: request.purchase_date,
        provenance: request.provenance,
        personal_notes: request.personal_notes,
    })
}

#[tauri::command]
pub async fn save_image_metadata_command(
    state: tauri::State<'_, AppState>,
    request: SaveImageMetadataRequest,
) -> std::result::Result<ArtworkDetail, String> {
    let catalog = state.catalog.clone();
    catalog_blocking("Save image metadata", move || {
        let artwork_id = catalog
            .update_image_role(
                request.asset_kind,
                request.asset_id,
                request.image_role.as_deref(),
            )
            .map_err(|error| error.to_string())?;
        catalog
            .artwork_detail(artwork_id)
            .map_err(|error| error.to_string())
    })
    .await
}

#[tauri::command]
pub async fn reorder_file_assets_command(
    state: tauri::State<'_, AppState>,
    request: ReorderFileAssetsRequest,
) -> std::result::Result<ArtworkDetail, String> {
    let catalog = state.catalog.clone();
    catalog_blocking("Reorder file assets", move || {
        catalog
            .reorder_file_assets(request.artwork_id, &request.file_asset_ids)
            .map_err(|error| error.to_string())
    })
    .await
}

#[tauri::command]
pub fn snikt_upload_prefill_url_command(
    state: tauri::State<'_, AppState>,
    request: SniktUploadPrefillUrlRequest,
) -> std::result::Result<String, String> {
    state
        .catalog
        .snikt_upload_prefill_url(request.artwork_id)
        .map_err(|error| error.to_string())
}

pub(crate) fn validate_artist_roles(
    credits: &[ArtistCreditRequest],
) -> std::result::Result<(), String> {
    for credit in credits {
        artist_role_id_from_request(credit)?;
    }
    Ok(())
}

fn artist_credit_update_from_request(
    credit: &ArtistCreditRequest,
) -> std::result::Result<ArtistCreditUpdate, String> {
    let role_id = artist_role_id_from_request(credit)?;
    let mut first_name = blank_to_none(credit.first_name.as_deref());
    let mut last_name = blank_to_none(credit.last_name.as_deref());
    if first_name.is_none() && last_name.is_none() {
        if let Some(name) = blank_to_none(credit.name.as_deref()) {
            let (first, last) = split_artist_name(&name);
            first_name = first;
            last_name = last;
        }
    }
    Ok(ArtistCreditUpdate {
        first_name,
        last_name,
        role_id,
    })
}

fn artist_role_id_from_request(
    credit: &ArtistCreditRequest,
) -> std::result::Result<Option<String>, String> {
    if let Some(role_id) = blank_to_none(credit.role_id.as_deref()) {
        if artist_role_label_for_id(&role_id).is_some() {
            return Ok(Some(role_id));
        }
        return Err(format!("Unsupported artist role ID: {role_id}"));
    }
    if let Some(role) = blank_to_none(credit.role.as_deref()) {
        if artist_role_label_for_id(&role).is_some() {
            return Ok(Some(role));
        }
        if let Some(role_id) = artist_role_id_for_label(&role) {
            return Ok(Some(role_id.to_string()));
        }
        return Err(format!("Unsupported artist role: {role}"));
    }
    Ok(None)
}

fn split_artist_name(name: &str) -> (Option<String>, Option<String>) {
    let parts = name.split_whitespace().collect::<Vec<_>>();
    match parts.as_slice() {
        [] => (None, None),
        [single] => (None, Some((*single).to_string())),
        [first @ .., last] => (Some(first.join(" ")), Some((*last).to_string())),
    }
}

fn blank_to_none(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::{validate_artist_roles, ArtistCreditRequest};

    #[test]
    fn validate_artist_roles_rejects_values_outside_caf_role_list() {
        let result = validate_artist_roles(&[ArtistCreditRequest {
            name: Some("Jane Doe".to_string()),
            role: Some("Sculptor".to_string()),
            first_name: None,
            last_name: None,
            role_id: None,
        }]);

        assert!(result.is_err());
    }

    #[test]
    fn validate_artist_roles_accepts_known_values_and_blank_roles() {
        let result = validate_artist_roles(&[
            ArtistCreditRequest {
                name: Some("Jane Doe".to_string()),
                role: None,
                first_name: None,
                last_name: None,
                role_id: Some("1".to_string()),
            },
            ArtistCreditRequest {
                name: Some("Alex Roe".to_string()),
                role: Some("Colorist".to_string()),
                first_name: None,
                last_name: None,
                role_id: None,
            },
        ]);

        assert!(result.is_ok());
    }
}
