//! One recoverable, metadata-only transaction before loading a legacy collection.
use crate::manifest::{
    staged_manifest_bytes, write_new_json_manifest, ArtworkManifest, CollectionManifest,
    GalleryManifest,
};
use crate::oaa_archive::{reject_filesystem_redirections, validate_extraction_paths};
use crate::oaa_validation::{
    ensure_upgrade_manifests_valid, prepare_manifest_upgrade, OaaValidationLimits,
};
use crate::{AppError, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const ROOT: &str = ".oacollection";
const BACKUP_PREFIX: &str = ".oaa-1.0-backup-";
const MAX_METADATA_BYTES: usize = 64 * 1024 * 1024;
// ponytail: one process-wide upgrade at a time; use per-collection locks only if
// simultaneous collection opens become a supported workflow.
static UPGRADE_LOCK: Mutex<()> = Mutex::new(());

#[derive(Serialize, Deserialize)]
struct UpgradeManifest {
    path: String,
    before: String,
    after: String,
}

#[derive(Serialize, Deserialize)]
struct UpgradeJournal {
    format_version: u8,
    collection_file: String,
    manifests: Vec<UpgradeManifest>,
}

pub(super) fn upgrade_collection(path: &Path, mut progress: impl FnMut(&str)) -> Result<()> {
    let _lock = UPGRADE_LOCK
        .lock()
        .map_err(|_| fail("Upgrade lock is unavailable"))?;
    reject_filesystem_redirections(path)?;
    let path = fs::canonicalize(path)?;
    let root = path
        .parent()
        .ok_or_else(|| fail("Collection has no folder"))?;
    let collection_file = path
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| fail("Collection name is not UTF-8"))?;
    let pending = path.with_extension("oaa-upgrade-pending");
    reject_filesystem_redirections(&pending)?;
    if pending.try_exists()? {
        progress("Recovering interrupted OAA 1.0 collection upgrade");
        let backup_name: String = serde_json::from_str(&read_text(&pending, 4096)?)?;
        if !backup_name.starts_with(BACKUP_PREFIX) || !backup_name.ends_with(".json") {
            return Err(fail("Unrecognized upgrade recovery file"));
        }
        validate_extraction_paths(root, &BTreeSet::from([backup_name.clone()]))?;
        if Path::new(&backup_name).components().count() != 1 {
            return Err(fail("Unsafe upgrade recovery file"));
        }
        let backup = root.join(backup_name);
        let journal: UpgradeJournal =
            serde_json::from_str(&read_text(&backup, MAX_METADATA_BYTES * 8)?)?;
        if journal.format_version != 1 || journal.collection_file != collection_file {
            return Err(fail(
                "Upgrade recovery belongs to a different collection or version",
            ));
        }
        validate_journal(root, &journal)?;
        install(root, &pending, &journal)?;
        return Ok(());
    }

    let mut journal = UpgradeJournal {
        format_version: 1,
        collection_file: collection_file.into(),
        manifests: Vec::new(),
    };
    let before = read_text(
        &path,
        OaaValidationLimits::default().max_manifest_size as usize,
    )?;
    let (collection, after) = prepare_manifest_upgrade(&before)?;
    let references = manifest_paths(&collection)?;
    validate_extraction_paths(root, &references)?;
    let mut bytes = before.len();
    journal.manifests.push(UpgradeManifest {
        path: ROOT.into(),
        before,
        after,
    });
    let mut missing = None;
    for relative in references.iter().filter(|p| p.as_str() != ROOT) {
        let target = root.join(relative);
        reject_filesystem_redirections(&target)?;
        if !target.try_exists()? {
            missing = Some(relative.clone());
            continue;
        }
        let before = read_text(
            &target,
            OaaValidationLimits::default().max_manifest_size as usize,
        )?;
        bytes += before.len();
        if bytes > MAX_METADATA_BYTES {
            return Err(fail("Collection metadata exceeds the 64 MiB upgrade limit"));
        }
        let (_, after) = prepare_manifest_upgrade(&before)?;
        journal.manifests.push(UpgradeManifest {
            path: relative.clone(),
            before,
            after,
        });
    }
    if journal.manifests.iter().all(|m| m.before == m.after) {
        return Ok(());
    }
    if let Some(missing) = missing {
        return Err(fail(&format!("Referenced manifest is missing: {missing}")));
    }
    progress("Checking older collection metadata for OAA 1.0 upgrade");
    validate_journal(root, &journal)?;

    // Save and sync every original before any manifest replacement. The pending
    // pointer is installed atomically only after this complete backup exists.
    let mut backup = tempfile::Builder::new()
        .prefix(BACKUP_PREFIX)
        .suffix(".json")
        .tempfile_in(root)?;
    serde_json::to_writer(&mut backup, &journal)?;
    backup.flush()?;
    backup.as_file().sync_all()?;
    let (_, backup_path) = backup.keep().map_err(|e| AppError::Io(e.error))?;
    let backup_name = backup_path
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| fail("Invalid backup name"))?;
    write_new_json_manifest(&pending, &backup_name)?;
    progress("Upgrading collection manifests to OAA 1.0; original metadata backed up");
    install(root, &pending, &journal)
}

fn manifest_paths(collection: &Value) -> Result<BTreeSet<String>> {
    let collection: CollectionManifest = serde_json::from_value(collection.clone())?;
    let mut paths = BTreeSet::from([ROOT.into()]);
    for path in collection
        .galleries
        .into_iter()
        .map(|r| Some(r.path))
        .chain(collection.artworks.into_iter().map(|r| r.path))
    {
        let path = path.ok_or_else(|| fail("Artwork reference has no manifest path"))?;
        if !paths.insert(path) {
            return Err(fail("Duplicate manifest reference"));
        }
    }
    if paths.len() > OaaValidationLimits::default().max_entries {
        return Err(fail("Too many manifests to upgrade"));
    }
    Ok(paths)
}

