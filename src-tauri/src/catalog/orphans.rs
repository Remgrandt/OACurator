use super::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UnreferencedArtworkDuplicate {
    pub canonical_id: String,
    pub title: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UnreferencedArtworkCandidate {
    pub canonical_id: String,
    pub title: Option<String>,
    pub manifest_path: PathBuf,
    pub can_import: bool,
    pub error: Option<String>,
    pub declared_file_count: usize,
    pub missing_declared_files: Vec<String>,
    pub undeclared_files: Vec<String>,
    pub duplicate_candidates: Vec<UnreferencedArtworkDuplicate>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UnreferencedArtworkReport {
    pub collection_id: i64,
    pub items: Vec<UnreferencedArtworkCandidate>,
}

impl Catalog {
    pub fn unreferenced_artworks(&self, collection_id: i64) -> Result<UnreferencedArtworkReport> {
        let collection = self.collection_summary(collection_id)?;
        let collection_manifest: CollectionManifest =
            read_json_manifest(&collection.manifest_path)?;
        let collection_root = collection.manifest_path.parent().ok_or_else(|| {
            AppError::Message(format!(
                "Collection manifest has no parent folder: {}",
                collection.manifest_path.display()
            ))
        })?;
        let artworks_root = collection_root.join("artworks");
        let referenced_paths = collection_manifest
            .artworks
            .iter()
            .filter_map(|reference| reference.path.as_deref())
            .map(|path| {
                path_key(&resolve_manifest_reference_path(
                    &collection.manifest_path,
                    path,
                ))
            })
            .collect::<BTreeSet<_>>();
        let referenced_ids = collection_manifest
            .artworks
            .iter()
            .map(|reference| reference.id.clone())
            .collect::<BTreeSet<_>>();
        let existing_artworks = self.artworks_for_collection(collection_id)?;
        let mut folders = if artworks_root.is_dir() {
            fs::read_dir(&artworks_root)?
                .filter_map(std::result::Result::ok)
                .filter_map(|entry| {
                    entry
                        .file_type()
                        .ok()
                        .filter(|kind| kind.is_dir())
                        .map(|_| entry.path())
                })
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        folders.sort();

        let mut items = Vec::new();
        for folder in folders {
            let folder_id = folder
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default()
                .to_string();
            let manifest_path = folder.join(".oaartwork");
            if referenced_paths.contains(&path_key(&manifest_path)) {
                continue;
            }
            if !manifest_path.is_file() {
                if oac_number(&folder_id).is_some() {
                    items.push(UnreferencedArtworkCandidate {
                        canonical_id: folder_id,
                        title: None,
                        manifest_path,
                        can_import: false,
                        error: Some("Artwork folder has no .oaartwork manifest".to_string()),
                        declared_file_count: 0,
                        missing_declared_files: Vec::new(),
                        undeclared_files: files_under(&folder)?,
                        duplicate_candidates: Vec::new(),
                    });
                }
                continue;
            }

            let manifest = match read_json_manifest::<ArtworkManifest>(&manifest_path) {
                Ok(manifest) => manifest,
                Err(error) => {
                    items.push(UnreferencedArtworkCandidate {
                        canonical_id: folder_id,
                        title: None,
                        manifest_path,
                        can_import: false,
                        error: Some(format!("Artwork manifest is not valid JSON: {error}")),
                        declared_file_count: 0,
                        missing_declared_files: Vec::new(),
                        undeclared_files: files_under(&folder)?,
                        duplicate_candidates: Vec::new(),
                    });
                    continue;
                }
            };

            let path_error = invalid_declared_path(&manifest);
            let identity_error = if manifest.id != folder_id {
                Some(format!(
                    "Manifest ID {} does not match folder {}",
                    manifest.id, folder_id
                ))
            } else if oac_number(&manifest.id).is_none() {
                Some(format!("Manifest ID {} is not a valid OAC ID", manifest.id))
            } else if referenced_ids.contains(&manifest.id) {
                Some(format!(
                    "Artwork ID {} is already referenced at another path",
                    manifest.id
                ))
            } else {
                None
            };
            let error = identity_error.or(path_error);
            let declared_paths = manifest
                .files
                .iter()
                .map(|file| normalized_relative_path(&file.relative_path))
                .collect::<BTreeSet<_>>();
            let all_files = files_under(&folder)?;
            let missing_declared_files = declared_paths
                .iter()
                .filter(|relative| {
                    !folder
                        .join(relative.replace('/', std::path::MAIN_SEPARATOR_STR))
                        .is_file()
                })
                .cloned()
                .collect();
            let undeclared_files = all_files
                .into_iter()
                .filter(|relative| relative != ".oaartwork" && !declared_paths.contains(relative))
                .collect();
            let duplicate_candidates = existing_artworks
                .iter()
                .filter(|artwork| artwork.title.eq_ignore_ascii_case(&manifest.title))
                .map(|artwork| UnreferencedArtworkDuplicate {
                    canonical_id: artwork.canonical_id.clone(),
                    title: artwork.title.clone(),
                })
                .collect();
            items.push(UnreferencedArtworkCandidate {
                canonical_id: manifest.id,
                title: Some(manifest.title),
                manifest_path,
                can_import: error.is_none(),
                error,
                declared_file_count: manifest.files.len(),
                missing_declared_files,
                undeclared_files,
                duplicate_candidates,
            });
        }

        Ok(UnreferencedArtworkReport {
            collection_id,
            items,
        })
    }

    pub fn import_unreferenced_artworks(
        &self,
        collection_id: i64,
        gallery_id: i64,
        manifest_paths: &[PathBuf],
    ) -> Result<WorkspaceState> {
        if manifest_paths.is_empty() {
            return Err(AppError::Message(
                "Select at least one Artwork manifest to import".to_string(),
            ));
        }
        if !self
            .galleries_for_collection(collection_id)?
            .iter()
            .any(|gallery| gallery.id == gallery_id)
        {
            return Err(AppError::Message(
                "The selected Gallery is not part of this Collection".to_string(),
            ));
        }
        let report = self.unreferenced_artworks(collection_id)?;
        let available = report
            .items
            .iter()
            .filter(|item| item.can_import)
            .map(|item| (path_key(&item.manifest_path), item))
            .collect::<BTreeMap<_, _>>();
        let selected = manifest_paths
            .iter()
            .map(|path| {
                available.get(&path_key(path)).copied().ok_or_else(|| {
                    AppError::Message(format!(
                        "Artwork manifest is no longer available for safe import: {}",
                        path.display()
                    ))
                })
            })
            .collect::<Result<Vec<_>>>()?;

        let batch_transaction = CatalogBatchTransaction::begin(self)?;
        for candidate in selected {
            let manifest: ArtworkManifest = read_json_manifest(&candidate.manifest_path)?;
            let artwork_id =
                self.upsert_artwork_manifest_row(&candidate.manifest_path, &manifest)?;
            self.link_artwork_to_gallery_session_only(gallery_id, artwork_id)?;
            let mut profile = CollectionOpenDebugProfile::default();
            self.import_artwork_manifest_payload(
                artwork_id,
                &candidate.manifest_path,
                &manifest,
                &mut profile,
            )?;
        }
        self.rewrite_gallery_manifest(gallery_id)?;
        self.rewrite_collection_manifest(collection_id)?;
        self.set_setting("active_gallery_id", &gallery_id.to_string())?;
        batch_transaction.commit()?;
        self.workspace_state()
    }
}

fn invalid_declared_path(manifest: &ArtworkManifest) -> Option<String> {
    manifest.files.iter().find_map(|file| {
        let path = PathBuf::from(
            file.relative_path
                .replace('/', std::path::MAIN_SEPARATOR_STR),
        );
        (path.is_absolute()
            || path.components().any(|component| {
                matches!(
                    component,
                    std::path::Component::ParentDir
                        | std::path::Component::Prefix(_)
                        | std::path::Component::RootDir
                )
            }))
        .then(|| {
            format!(
                "Declared file path escapes the Artwork folder: {}",
                file.relative_path
            )
        })
    })
}

fn files_under(root: &Path) -> Result<Vec<String>> {
    let mut files = Vec::new();
    collect_files(root, root, &mut files)?;
    files.sort();
    Ok(files)
}

fn collect_files(root: &Path, current: &Path, files: &mut Vec<String>) -> Result<()> {
    for entry in fs::read_dir(current)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        if kind.is_dir() {
            collect_files(root, &entry.path(), files)?;
        } else if kind.is_file() {
            let relative = entry
                .path()
                .strip_prefix(root)
                .unwrap_or(&entry.path())
                .to_path_buf();
            files.push(normalized_relative_path(&relative.to_string_lossy()));
        }
    }
    Ok(())
}

fn normalized_relative_path(path: &str) -> String {
    path.replace('\\', "/")
}

fn path_key(path: &Path) -> String {
    let key = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let key = key.to_string_lossy().replace('\\', "/");
    if cfg!(windows) {
        key.to_ascii_lowercase()
    } else {
        key
    }
}
