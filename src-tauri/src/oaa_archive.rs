use crate::catalog::{
    art_type_id_for_label, artist_role_id_for_label, media_type_id_for_label, ArtistCreditUpdate,
    ArtworkDetail, ArtworkSummary, AssetKind, Catalog, CollectionSummary, DerivedAssetInsert,
    FileAsset, FileAssetKnownMetadataInsert, FileAssetMetadata, GallerySummary, MetadataUpdate,
};
use crate::export_policy::ExportPolicy;
use crate::oaa_validation::ensure_oaa_archive_valid;
pub use crate::oaa_validation::OAA_MEDIA_TYPE;
use crate::path_safety::is_safe_archive_path_component;
use crate::{AppError, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs::{self, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

const OAA_SCHEMA_VERSION: &str = "0.1";

#[derive(Debug, Clone)]
pub struct OaaImportOptions {
    pub archive_path: PathBuf,
    pub destination_root: Option<PathBuf>,
    pub target_collection_id: Option<i64>,
    pub cache_dir: PathBuf,
}

#[derive(Debug, Clone)]
pub struct OaaExportOptions {
    pub collection_id: i64,
    pub archive_path: PathBuf,
    pub include_images: bool,
    pub include_private_metadata: bool,
    pub allow_overwrite: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OaaImportReport {
    pub collection_id: i64,
    pub galleries_imported: usize,
    pub artworks_imported: usize,
    pub files_imported: usize,
    pub messages: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OaaImportProgress {
    pub phase: String,
    pub message: String,
    pub galleries_total: usize,
    pub galleries_imported: usize,
    pub artworks_total: usize,
    pub artworks_imported: usize,
    pub files_total: usize,
    pub files_imported: usize,
    pub current_artwork: Option<String>,
    pub done: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OaaExportReport {
    pub collection_id: i64,
    pub archive_path: PathBuf,
    pub galleries_exported: usize,
    pub artworks_exported: usize,
    pub files_exported: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OaaExportProgress {
    pub phase: String,
    pub message: String,
    pub current: usize,
    pub total: usize,
}

struct OaaExportSnapshot {
    collection: CollectionSummary,
    galleries: Vec<GallerySummary>,
    artwork_by_id: BTreeMap<i64, ArtworkSummary>,
    gallery_artworks: HashMap<i64, Vec<ArtworkSummary>>,
    artwork_details: BTreeMap<i64, ArtworkDetail>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OaaCollectionManifest {
    schema_version: String,
    id: String,
    name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    external_links: Vec<OaaExternalLink>,
    galleries: Vec<OaaCollectionGalleryRef>,
    artworks: Vec<OaaCollectionArtworkRef>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    extensions: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OaaCollectionGalleryRef {
    id: String,
    name: String,
    path: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    extensions: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OaaCollectionArtworkRef {
    id: String,
    path: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    extensions: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OaaGalleryManifest {
    schema_version: String,
    id: String,
    name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    external_links: Vec<OaaExternalLink>,
    artworks: Vec<OaaGalleryArtworkRef>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    extensions: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OaaGalleryArtworkRef {
    id: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    extensions: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OaaArtworkManifest {
    schema_version: String,
    id: String,
    title: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    external_links: Vec<OaaExternalLink>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    public_metadata: Option<OaaPublicMetadata>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    private_metadata: Option<OaaPrivateMetadata>,
    files: Vec<OaaFileObject>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    extensions: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OaaExternalLink {
    provider: String,
    id: String,
    url: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    extensions: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct OaaPublicMetadata {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    for_sale_status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    media: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    artwork_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    publication_status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    is_public: Option<bool>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    artist_credits: Vec<OaaArtistCredit>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    extensions: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct OaaArtistCredit {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    first_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    last_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    role: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    extensions: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct OaaPrivateMetadata {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    purchase_price: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    estimated_value: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    purchase_date: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    provenance: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    personal_notes: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    extensions: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OaaFileObject {
    id: String,
    relative_path: String,
    file_kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    file_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    size_bytes: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    width: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    height: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    dpi_x: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    dpi_y: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    format: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    media_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    is_primary: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    image_role: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    external_links: Vec<OaaExternalLink>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    extensions: BTreeMap<String, Value>,
}

pub fn import_oaa_archive(catalog: &Catalog, options: OaaImportOptions) -> Result<OaaImportReport> {
    import_oaa_archive_with_progress(catalog, options, |_| {})
}

pub fn import_oaa_archive_with_progress<F>(
    catalog: &Catalog,
    options: OaaImportOptions,
    mut progress: F,
) -> Result<OaaImportReport>
where
    F: FnMut(OaaImportProgress),
{
    ensure_oaa_archive_valid(&options.archive_path)?;

    let archive_file = fs::File::open(&options.archive_path)?;
    let mut zip = ZipArchive::new(archive_file)?;
    validate_zip_entries(&mut zip)?;

    let mimetype = read_zip_string(&mut zip, "mimetype")?;
    if mimetype != OAA_MEDIA_TYPE {
        return Err(AppError::Message(format!(
            "Unsupported OAA mimetype: {mimetype}"
        )));
    }

    let collection_manifest: OaaCollectionManifest =
        read_zip_json(&mut zip, ".oacollection", "collection manifest")?;
    validate_schema(&collection_manifest.schema_version, ".oacollection")?;
    validate_manifest_value(
        &serde_json::to_value(&collection_manifest)?,
        ".oacollection",
    )?;

    fs::create_dir_all(&options.cache_dir)?;

    let artwork_total = collection_manifest.artworks.len();
    let gallery_total = collection_manifest.galleries.len();
    progress(OaaImportProgress {
        phase: "prepare".to_string(),
        message: format!("Preparing OAA archive import: {artwork_total} artworks"),
        galleries_total: gallery_total,
        galleries_imported: 0,
        artworks_total: artwork_total,
        artworks_imported: 0,
        files_total: 0,
        files_imported: 0,
        current_artwork: None,
        done: false,
    });

    if options.target_collection_id.is_none() {
        return import_oaa_archive_as_new_collection(
            catalog,
            &mut zip,
            &collection_manifest,
            &options,
            &mut progress,
        );
    }

    let (collection, collection_folder) = if let Some(collection_id) = options.target_collection_id
    {
        let collection = catalog.collection_summary(collection_id)?;
        let collection_folder = collection
            .manifest_path
            .parent()
            .ok_or_else(|| {
                AppError::Message(format!(
                    "Collection manifest has no parent folder: {}",
                    collection.manifest_path.display()
                ))
            })?
            .to_path_buf();
        (collection, collection_folder)
    } else {
        let destination_root = options.destination_root.as_ref().ok_or_else(|| {
            AppError::Message(
                "Destination folder is required when importing a new Collection".to_string(),
            )
        })?;
        fs::create_dir_all(destination_root)?;
        let collection_folder = unique_child_folder(destination_root, &collection_manifest.name)?;
        let collection_path = collection_folder.join(".oacollection");
        let collection = catalog.create_collection_with_provider_ids(
            &collection_manifest.name,
            &collection_path,
            provider_id(&collection_manifest.external_links, "com.comicartfans").as_deref(),
            provider_id(&collection_manifest.external_links, "com.snikt").as_deref(),
            provider_id(&collection_manifest.external_links, "com.raremarq").as_deref(),
        )?;
        (collection, collection_folder)
    };
    let batch_transaction = catalog.begin_batch_transaction()?;
    save_extension_blocks(
        catalog,
        "collection",
        collection.id,
        &collection_manifest.extensions,
    )?;

    let mut gallery_manifests = Vec::new();
    let mut gallery_id_by_oaa_id = HashMap::new();
    for reference in &collection_manifest.galleries {
        validate_archive_path(&reference.path, "collection gallery reference path")?;
        let gallery_manifest: OaaGalleryManifest =
            read_zip_json(&mut zip, &reference.path, "gallery manifest")?;
        validate_schema(&gallery_manifest.schema_version, &reference.path)?;
        validate_manifest_value(&serde_json::to_value(&gallery_manifest)?, &reference.path)?;
        if gallery_manifest.id != reference.id {
            return Err(AppError::Message(format!(
                "Gallery manifest ID mismatch for {}",
                reference.path
            )));
        }

        let gallery_folder =
            unique_child_folder(&collection_folder.join("galleries"), &gallery_manifest.name)?;
        let gallery_path = gallery_folder.join(".oagallery");
        let gallery = catalog.create_gallery_with_caf_gallery_room_id(
            &gallery_manifest.name,
            &gallery_path,
            provider_id(&gallery_manifest.external_links, "com.comicartfans").as_deref(),
        )?;
        if let Some(snikt_id) = provider_id(&gallery_manifest.external_links, "com.snikt") {
            catalog.mark_gallery_as_snikt_gallery(gallery.id, &snikt_id)?;
        }
        if let Some(raremarq_id) = provider_id(&gallery_manifest.external_links, "com.raremarq") {
            catalog.mark_gallery_as_raremarq_gallery(gallery.id, &raremarq_id)?;
        }
        catalog.link_gallery_to_collection(collection.id, gallery.id)?;
        save_extension_blocks(catalog, "gallery", gallery.id, &gallery_manifest.extensions)?;
        gallery_id_by_oaa_id.insert(gallery_manifest.id.clone(), gallery.id);
        gallery_manifests.push(gallery_manifest);
        progress(OaaImportProgress {
            phase: "gallery".to_string(),
            message: format!(
                "Importing OAA Gallery {} of {}: {}",
                gallery_manifests.len(),
                gallery_total,
                gallery.name
            ),
            galleries_total: gallery_total,
            galleries_imported: gallery_manifests.len(),
            artworks_total: artwork_total,
            artworks_imported: 0,
            files_total: 0,
            files_imported: 0,
            current_artwork: None,
            done: false,
        });
    }

    let mut gallery_membership_by_artwork: HashMap<String, Vec<i64>> = HashMap::new();
    for gallery in &gallery_manifests {
        let Some(&gallery_id) = gallery_id_by_oaa_id.get(&gallery.id) else {
            continue;
        };
        for artwork in &gallery.artworks {
            gallery_membership_by_artwork
                .entry(artwork.id.clone())
                .or_default()
                .push(gallery_id);
        }
    }

    let fallback_gallery =
        if let Some(gallery) = catalog.galleries_for_collection(collection.id)?.first() {
            gallery.clone()
        } else {
            let gallery_path = collection_folder
                .join("galleries")
                .join("Imported Artwork")
                .join(".oagallery");
            let gallery = catalog.create_gallery("Imported Artwork", &gallery_path)?;
            catalog.link_gallery_to_collection(collection.id, gallery.id)?;
            gallery
        };

    let mut files_imported = 0usize;
    let mut source_file_ids_by_oaa_file_id: HashMap<String, i64> = HashMap::new();
    let mut artworks_imported = 0usize;
    for reference in &collection_manifest.artworks {
        validate_archive_path(&reference.path, "collection artwork reference path")?;
        let artwork_manifest: OaaArtworkManifest =
            read_zip_json(&mut zip, &reference.path, "artwork manifest")?;
        validate_schema(&artwork_manifest.schema_version, &reference.path)?;
        validate_manifest_value(&serde_json::to_value(&artwork_manifest)?, &reference.path)?;
        if artwork_manifest.id != reference.id {
            return Err(AppError::Message(format!(
                "Artwork manifest ID mismatch for {}",
                reference.path
            )));
        }
        validate_file_ids(&artwork_manifest)?;

        let member_galleries = gallery_membership_by_artwork
            .get(&artwork_manifest.id)
            .cloned()
            .filter(|galleries| !galleries.is_empty())
            .unwrap_or_else(|| vec![fallback_gallery.id]);
        let artwork_id = if let Some(existing_artwork_id) =
            existing_artwork_id_for_oaa_external_links(catalog, &artwork_manifest.external_links)?
        {
            for gallery_id in &member_galleries {
                catalog.link_artwork_to_gallery(*gallery_id, existing_artwork_id)?;
            }
            existing_artwork_id
        } else {
            let primary_gallery_id = member_galleries[0];
            let artwork = catalog.create_artwork_in_gallery(
                primary_gallery_id,
                &artwork_manifest.title,
                None,
            )?;
            for gallery_id in member_galleries.iter().skip(1) {
                catalog.link_artwork_to_gallery(*gallery_id, artwork.id)?;
            }
            artwork.id
        };

        save_artwork_metadata(catalog, artwork_id, &artwork_manifest)?;
        save_external_links(catalog, artwork_id, &artwork_manifest.external_links)?;
        save_extension_blocks(catalog, "artwork", artwork_id, &artwork_manifest.extensions)?;
        if let Some(public_metadata) = artwork_manifest.public_metadata.as_ref() {
            save_extension_blocks(
                catalog,
                "artwork_public_metadata",
                artwork_id,
                &public_metadata.extensions,
            )?;
        }
        if let Some(private_metadata) = artwork_manifest.private_metadata.as_ref() {
            save_extension_blocks(
                catalog,
                "artwork_private_metadata",
                artwork_id,
                &private_metadata.extensions,
            )?;
        }

        let artwork_archive_dir = parent_archive_dir(&reference.path)?;
        let artwork_folder = catalog.artwork_asset_folder(artwork_id)?;
        fs::create_dir_all(&artwork_folder)?;
        for file in &artwork_manifest.files {
            validate_archive_path(&file.relative_path, "artwork file relative path")?;
            let zip_path = join_archive_path(&artwork_archive_dir, &file.relative_path)?;
            let file_name = file
                .file_name
                .as_deref()
                .or_else(|| {
                    Path::new(&file.relative_path)
                        .file_name()
                        .and_then(|name| name.to_str())
                })
                .unwrap_or("file");
            let destination = unique_child_file(&artwork_folder, file_name)?;
            extract_zip_file(&mut zip, &zip_path, &destination)?;

            let derivative_type = app_extension_string(&file.extensions, "derivative_type");
            if file.file_kind == "derivative"
                && file.width.is_some()
                && file.height.is_some()
                && derivative_type.as_deref() != Some("png_export")
            {
                let source_file_asset_id = file
                    .extensions
                    .get("app.oa-curator")
                    .and_then(|extension| extension.get("source_file_id"))
                    .and_then(Value::as_str)
                    .and_then(|source_file_id| source_file_ids_by_oaa_file_id.get(source_file_id))
                    .copied();
                let image_role = manifest_image_role(file.image_role.as_deref(), &file.extensions);
                let derivative = catalog.add_derived_asset(
                    artwork_id,
                    DerivedAssetInsert {
                        source_file_asset_id,
                        derivative_type: derivative_type.as_deref().unwrap_or("oaa_derivative"),
                        format: file
                            .format
                            .as_deref()
                            .or_else(|| destination.extension().and_then(|value| value.to_str()))
                            .unwrap_or("file"),
                        path: &destination,
                        width: file.width.unwrap_or_default(),
                        height: file.height.unwrap_or_default(),
                        image_role,
                    },
                )?;
                save_extension_blocks(catalog, "derived_asset", derivative.id, &file.extensions)?;
            } else {
                let image_role = manifest_image_role(file.image_role.as_deref(), &file.extensions);
                let file_asset_id = catalog.upsert_file_asset_with_known_metadata(
                    artwork_id,
                    FileAssetKnownMetadataInsert {
                        original_path: &destination,
                        root: &artwork_folder,
                        path: &destination,
                        is_primary: file.is_primary.unwrap_or(false),
                        source_kind: app_extension_string(&file.extensions, "source_kind")
                            .as_deref()
                            .unwrap_or("imported"),
                        metadata: FileAssetMetadata {
                            width: file.width,
                            height: file.height,
                            dpi_x: file.dpi_x,
                            dpi_y: file.dpi_y,
                        },
                    },
                )?;
                if let Some(image_role) = image_role {
                    catalog.update_image_role(AssetKind::File, file_asset_id, Some(image_role))?;
                }
                for link in &file.external_links {
                    let extensions_value = if link.extensions.is_empty() {
                        None
                    } else {
                        Some(Value::Object(link.extensions.clone().into_iter().collect()))
                    };
                    catalog.upsert_file_asset_external_link(
                        file_asset_id,
                        &link.provider,
                        &link.id,
                        &link.url,
                        extensions_value.as_ref(),
                    )?;
                }
                save_extension_blocks(catalog, "file_asset", file_asset_id, &file.extensions)?;
                source_file_ids_by_oaa_file_id.insert(file.id.clone(), file_asset_id);
            }
            files_imported += 1;
        }
        catalog.ensure_artwork_manifest(artwork_id)?;
        artworks_imported += 1;
        progress(OaaImportProgress {
            phase: "artwork".to_string(),
            message: format!(
                "Importing OAA artwork {} of {}: {}",
                artworks_imported, artwork_total, artwork_manifest.title
            ),
            galleries_total: gallery_total,
            galleries_imported: gallery_total,
            artworks_total: artwork_total,
            artworks_imported,
            files_total: 0,
            files_imported,
            current_artwork: Some(artwork_manifest.title),
            done: false,
        });
    }

    batch_transaction.commit()?;
    progress(OaaImportProgress {
        phase: "complete".to_string(),
        message: format!(
            "Imported OAA archive: {} galleries, {} artworks, {} files",
            collection_manifest.galleries.len(),
            collection_manifest.artworks.len(),
            files_imported
        ),
        galleries_total: gallery_total,
        galleries_imported: gallery_total,
        artworks_total: artwork_total,
        artworks_imported: artwork_total,
        files_total: files_imported,
        files_imported,
        current_artwork: None,
        done: true,
    });
    Ok(OaaImportReport {
        collection_id: collection.id,
        galleries_imported: collection_manifest.galleries.len(),
        artworks_imported: collection_manifest.artworks.len(),
        files_imported,
        messages: Vec::new(),
    })
}

fn import_oaa_archive_as_new_collection<F>(
    catalog: &Catalog,
    zip: &mut ZipArchive<fs::File>,
    collection_manifest: &OaaCollectionManifest,
    options: &OaaImportOptions,
    progress: &mut F,
) -> Result<OaaImportReport>
where
    F: FnMut(OaaImportProgress),
{
    let destination_root = options.destination_root.as_ref().ok_or_else(|| {
        AppError::Message(
            "Destination folder is required when importing a new Collection".to_string(),
        )
    })?;
    fs::create_dir_all(destination_root)?;

    let validation = validate_oaa_manifest_tree(zip, collection_manifest, progress)?;
    let collection_folder = unique_child_folder(destination_root, &collection_manifest.name)?;
    let archive_entries_total = validation.allowed_entries.len();
    progress(OaaImportProgress {
        phase: "extract".to_string(),
        message: format!(
            "Extracting referenced OAA archive entries: 0 of {archive_entries_total}; {} artworks queued",
            collection_manifest.artworks.len()
        ),
        galleries_total: collection_manifest.galleries.len(),
        galleries_imported: collection_manifest.galleries.len(),
        artworks_total: collection_manifest.artworks.len(),
        artworks_imported: 0,
        files_total: archive_entries_total,
        files_imported: 0,
        current_artwork: None,
        done: false,
    });

    let extraction = extract_oaa_payload(
        zip,
        &collection_folder,
        &validation.allowed_entries,
        archive_entries_total,
        collection_manifest.galleries.len(),
        collection_manifest.artworks.len(),
        progress,
    )?;
    progress(OaaImportProgress {
        phase: "load".to_string(),
        message: "Loading imported OAA Collection manifests".to_string(),
        galleries_total: collection_manifest.galleries.len(),
        galleries_imported: collection_manifest.galleries.len(),
        artworks_total: collection_manifest.artworks.len(),
        artworks_imported: collection_manifest.artworks.len(),
        files_total: archive_entries_total,
        files_imported: extraction.extracted_entries,
        current_artwork: None,
        done: false,
    });

    let collection_path = collection_folder.join(".oacollection");
    let collection = catalog.open_collection(&collection_path)?;
    progress(OaaImportProgress {
        phase: "complete".to_string(),
        message: format!(
            "Imported OAA archive: {} galleries, {} artworks, {} files",
            collection_manifest.galleries.len(),
            collection_manifest.artworks.len(),
            validation.referenced_file_count
        ),
        galleries_total: collection_manifest.galleries.len(),
        galleries_imported: collection_manifest.galleries.len(),
        artworks_total: collection_manifest.artworks.len(),
        artworks_imported: collection_manifest.artworks.len(),
        files_total: validation.referenced_file_count,
        files_imported: validation.referenced_file_count,
        current_artwork: None,
        done: true,
    });

    let mut messages = Vec::new();
    if extraction.skipped_entries > 0 {
        let label = if extraction.skipped_entries == 1 {
            "entry"
        } else {
            "entries"
        };
        messages.push(format!(
            "Skipped {} unreferenced archive {label}.",
            extraction.skipped_entries
        ));
    }

    Ok(OaaImportReport {
        collection_id: collection.id,
        galleries_imported: collection_manifest.galleries.len(),
        artworks_imported: collection_manifest.artworks.len(),
        files_imported: validation.referenced_file_count,
        messages,
    })
}

struct OaaManifestValidation {
    referenced_file_count: usize,
    allowed_entries: BTreeSet<String>,
}

fn validate_oaa_manifest_tree<F>(
    zip: &mut ZipArchive<fs::File>,
    collection_manifest: &OaaCollectionManifest,
    progress: &mut F,
) -> Result<OaaManifestValidation>
where
    F: FnMut(OaaImportProgress),
{
    let gallery_total = collection_manifest.galleries.len();
    let artwork_total = collection_manifest.artworks.len();
    let mut allowed_entries = BTreeSet::from([".oacollection".to_string()]);
    for (index, reference) in collection_manifest.galleries.iter().enumerate() {
        validate_archive_path(&reference.path, "collection gallery reference path")?;
        allowed_entries.insert(reference.path.clone());
        let gallery_manifest: OaaGalleryManifest =
            read_zip_json(zip, &reference.path, "gallery manifest")?;
        validate_schema(&gallery_manifest.schema_version, &reference.path)?;
        validate_manifest_value(&serde_json::to_value(&gallery_manifest)?, &reference.path)?;
        if gallery_manifest.id != reference.id {
            return Err(AppError::Message(format!(
                "Gallery manifest ID mismatch for {}",
                reference.path
            )));
        }
        progress(OaaImportProgress {
            phase: "validate".to_string(),
            message: format!(
                "Validating OAA Gallery {} of {}: {}",
                index + 1,
                gallery_total,
                gallery_manifest.name
            ),
            galleries_total: gallery_total,
            galleries_imported: index + 1,
            artworks_total: artwork_total,
            artworks_imported: 0,
            files_total: 0,
            files_imported: 0,
            current_artwork: None,
            done: false,
        });
    }

    let mut files_total = 0usize;
    for (index, reference) in collection_manifest.artworks.iter().enumerate() {
        validate_archive_path(&reference.path, "collection artwork reference path")?;
        allowed_entries.insert(reference.path.clone());
        let artwork_manifest: OaaArtworkManifest =
            read_zip_json(zip, &reference.path, "artwork manifest")?;
        validate_schema(&artwork_manifest.schema_version, &reference.path)?;
        validate_manifest_value(&serde_json::to_value(&artwork_manifest)?, &reference.path)?;
        if artwork_manifest.id != reference.id {
            return Err(AppError::Message(format!(
                "Artwork manifest ID mismatch for {}",
                reference.path
            )));
        }
        validate_file_ids(&artwork_manifest)?;
        let artwork_archive_dir = parent_archive_dir(&reference.path)?;
        for file in &artwork_manifest.files {
            validate_archive_path(&file.relative_path, "artwork file relative path")?;
            allowed_entries.insert(join_archive_path(
                &artwork_archive_dir,
                &file.relative_path,
            )?);
        }
        files_total += artwork_manifest.files.len();
        progress(OaaImportProgress {
            phase: "validate".to_string(),
            message: format!(
                "Validating OAA artwork {} of {}: {}",
                index + 1,
                artwork_total,
                artwork_manifest.title
            ),
            galleries_total: gallery_total,
            galleries_imported: gallery_total,
            artworks_total: artwork_total,
            artworks_imported: index + 1,
            files_total,
            files_imported: 0,
            current_artwork: Some(artwork_manifest.title),
            done: false,
        });
    }
    validate_required_archive_entries(zip, &allowed_entries)?;
    Ok(OaaManifestValidation {
        referenced_file_count: files_total,
        allowed_entries,
    })
}

struct OaaPayloadExtraction {
    extracted_entries: usize,
    skipped_entries: usize,
}

fn extract_oaa_payload<F>(
    zip: &mut ZipArchive<fs::File>,
    collection_folder: &Path,
    allowed_entries: &BTreeSet<String>,
    archive_entries_total: usize,
    gallery_total: usize,
    artwork_total: usize,
    progress: &mut F,
) -> Result<OaaPayloadExtraction>
where
    F: FnMut(OaaImportProgress),
{
    let mut extracted = 0usize;
    let mut skipped = 0usize;
    for index in 0..zip.len() {
        let mut source = zip.by_index(index)?;
        let name = source.name().to_string();
        if source.is_dir() || name == "mimetype" {
            continue;
        }
        validate_archive_path(&name, "archive entry")?;
        if !allowed_entries.contains(&name) {
            skipped += 1;
            continue;
        }
        let destination = collection_folder.join(PathBuf::from(
            name.replace('/', std::path::MAIN_SEPARATOR_STR),
        ));
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut output = fs::File::create(&destination)?;
        std::io::copy(&mut source, &mut output)?;
        extracted += 1;
        progress(OaaImportProgress {
            phase: "extract".to_string(),
            message: format!(
                "Extracting OAA archive entry {} of {}: {}",
                extracted, archive_entries_total, name
            ),
            galleries_total: gallery_total,
            galleries_imported: gallery_total,
            artworks_total: artwork_total,
            artworks_imported: 0,
            files_total: archive_entries_total,
            files_imported: extracted,
            current_artwork: None,
            done: false,
        });
    }
    Ok(OaaPayloadExtraction {
        extracted_entries: extracted,
        skipped_entries: skipped,
    })
}

pub fn export_oaa_archive(catalog: &Catalog, options: OaaExportOptions) -> Result<OaaExportReport> {
    export_oaa_archive_with_progress(catalog, options, |_| {})
}

pub fn export_oaa_archive_with_progress<F>(
    catalog: &Catalog,
    options: OaaExportOptions,
    mut progress: F,
) -> Result<OaaExportReport>
where
    F: FnMut(OaaExportProgress),
{
    let policy =
        ExportPolicy::private_backup(options.include_private_metadata, options.allow_overwrite);
    policy.verify_private_metadata_export()?;
    prepare_archive_destination(&options.archive_path, policy.allow_overwrite)?;

    let final_archive_path = options.archive_path.clone();
    let temporary_archive_path = temporary_archive_path(&final_archive_path)?;

    if final_archive_path.try_exists()? && !policy.allow_overwrite {
        return Err(AppError::Message(format!(
            "OAA archive already exists: {}",
            final_archive_path.display()
        )));
    }

    let snapshot = oaa_export_snapshot(catalog, options.collection_id)?;
    let total_steps = snapshot.galleries.len() + snapshot.artwork_by_id.len() + 2;
    let mut current_step = 0usize;
    progress(OaaExportProgress {
        phase: "prepare".to_string(),
        message: "Preparing OAA archive".to_string(),
        current: current_step,
        total: total_steps,
    });

    if let Some(parent) = temporary_archive_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)?;
    }
    let file = fs::File::create(&temporary_archive_path)?;
    let mut zip = ZipWriter::new(file);
    let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    let deflated = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

    zip.start_file("mimetype", stored)?;
    zip.write_all(OAA_MEDIA_TYPE.as_bytes())?;

    let mut gallery_refs = Vec::new();
    for gallery in &snapshot.galleries {
        gallery_refs.push(OaaCollectionGalleryRef {
            id: gallery.stable_id.clone(),
            name: gallery.name.clone(),
            path: gallery_manifest_archive_path(gallery),
            extensions: BTreeMap::new(),
        });
    }

    let mut artwork_refs = Vec::new();
    for artwork in snapshot.artwork_by_id.values() {
        artwork_refs.push(OaaCollectionArtworkRef {
            id: artwork.canonical_id.clone(),
            path: artwork_manifest_archive_path(&artwork.canonical_id),
            extensions: BTreeMap::new(),
        });
    }

    let collection_manifest = OaaCollectionManifest {
        schema_version: OAA_SCHEMA_VERSION.to_string(),
        id: snapshot.collection.stable_id.clone(),
        name: snapshot.collection.name.clone(),
        external_links: collection_external_links(&snapshot.collection),
        galleries: gallery_refs,
        artworks: artwork_refs,
        extensions: extension_blocks_map(catalog, "collection", snapshot.collection.id)?,
    };
    write_zip_json(&mut zip, ".oacollection", &collection_manifest, deflated)?;
    current_step += 1;
    progress(OaaExportProgress {
        phase: "collection".to_string(),
        message: "Wrote Collection manifest".to_string(),
        current: current_step,
        total: total_steps,
    });

    for gallery in &snapshot.galleries {
        let artworks = snapshot
            .gallery_artworks
            .get(&gallery.id)
            .cloned()
            .unwrap_or_default();
        let gallery_manifest = OaaGalleryManifest {
            schema_version: OAA_SCHEMA_VERSION.to_string(),
            id: gallery.stable_id.clone(),
            name: gallery.name.clone(),
            external_links: gallery_external_links(
                gallery,
                snapshot.collection.raremarq_collection_id.as_deref(),
            ),
            artworks: artworks
                .iter()
                .map(|artwork| OaaGalleryArtworkRef {
                    id: artwork.canonical_id.clone(),
                    extensions: BTreeMap::new(),
                })
                .collect(),
            extensions: extension_blocks_map(catalog, "gallery", gallery.id)?,
        };
        write_zip_json(
            &mut zip,
            &gallery_manifest_archive_path(gallery),
            &gallery_manifest,
            deflated,
        )?;
        current_step += 1;
        progress(OaaExportProgress {
            phase: "gallery".to_string(),
            message: format!("Wrote Gallery {}", gallery.name),
            current: current_step,
            total: total_steps,
        });
    }

    let mut files_exported = 0usize;
    let mut emitted_paths = BTreeSet::new();
    for detail in snapshot.artwork_details.values() {
        let artwork_dir = format!("artworks/{}/", safe_archive_segment(&detail.canonical_id));
        let mut file_objects = Vec::new();

        if options.include_images {
            for file_asset in &detail.file_assets {
                let relative_path = unique_archive_file_name(
                    &mut emitted_paths,
                    &artwork_dir,
                    &file_asset.file_name,
                );
                add_file_to_zip(
                    &mut zip,
                    &file_asset.current_path,
                    &format!("{artwork_dir}{relative_path}"),
                    stored,
                )?;
                file_objects.push(file_object_for_file_asset(
                    catalog,
                    file_asset,
                    &relative_path,
                )?);
                files_exported += 1;
            }

            for derived_asset in &detail.derived_assets {
                if derived_asset.derivative_type == "thumbnail"
                    || derived_asset.derivative_type == "preview"
                {
                    continue;
                }
                let file_name = derived_asset
                    .path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("derivative");
                let relative_path =
                    unique_archive_file_name(&mut emitted_paths, &artwork_dir, file_name);
                add_file_to_zip(
                    &mut zip,
                    &derived_asset.path,
                    &format!("{artwork_dir}{relative_path}"),
                    stored,
                )?;
                file_objects.push(file_object_for_derived_asset(
                    catalog,
                    derived_asset,
                    &relative_path,
                )?);
                files_exported += 1;
            }
        }

        let artwork_manifest = OaaArtworkManifest {
            schema_version: OAA_SCHEMA_VERSION.to_string(),
            id: detail.canonical_id.clone(),
            title: detail.title.clone(),
            external_links: artwork_external_links(catalog, detail.id)?,
            public_metadata: Some(public_metadata_for_detail(catalog, detail)?),
            private_metadata: if policy.include_private_metadata {
                private_metadata_for_detail(catalog, detail)?
            } else {
                None
            },
            files: file_objects,
            extensions: artwork_extension_blocks(catalog, detail)?,
        };
        write_zip_json(
            &mut zip,
            &artwork_manifest_archive_path(&detail.canonical_id),
            &artwork_manifest,
            deflated,
        )?;
        current_step += 1;
        progress(OaaExportProgress {
            phase: "artwork".to_string(),
            message: format!("Wrote Artwork {}", detail.title),
            current: current_step,
            total: total_steps,
        });
    }

    let output = zip.finish()?;
    output.sync_all()?;
    drop(output);
    place_archive_output(
        &temporary_archive_path,
        &final_archive_path,
        policy.allow_overwrite,
    )?;
    progress(OaaExportProgress {
        phase: "complete".to_string(),
        message: "OAA archive finished".to_string(),
        current: total_steps,
        total: total_steps,
    });
    Ok(OaaExportReport {
        collection_id: snapshot.collection.id,
        archive_path: final_archive_path,
        galleries_exported: snapshot.galleries.len(),
        artworks_exported: snapshot.artwork_by_id.len(),
        files_exported,
    })
}

fn oaa_export_snapshot(catalog: &Catalog, collection_id: i64) -> Result<OaaExportSnapshot> {
    let collection = catalog.collection_summary(collection_id)?;
    let galleries = catalog.galleries_for_collection(collection.id)?;
    let mut artwork_by_id = BTreeMap::new();
    let mut gallery_artworks = HashMap::new();
    for gallery in &galleries {
        let artworks = catalog.artworks_for_gallery(gallery.id)?;
        for artwork in &artworks {
            artwork_by_id.insert(artwork.id, artwork.clone());
        }
        gallery_artworks.insert(gallery.id, artworks);
    }
    let mut artwork_details = BTreeMap::new();
    for artwork_id in artwork_by_id.keys() {
        artwork_details.insert(*artwork_id, catalog.artwork_detail(*artwork_id)?);
    }
    Ok(OaaExportSnapshot {
        collection,
        galleries,
        artwork_by_id,
        gallery_artworks,
        artwork_details,
    })
}

fn prepare_archive_destination(path: &Path, allow_overwrite: bool) -> Result<()> {
    if path.as_os_str().is_empty() {
        return Err(AppError::Message(
            "OAA archive path cannot be empty".to_string(),
        ));
    }
    if path.try_exists()? && !allow_overwrite {
        return Err(AppError::Message(format!(
            "OAA archive already exists: {}",
            path.display()
        )));
    }
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)?;
    }
    Ok(())
}

fn temporary_archive_path(destination: &Path) -> Result<PathBuf> {
    let file_name = destination
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            AppError::Message(format!(
                "OAA archive path does not include a file name: {}",
                destination.display()
            ))
        })?;
    let parent = destination
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    Ok(parent.join(format!(".{file_name}.{}.tmp", archive_temp_stamp())))
}

fn place_archive_output(
    temporary_path: &Path,
    destination: &Path,
    allow_overwrite: bool,
) -> Result<()> {
    if allow_overwrite {
        replace_archive_with_temp(temporary_path, destination)
    } else {
        create_archive_from_temp_without_overwrite(temporary_path, destination)
    }
}

fn replace_archive_with_temp(temporary_path: &Path, destination: &Path) -> Result<()> {
    #[cfg(target_os = "windows")]
    if destination.try_exists()? {
        fs::remove_file(destination)?;
    }
    fs::rename(temporary_path, destination)?;
    Ok(())
}

fn create_archive_from_temp_without_overwrite(
    temporary_path: &Path,
    destination: &Path,
) -> Result<()> {
    let mut source = fs::File::open(temporary_path)?;
    let mut destination_file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)
        .map_err(|error| {
            if error.kind() == io::ErrorKind::AlreadyExists {
                AppError::Message(format!(
                    "OAA archive already exists: {}",
                    destination.display()
                ))
            } else {
                AppError::Io(error)
            }
        })?;
    let copy_result = io::copy(&mut source, &mut destination_file)
        .and_then(|_| destination_file.flush())
        .and_then(|_| destination_file.sync_all());
    if let Err(error) = copy_result {
        let _ = fs::remove_file(destination);
        return Err(AppError::Io(error));
    }
    fs::remove_file(temporary_path)?;
    Ok(())
}

fn archive_temp_stamp() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default()
}

fn save_artwork_metadata(
    catalog: &Catalog,
    artwork_id: i64,
    manifest: &OaaArtworkManifest,
) -> Result<()> {
    let public = manifest.public_metadata.clone().unwrap_or_default();
    let private = manifest.private_metadata.clone().unwrap_or_default();
    let caf_extension = public.extensions.get("com.comicartfans");
    let caf_csv_image_link = caf_extension
        .and_then(|extension| extension.get("csv_image_link"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let caf_csv_added_to_caf = caf_extension
        .and_then(|extension| extension.get("csv_added_to_caf"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let snikt_csv_created_date = public
        .extensions
        .get("com.snikt")
        .and_then(|extension| extension.get("metadata"))
        .and_then(|metadata| metadata.get("csv_created_date"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let publication_status_id = caf_extension
        .and_then(|extension| extension.get("publication_status_id"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| match public.publication_status.as_deref() {
            Some("published_art") => Some("1".to_string()),
            Some("unpublished_art") => Some("2".to_string()),
            _ => None,
        });
    let update = MetadataUpdate {
        artwork_id,
        title: manifest.title.clone(),
        description: public.description,
        for_sale_status: public.for_sale_status,
        media_type_id: caf_extension
            .and_then(|extension| extension.get("media_type_id"))
            .and_then(Value::as_str)
            .map(str::to_string)
            .or_else(|| {
                public
                    .media
                    .as_deref()
                    .and_then(media_type_id_for_label)
                    .map(str::to_string)
            }),
        art_type_id: caf_extension
            .and_then(|extension| extension.get("art_type_id"))
            .and_then(Value::as_str)
            .map(str::to_string)
            .or_else(|| {
                public
                    .artwork_type
                    .as_deref()
                    .and_then(art_type_id_for_label)
                    .map(str::to_string)
            }),
        publication_status_id,
        active: public.is_public.unwrap_or(true),
        illustration_exchange: caf_extension
            .and_then(|extension| extension.get("illustration_exchange"))
            .and_then(Value::as_bool)
            .unwrap_or(false),
        ix_for_sale: caf_extension
            .and_then(|extension| extension.get("ix_for_sale"))
            .and_then(Value::as_bool)
            .unwrap_or(false),
        artist_credits: public
            .artist_credits
            .iter()
            .map(|credit| {
                let role_id = credit
                    .role
                    .as_deref()
                    .and_then(artist_role_id_for_label)
                    .map(str::to_string);
                ArtistCreditUpdate {
                    first_name: credit.first_name.clone(),
                    last_name: credit.last_name.clone(),
                    role_id,
                }
            })
            .collect(),
        media: public.media,
        format: public.artwork_type,
        caf_url: provider_url(&manifest.external_links, "com.comicartfans"),
        snikt_url: provider_url(&manifest.external_links, "com.snikt"),
        raremarq_url: provider_url(&manifest.external_links, "com.raremarq"),
        generic_url: app_extension_string(&manifest.extensions, "generic_url"),
        snikt_metadata: public
            .extensions
            .get("com.snikt")
            .and_then(|extension| extension.get("metadata"))
            .cloned()
            .map(serde_json::from_value)
            .transpose()?,
        purchase_price: private.purchase_price,
        estimated_value: private.estimated_value,
        purchase_date: private.purchase_date,
        provenance: private.provenance,
        personal_notes: private.personal_notes,
    };
    catalog.save_metadata(update)?;
    catalog.update_caf_csv_tracking(
        artwork_id,
        caf_csv_image_link.as_deref(),
        caf_csv_added_to_caf.as_deref(),
    )?;
    catalog.update_snikt_csv_tracking(artwork_id, snikt_csv_created_date.as_deref())
}

fn save_external_links(
    catalog: &Catalog,
    artwork_id: i64,
    links: &[OaaExternalLink],
) -> Result<()> {
    for link in links {
        let extensions_value = if link.extensions.is_empty() {
            None
        } else {
            Some(Value::Object(link.extensions.clone().into_iter().collect()))
        };
        catalog.upsert_artwork_external_link(
            artwork_id,
            oac_link_type_for_provider(&link.provider),
            Some(&link.id),
            &link.url,
            extensions_value.as_ref(),
        )?;
    }
    Ok(())
}

fn existing_artwork_id_for_oaa_external_links(
    catalog: &Catalog,
    links: &[OaaExternalLink],
) -> Result<Option<i64>> {
    let mut matched_artwork_id = None;
    for link in links {
        let Some(artwork_id) = catalog
            .artwork_id_for_external_id(oac_link_type_for_provider(&link.provider), &link.id)?
        else {
            continue;
        };
        if matched_artwork_id.is_some_and(|matched_id| matched_id != artwork_id) {
            return Err(AppError::Message(format!(
                "OAA artwork external links resolve to multiple existing Artworks: {}",
                link.id
            )));
        }
        matched_artwork_id = Some(artwork_id);
    }
    Ok(matched_artwork_id)
}

fn save_extension_blocks(
    catalog: &Catalog,
    owner_kind: &str,
    owner_id: i64,
    extensions: &BTreeMap<String, Value>,
) -> Result<()> {
    for (provider, value) in extensions {
        catalog.save_oaa_extension_block(owner_kind, owner_id, provider, value)?;
    }
    Ok(())
}

fn collection_external_links(
    collection: &crate::catalog::CollectionSummary,
) -> Vec<OaaExternalLink> {
    let mut links = Vec::new();
    if let Some(id) = collection.caf_collection_id.as_deref() {
        links.push(OaaExternalLink {
            provider: "com.comicartfans".to_string(),
            id: id.to_string(),
            url: format!("https://www.comicartfans.com/GalleryDetail.asp?GCat={id}"),
            extensions: BTreeMap::new(),
        });
    }
    if let Some(id) = collection.snikt_collection_id.as_deref() {
        links.push(OaaExternalLink {
            provider: "com.snikt".to_string(),
            id: id.to_string(),
            url: format!("https://www.snikt.com/user/{id}"),
            extensions: BTreeMap::new(),
        });
    }
    if let Some(id) = collection.raremarq_collection_id.as_deref() {
        links.push(OaaExternalLink {
            provider: "com.raremarq".to_string(),
            id: id.to_string(),
            url: format!("https://www.raremarq.com/u/{id}"),
            extensions: BTreeMap::new(),
        });
    }
    links
}

fn gallery_external_links(
    gallery: &GallerySummary,
    raremarq_collection_id: Option<&str>,
) -> Vec<OaaExternalLink> {
    let mut links = Vec::new();
    if let Some(id) = gallery.caf_gallery_room_id.as_deref() {
        links.push(OaaExternalLink {
            provider: "com.comicartfans".to_string(),
            id: id.to_string(),
            url: format!("https://www.comicartfans.com/my/GalleryRoom.asp?GSub={id}"),
            extensions: BTreeMap::new(),
        });
    }
    if let Some(id) = gallery.snikt_gallery_id.as_deref() {
        links.push(OaaExternalLink {
            provider: "com.snikt".to_string(),
            id: id.to_string(),
            url: format!("https://www.snikt.com/user/{id}"),
            extensions: BTreeMap::new(),
        });
    }
    if let (Some(id), Some(collection_id)) = (
        gallery.raremarq_gallery_id.as_deref(),
        raremarq_collection_id,
    ) {
        links.push(OaaExternalLink {
            provider: "com.raremarq".to_string(),
            id: id.to_string(),
            url: format!("https://www.raremarq.com/u/{collection_id}/galleries/{id}"),
            extensions: BTreeMap::new(),
        });
    }
    links
}

fn artwork_external_links(catalog: &Catalog, artwork_id: i64) -> Result<Vec<OaaExternalLink>> {
    let mut links = Vec::new();
    for link in catalog.artwork_external_links(artwork_id)? {
        let provider = provider_for_oac_link_type(&link.provider);
        if provider == "com.comicartfans.image" || provider == "com.comicartfans.thumbnail" {
            continue;
        }
        let Some(id) = link.external_id else {
            continue;
        };
        links.push(OaaExternalLink {
            provider: provider.to_string(),
            id,
            url: link.url,
            extensions: object_to_extension_map(link.extensions),
        });
    }
    Ok(links)
}

fn public_metadata_for_detail(
    catalog: &Catalog,
    detail: &crate::catalog::ArtworkDetail,
) -> Result<OaaPublicMetadata> {
    let mut extensions = extension_blocks_map(catalog, "artwork_public_metadata", detail.id)?;
    let caf_extension = caf_public_extension(detail);
    if !caf_extension.is_null() {
        extensions.insert("com.comicartfans".to_string(), caf_extension);
    }
    let snikt_extension = snikt_public_extension(detail);
    if !snikt_extension.is_null() {
        extensions.insert("com.snikt".to_string(), snikt_extension);
    }
    let publication_status = match detail.publication_status_id.as_deref() {
        Some("1") => Some("published_art".to_string()),
        Some("2") => Some("unpublished_art".to_string()),
        _ => None,
    };
    Ok(OaaPublicMetadata {
        description: detail.description.clone(),
        for_sale_status: detail.for_sale_status.clone(),
        media: detail.media.clone(),
        artwork_type: detail.format.clone(),
        publication_status,
        is_public: Some(detail.active),
        artist_credits: detail
            .artist_credits
            .iter()
            .map(|credit| OaaArtistCredit {
                first_name: credit.first_name.clone(),
                last_name: credit.last_name.clone(),
                role: credit.role.clone(),
                extensions: BTreeMap::new(),
            })
            .collect(),
        extensions,
    })
}

fn private_metadata_for_detail(
    catalog: &Catalog,
    detail: &crate::catalog::ArtworkDetail,
) -> Result<Option<OaaPrivateMetadata>> {
    let extensions = extension_blocks_map(catalog, "artwork_private_metadata", detail.id)?;
    if detail.purchase_price.is_none()
        && detail.estimated_value.is_none()
        && detail.purchase_date.is_none()
        && detail.provenance.is_none()
        && detail.personal_notes.is_none()
        && extensions.is_empty()
    {
        return Ok(None);
    }
    Ok(Some(OaaPrivateMetadata {
        purchase_price: detail.purchase_price.clone(),
        estimated_value: detail.estimated_value.clone(),
        purchase_date: detail.purchase_date.clone(),
        provenance: detail.provenance.clone(),
        personal_notes: detail.personal_notes.clone(),
        extensions,
    }))
}

fn artwork_extension_blocks(
    catalog: &Catalog,
    detail: &crate::catalog::ArtworkDetail,
) -> Result<BTreeMap<String, Value>> {
    let mut extensions = extension_blocks_map(catalog, "artwork", detail.id)?;
    let app_extension = extensions
        .entry("app.oa-curator".to_string())
        .or_insert_with(|| Value::Object(Default::default()));
    let Some(app_object) = app_extension.as_object_mut() else {
        return Ok(extensions);
    };
    app_object
        .entry("artwork_id".to_string())
        .or_insert_with(|| Value::from(detail.id));
    if let Some(generic_url) = detail.generic_url.clone() {
        app_object.insert("generic_url".to_string(), Value::from(generic_url));
    } else {
        app_object.remove("generic_url");
    }
    Ok(extensions)
}

fn file_object_for_file_asset(
    catalog: &Catalog,
    asset: &FileAsset,
    relative_path: &str,
) -> Result<OaaFileObject> {
    let mut extensions = extension_blocks_map(catalog, "file_asset", asset.id)?;
    let mut app_extension = extensions
        .remove("app.oa-curator")
        .unwrap_or_else(|| Value::Object(Default::default()));
    if let Value::Object(map) = &mut app_extension {
        map.insert("file_asset_id".to_string(), Value::from(asset.id));
        map.insert(
            "source_kind".to_string(),
            Value::from(asset.source_kind.clone()),
        );
    }
    extensions.insert("app.oa-curator".to_string(), app_extension);
    let image_role = portable_image_role(asset.image_role.as_deref(), &mut extensions);

    Ok(OaaFileObject {
        id: format!("file-{}", asset.id),
        relative_path: relative_path.to_string(),
        file_kind: if asset.width.is_some() && asset.height.is_some() {
            "raw".to_string()
        } else {
            "supporting".to_string()
        },
        file_name: Some(asset.file_name.clone()),
        size_bytes: Some(asset.size_bytes),
        width: asset.width,
        height: asset.height,
        dpi_x: asset.dpi_x,
        dpi_y: asset.dpi_y,
        format: Some(asset.extension.to_uppercase()),
        media_type: media_type_for_extension(&asset.extension).map(str::to_string),
        is_primary: Some(asset.is_primary),
        image_role,
        external_links: catalog
            .file_asset_external_links(asset.id)?
            .into_iter()
            .map(|link| OaaExternalLink {
                provider: link.provider,
                id: link.external_id,
                url: link.url,
                extensions: object_to_extension_map(link.extensions),
            })
            .collect(),
        extensions,
    })
}

fn file_object_for_derived_asset(
    catalog: &Catalog,
    asset: &crate::catalog::DerivedAsset,
    relative_path: &str,
) -> Result<OaaFileObject> {
    let mut extensions = extension_blocks_map(catalog, "derived_asset", asset.id)?;
    let mut app_extension = extensions
        .remove("app.oa-curator")
        .unwrap_or_else(|| Value::Object(Default::default()));
    if let Value::Object(map) = &mut app_extension {
        map.insert("derived_asset_id".to_string(), Value::from(asset.id));
        map.insert(
            "derivative_type".to_string(),
            Value::from(asset.derivative_type.clone()),
        );
        if let Some(source_file_asset_id) = asset.source_file_asset_id {
            map.insert(
                "source_file_asset_id".to_string(),
                Value::from(source_file_asset_id),
            );
        }
    }
    extensions.insert("app.oa-curator".to_string(), app_extension);
    Ok(OaaFileObject {
        id: format!("derived-{}", asset.id),
        relative_path: relative_path.to_string(),
        file_kind: "derivative".to_string(),
        file_name: asset
            .path
            .file_name()
            .and_then(|name| name.to_str())
            .map(str::to_string),
        size_bytes: fs::metadata(&asset.path)
            .ok()
            .map(|metadata| metadata.len() as i64),
        width: Some(asset.width),
        height: Some(asset.height),
        dpi_x: None,
        dpi_y: None,
        format: Some(asset.format.clone()),
        media_type: media_type_for_extension(&asset.format).map(str::to_string),
        is_primary: Some(false),
        image_role: portable_image_role(asset.image_role.as_deref(), &mut extensions),
        external_links: Vec::new(),
        extensions,
    })
}

fn portable_image_role(
    image_role: Option<&str>,
    extensions: &mut BTreeMap<String, Value>,
) -> Option<String> {
    match image_role {
        Some("basic") | Some("caf_basic") => {
            extensions
                .entry("com.comicartfans".to_string())
                .or_insert_with(|| Value::Object(Default::default()))
                .as_object_mut()
                .expect("object")
                .insert("format_tier".to_string(), Value::from("basic"));
            None
        }
        Some("premium") | Some("caf_premium") => {
            extensions
                .entry("com.comicartfans".to_string())
                .or_insert_with(|| Value::Object(Default::default()))
                .as_object_mut()
                .expect("object")
                .insert("format_tier".to_string(), Value::from("premium"));
            None
        }
        other => other.map(str::to_string),
    }
}

fn manifest_image_role<'a>(
    image_role: Option<&'a str>,
    extensions: &'a BTreeMap<String, Value>,
) -> Option<&'a str> {
    image_role.or_else(|| {
        extensions
            .get("com.comicartfans")
            .and_then(|extension| extension.get("format_tier"))
            .and_then(Value::as_str)
    })
}

fn caf_public_extension(detail: &crate::catalog::ArtworkDetail) -> Value {
    let mut map = serde_json::Map::new();
    if let Some(value) = detail.media_type_id.as_deref() {
        map.insert("media_type_id".to_string(), Value::from(value));
    }
    if let Some(value) = detail.art_type_id.as_deref() {
        map.insert("art_type_id".to_string(), Value::from(value));
    }
    if let Some(value) = detail.publication_status_id.as_deref() {
        map.insert("publication_status_id".to_string(), Value::from(value));
        if value == "3" {
            map.insert(
                "publication_status".to_string(),
                Value::from("CAF Member Art"),
            );
        }
    }
    if let Some(value) = detail.caf_csv_image_link.as_deref() {
        map.insert("csv_image_link".to_string(), Value::from(value));
    }
    if let Some(value) = detail.caf_csv_added_to_caf.as_deref() {
        map.insert("csv_added_to_caf".to_string(), Value::from(value));
    }
    map.insert(
        "illustration_exchange".to_string(),
        Value::from(detail.illustration_exchange),
    );
    map.insert("ix_for_sale".to_string(), Value::from(detail.ix_for_sale));
    Value::Object(map)
}

fn snikt_public_extension(detail: &crate::catalog::ArtworkDetail) -> Value {
    let metadata = &detail.snikt_metadata;
    let metadata_value = serde_json::to_value(metadata).unwrap_or(Value::Null);
    let Some(map) = metadata_value.as_object() else {
        return Value::Null;
    };
    if map
        .values()
        .all(|value| matches!(value, Value::Null) || matches!(value, Value::Bool(false)))
        && detail.snikt_csv_created_date.is_none()
    {
        return Value::Null;
    }
    let mut metadata = serde_json::to_value(metadata).unwrap_or(Value::Null);
    if let (Value::Object(map), Some(created_date)) =
        (&mut metadata, detail.snikt_csv_created_date.as_deref())
    {
        map.insert("csv_created_date".to_string(), Value::from(created_date));
    }
    serde_json::json!({ "metadata": metadata })
}

fn extension_blocks_map(
    catalog: &Catalog,
    owner_kind: &str,
    owner_id: i64,
) -> Result<BTreeMap<String, Value>> {
    Ok(catalog
        .oaa_extension_blocks(owner_kind, owner_id)?
        .into_iter()
        .collect())
}

fn object_to_extension_map(value: Option<Value>) -> BTreeMap<String, Value> {
    match value {
        Some(Value::Object(map)) => map.into_iter().collect(),
        _ => BTreeMap::new(),
    }
}

fn provider_id(links: &[OaaExternalLink], provider: &str) -> Option<String> {
    links
        .iter()
        .find(|link| link.provider == provider)
        .map(|link| link.id.clone())
}

fn provider_url(links: &[OaaExternalLink], provider: &str) -> Option<String> {
    links
        .iter()
        .find(|link| link.provider == provider)
        .map(|link| link.url.clone())
}

fn app_extension_string(extensions: &BTreeMap<String, Value>, key: &str) -> Option<String> {
    extensions
        .get("app.oa-curator")
        .and_then(|extension| extension.get(key))
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn oac_link_type_for_provider(provider: &str) -> &str {
    match provider {
        "com.comicartfans" => "caf",
        "com.snikt" => "snikt",
        "com.raremarq" => "raremarq",
        other => other,
    }
}

fn provider_for_oac_link_type(link_type: &str) -> &str {
    match link_type {
        "caf" => "com.comicartfans",
        "snikt" => "com.snikt",
        "raremarq" => "com.raremarq",
        "caf_image" => "com.comicartfans.image",
        "caf_image_thumbnail" => "com.comicartfans.thumbnail",
        other => other,
    }
}

fn validate_zip_entries(zip: &mut ZipArchive<fs::File>) -> Result<()> {
    let mut seen = BTreeSet::new();
    for index in 0..zip.len() {
        let file = zip.by_index(index)?;
        let name = file.name().to_string();
        if !seen.insert(name.clone()) {
            return Err(AppError::Message(format!(
                "Duplicate OAA archive entry: {name}"
            )));
        }
        if file.is_dir() {
            validate_archive_directory_entry(&name)?;
        } else {
            validate_archive_path(&name, "archive entry")?;
        }
    }
    Ok(())
}

fn validate_archive_directory_entry(path: &str) -> Result<()> {
    let trimmed = path.trim_end_matches('/');
    if trimmed.is_empty() {
        return Ok(());
    }
    validate_archive_path(trimmed, "archive directory entry")
}

fn validate_archive_path(path: &str, label: &str) -> Result<()> {
    if path.is_empty()
        || path.starts_with('/')
        || path.contains('\\')
        || path
            .split('/')
            .any(|segment| !is_safe_archive_path_component(segment))
    {
        return Err(AppError::Message(format!("Unsafe {label}: {path}")));
    }
    Ok(())
}

fn validate_required_archive_entries(
    zip: &mut ZipArchive<fs::File>,
    required_entries: &BTreeSet<String>,
) -> Result<()> {
    for entry in required_entries {
        match zip.by_name(entry) {
            Ok(_) => {}
            Err(zip::result::ZipError::FileNotFound) => {
                return Err(AppError::Message(format!(
                    "Missing referenced OAA archive entry: {entry}"
                )));
            }
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

fn validate_schema(schema_version: &str, path: &str) -> Result<()> {
    if schema_version != OAA_SCHEMA_VERSION {
        return Err(AppError::Message(format!(
            "Unsupported OAA schema version in {path}: {schema_version}"
        )));
    }
    Ok(())
}

fn validate_manifest_value(value: &Value, path: &str) -> Result<()> {
    match value {
        Value::String(value) if is_apparent_local_path(value) => Err(AppError::Message(format!(
            "OAA manifest contains a local filesystem path in {path}: {value}"
        ))),
        Value::Array(values) => {
            for value in values {
                validate_manifest_value(value, path)?;
            }
            Ok(())
        }
        Value::Object(values) => {
            for value in values.values() {
                validate_manifest_value(value, path)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

fn is_apparent_local_path(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    lower.starts_with("file://")
        || value.starts_with("\\\\")
        || value.starts_with('/')
        || (value.len() >= 3
            && value.as_bytes()[1] == b':'
            && matches!(value.as_bytes()[2], b'\\' | b'/'))
}

fn validate_file_ids(manifest: &OaaArtworkManifest) -> Result<()> {
    let mut seen = BTreeSet::new();
    for file in &manifest.files {
        if !seen.insert(file.id.clone()) {
            return Err(AppError::Message(format!(
                "Duplicate OAA file ID in {}: {}",
                manifest.id, file.id
            )));
        }
    }
    Ok(())
}

fn read_zip_json<T: for<'de> Deserialize<'de>>(
    zip: &mut ZipArchive<fs::File>,
    path: &str,
    label: &str,
) -> Result<T> {
    let value = read_zip_string(zip, path)?;
    serde_json::from_str(&value)
        .map_err(|error| AppError::Message(format!("Could not parse {label} at {path}: {error}")))
}

fn read_zip_string(zip: &mut ZipArchive<fs::File>, path: &str) -> Result<String> {
    let mut file = zip.by_name(path)?;
    let mut value = String::new();
    file.read_to_string(&mut value)?;
    Ok(value)
}

fn extract_zip_file(
    zip: &mut ZipArchive<fs::File>,
    zip_path: &str,
    destination: &Path,
) -> Result<()> {
    let mut source = zip.by_name(zip_path)?;
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut output = fs::File::create(destination)?;
    std::io::copy(&mut source, &mut output)?;
    Ok(())
}

fn write_zip_json<T: Serialize>(
    zip: &mut ZipWriter<fs::File>,
    path: &str,
    value: &T,
    options: SimpleFileOptions,
) -> Result<()> {
    zip.start_file(path, options)?;
    let bytes = serde_json::to_vec_pretty(value)?;
    zip.write_all(&bytes)?;
    Ok(())
}

fn add_file_to_zip(
    zip: &mut ZipWriter<fs::File>,
    source_path: &Path,
    archive_path: &str,
    options: SimpleFileOptions,
) -> Result<()> {
    zip.start_file(archive_path, options)?;
    let mut source = fs::File::open(source_path)?;
    std::io::copy(&mut source, zip)?;
    Ok(())
}

fn parent_archive_dir(path: &str) -> Result<String> {
    let Some((parent, _)) = path.rsplit_once('/') else {
        return Ok(String::new());
    };
    validate_archive_path(parent, "artwork manifest parent path")?;
    Ok(parent.to_string())
}

fn join_archive_path(parent: &str, child: &str) -> Result<String> {
    let joined = if parent.is_empty() {
        child.to_string()
    } else {
        format!("{parent}/{child}")
    };
    validate_archive_path(&joined, "resolved artwork file path")?;
    Ok(joined)
}

fn unique_child_folder(parent: &Path, name: &str) -> Result<PathBuf> {
    let base = safe_file_name(name, "Imported Collection");
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

fn unique_child_file(parent: &Path, name: &str) -> Result<PathBuf> {
    let base = safe_file_name(name, "file");
    let path = Path::new(&base);
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("file");
    let extension = path.extension().and_then(|value| value.to_str());
    for index in 0..10_000 {
        let candidate_name = if index == 0 {
            base.clone()
        } else if let Some(extension) = extension {
            format!("{stem} {index}.{extension}")
        } else {
            format!("{stem} {index}")
        };
        let candidate = parent.join(candidate_name);
        if !candidate.exists() {
            return Ok(candidate);
        }
    }
    Err(AppError::Message(format!(
        "Could not choose a unique file under {}",
        parent.display()
    )))
}

fn unique_archive_file_name(
    emitted_paths: &mut BTreeSet<String>,
    artwork_dir: &str,
    name: &str,
) -> String {
    let base = safe_file_name(name, "file");
    let path = Path::new(&base);
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("file");
    let extension = path.extension().and_then(|value| value.to_str());
    for index in 0..10_000 {
        let candidate = if index == 0 {
            base.clone()
        } else if let Some(extension) = extension {
            format!("{stem} {index}.{extension}")
        } else {
            format!("{stem} {index}")
        };
        let full_path = format!("{artwork_dir}{candidate}");
        if emitted_paths.insert(full_path) {
            return candidate;
        }
    }
    base
}

fn safe_file_name(value: &str, fallback: &str) -> String {
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

fn safe_archive_segment(value: &str) -> String {
    safe_file_name(value, "item").replace(' ', "-")
}

fn gallery_manifest_archive_path(gallery: &GallerySummary) -> String {
    format!(
        "galleries/{}/.oagallery",
        safe_archive_segment(&gallery.stable_id)
    )
}

fn artwork_manifest_archive_path(canonical_id: &str) -> String {
    format!("artworks/{}/.oaartwork", safe_archive_segment(canonical_id))
}

fn media_type_for_extension(extension: &str) -> Option<&'static str> {
    match extension
        .trim_start_matches('.')
        .to_ascii_lowercase()
        .as_str()
    {
        "jpg" | "jpeg" => Some("image/jpeg"),
        "png" => Some("image/png"),
        "tif" | "tiff" => Some("image/tiff"),
        "txt" => Some("text/plain"),
        "pdf" => Some("application/pdf"),
        _ => None,
    }
}