fn validate_journal(root: &Path, journal: &UpgradeJournal) -> Result<()> {
    let mut documents = BTreeMap::new();
    let mut bytes = 0;
    for entry in &journal.manifests {
        bytes += entry.before.len();
        if bytes > MAX_METADATA_BYTES
            || entry.before.len() > OaaValidationLimits::default().max_manifest_size as usize
        {
            return Err(fail("Collection metadata exceeds upgrade limits"));
        }
        let (value, after) = prepare_manifest_upgrade(&entry.before)?;
        if after != entry.after
            || documents
                .insert(entry.path.clone(), value.clone())
                .is_some()
        {
            return Err(fail("Upgrade backup is inconsistent"));
        }
        // Ensure the current app can load every proposed document before writing.
        if entry.path == ROOT {
            serde_json::from_value::<CollectionManifest>(value)?;
        } else if entry.path.ends_with("/.oagallery") {
            serde_json::from_value::<GalleryManifest>(value)?;
        } else if entry.path.ends_with("/.oaartwork") {
            serde_json::from_value::<ArtworkManifest>(value)?;
        } else {
            return Err(fail("Reference does not name an OAA manifest"));
        }
    }
    let collection = documents
        .get(ROOT)
        .ok_or_else(|| fail("Missing collection in upgrade backup"))?;
    if manifest_paths(collection)? != documents.keys().cloned().collect() {
        return Err(fail(
            "Upgrade backup does not match the collection references",
        ));
    }
    validate_extraction_paths(root, &documents.keys().cloned().collect())?;
    let mut file_sizes = BTreeMap::new();
    for (path, document) in &documents {
        if let Some(files) = document.get("files").and_then(Value::as_array) {
            for file in files {
                if let Some(relative) = file.get("relative_path").and_then(Value::as_str) {
                    let parent = path.rsplit_once('/').map(|(p, _)| p).unwrap_or("");
                    let relative = format!("{parent}/{relative}");
                    validate_extraction_paths(root, &BTreeSet::from([relative.clone()]))?;
                    let target = root.join(&relative);
                    reject_filesystem_redirections(&target)?;
                    match fs::metadata(target) {
                        Ok(metadata) if metadata.is_file() => {
                            file_sizes.insert(relative, metadata.len());
                        }
                        Ok(_) => return Err(fail("Artwork attachment is not a regular file")),
                        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                        Err(error) => return Err(error.into()),
                    }
                }
            }
        }
    }
    ensure_upgrade_manifests_valid(&root.join(&journal.collection_file), &documents, file_sizes)?;
    for entry in &journal.manifests {
        check_current(&target_path(root, journal, entry), entry)?;
    }
    Ok(())
}

fn target_path(root: &Path, journal: &UpgradeJournal, entry: &UpgradeManifest) -> PathBuf {
    root.join(if entry.path == ROOT {
        &journal.collection_file
    } else {
        &entry.path
    })
}

fn check_current(path: &Path, entry: &UpgradeManifest) -> Result<String> {
    let current = read_text(
        path,
        OaaValidationLimits::default().max_manifest_size as usize,
    )?;
    if current != entry.before && current != entry.after {
        return Err(fail(&format!(
            "Manifest changed outside the upgrade; retained backup for recovery: {}",
            path.display()
        )));
    }
    Ok(current)
}

fn install(root: &Path, pending: &Path, journal: &UpgradeJournal) -> Result<()> {
    let result: Result<()> = (|| {
        // Root last: it must not advertise 1.0 before its children are installed.
        for entry in journal
            .manifests
            .iter()
            .filter(|m| m.path != ROOT)
            .chain(journal.manifests.iter().filter(|m| m.path == ROOT))
        {
            let path = target_path(root, journal, entry);
            if check_current(&path, entry)? != entry.after {
                staged_manifest_bytes(&path, entry.after.as_bytes())?.persist(true)?;
            }
        }
        Ok(())
    })();
    if let Err(error) = result {
        let rollback = (|| -> Result<()> {
            for entry in journal.manifests.iter().rev() {
                let path = target_path(root, journal, entry);
                if check_current(&path, entry)? != entry.before {
                    staged_manifest_bytes(&path, entry.before.as_bytes())?.persist(true)?;
                }
            }
            fs::remove_file(pending)?;
            Ok(())
        })();
        return Err(fail(&format!(
            "{error}. {}",
            if rollback.is_ok() {
                "Original manifests restored; backup retained"
            } else {
                "Recovery is pending; reopen this collection to retry. Backup retained"
            }
        )));
    }
    fs::remove_file(pending)?;
    Ok(())
}

fn read_text(path: &Path, limit: usize) -> Result<String> {
    reject_filesystem_redirections(path)?;
    if !fs::metadata(path)?.is_file() {
        return Err(fail("Manifest or recovery file is not a regular file"));
    }
    let mut text = String::new();
    fs::File::open(path)?
        .take(limit as u64 + 1)
        .read_to_string(&mut text)?;
    if text.len() > limit {
        return Err(fail("Manifest or recovery file exceeds upgrade limits"));
    }
    Ok(text)
}

fn fail(message: &str) -> AppError {
    AppError::Message(format!("OAA collection upgrade: {message}"))
}
