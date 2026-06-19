// Copyright (c) 2026 Remgrandt Works. All rights reserved.

use crate::{AppError, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use unicode_normalization::UnicodeNormalization;
use url::Url;
use zip::{CompressionMethod, ZipArchive};

pub const OAA_MEDIA_TYPE: &str = "application/vnd.original-art-archive+zip";

const OAA_SCHEMA_VERSION: &str = "0.1";
const KNOWN_FILE_KINDS: &[&str] = &["raw", "derivative", "supporting"];
const KNOWN_IMAGE_ROLES: &[&str] = &[
    "raw_scan",
    "raw_photo",
    "corrected_scan",
    "detail",
    "verso",
    "reference",
];
const HIGH_RISK_EXTENSIONS: &[&str] = &[
    ".html", ".htm", ".svg", ".js", ".exe", ".bat", ".cmd", ".ps1", ".vbs", ".scr", ".msi",
];
const HIGH_RISK_MEDIA_TYPES: &[&str] = &[
    "text/html",
    "image/svg+xml",
    "application/javascript",
    "text/javascript",
];
const KNOWN_EXTERNAL_SITE_KEYS: &[&str] = &[
    "app.oa-curator",
    "com.comicartfans",
    "com.comicartfans.image",
    "com.comicartfans.thumbnail",
    "com.comicbookplus",
    "com.raremarq",
    "com.snikt",
    "gov.loc",
    "org.originalartarchive.examples",
];
const KNOWN_EXTENSION_BLOCKS: &[&str] = &[
    "app.oa-curator",
    "com.comicartfans",
    "com.comicartfans.image",
    "com.comicartfans.thumbnail",
    "com.comicbookplus",
    "com.raremarq",
    "com.snikt",
    "gov.loc",
    "org.originalartarchive.examples",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OaaValidationSeverity {
    Fatal,
    Error,
    Warning,
    Info,
}

impl OaaValidationSeverity {
    fn fails_import(self) -> bool {
        matches!(self, Self::Fatal | Self::Error)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OaaValidationIssue {
    pub rule_id: String,
    pub severity: OaaValidationSeverity,
    pub message: String,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manifest: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub json_pointer: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OaaValidationReport {
    pub input: PathBuf,
    pub valid: bool,
    pub issues: Vec<OaaValidationIssue>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct OaaValidationLimits {
    pub max_archive_size: u64,
    pub max_uncompressed_size: u64,
    pub max_entries: usize,
    pub max_entry_size: u64,
    pub max_manifest_size: u64,
}

impl Default for OaaValidationLimits {
    fn default() -> Self {
        Self {
            max_archive_size: 2 * 1024 * 1024 * 1024,
            max_uncompressed_size: 4 * 1024 * 1024 * 1024,
            max_entries: 100_000,
            max_entry_size: 1024 * 1024 * 1024,
            max_manifest_size: 10 * 1024 * 1024,
        }
    }
}

impl OaaValidationReport {
    fn new(input: &Path) -> Self {
        Self {
            input: input.to_path_buf(),
            valid: true,
            issues: Vec::new(),
        }
    }

    fn push(
        &mut self,
        severity: OaaValidationSeverity,
        rule_id: impl Into<String>,
        message: impl Into<String>,
        path: impl Into<String>,
        manifest: Option<&str>,
        json_pointer: Option<String>,
    ) {
        if severity.fails_import() {
            self.valid = false;
        }
        self.issues.push(OaaValidationIssue {
            rule_id: rule_id.into(),
            severity,
            message: message.into(),
            path: path.into(),
            manifest: manifest.map(str::to_string),
            json_pointer,
        });
    }

    pub fn first_blocking_issue(&self) -> Option<&OaaValidationIssue> {
        self.issues
            .iter()
            .find(|issue| issue.severity.fails_import())
    }
}

struct ArchiveIndex {
    file_paths: BTreeSet<String>,
}

pub fn ensure_oaa_archive_valid(path: &Path) -> Result<()> {
    let report = validate_oaa_archive_file(path)?;
    if let Some(issue) = report.first_blocking_issue() {
        return Err(AppError::Message(format!(
            "OAA archive validation failed: {}",
            issue.message
        )));
    }
    Ok(())
}

pub fn validate_oaa_archive_file(path: &Path) -> Result<OaaValidationReport> {
    validate_oaa_archive_file_with_limits(path, OaaValidationLimits::default())
}

pub fn validate_oaa_archive_file_with_limits(
    path: &Path,
    limits: OaaValidationLimits,
) -> Result<OaaValidationReport> {
    let mut report = OaaValidationReport::new(path);
    if path.extension().and_then(|extension| extension.to_str()) != Some("oaa") {
        report.push(
            OaaValidationSeverity::Warning,
            "package.extension_oaa",
            "Archive filesystem name does not use the `.oaa` extension.",
            path.display().to_string(),
            None,
            None,
        );
    }
    if path.exists() && path.is_file() && path.metadata()?.len() > limits.max_archive_size {
        report.push(
            OaaValidationSeverity::Fatal,
            "security.resource_limits",
            "Archive exceeds the configured archive size limit.",
            path.display().to_string(),
            None,
            None,
        );
    }

    let file = fs::File::open(path)?;
    let mut zip = match ZipArchive::new(file) {
        Ok(zip) => zip,
        Err(_) => {
            report.push(
                OaaValidationSeverity::Fatal,
                "package.archive_readable",
                "Input is not a readable ZIP-compatible OAA archive.",
                path.display().to_string(),
                None,
                None,
            );
            return Ok(report);
        }
    };

    let index = validate_zip_package(&mut zip, &mut report, limits)?;
    validate_mimetype(&mut zip, &mut report);
    let mut manifests = BTreeMap::new();
    let Some(collection) = parse_manifest(&mut zip, &mut report, ".oacollection") else {
        if !index.file_paths.contains(".oacollection") {
            report.push(
                OaaValidationSeverity::Fatal,
                "collection.manifest_present",
                "Archive is missing the root `.oacollection` manifest.",
                ".oacollection",
                None,
                None,
            );
        }
        return Ok(report);
    };
    manifests.insert(".oacollection".to_string(), collection.clone());
    validate_collection(&mut zip, &index, &mut report, &collection, &mut manifests);
    validate_schema_versions_match(&mut report, &manifests);
    Ok(report)
}

fn validate_zip_package(
    zip: &mut ZipArchive<fs::File>,
    report: &mut OaaValidationReport,
    limits: OaaValidationLimits,
) -> Result<ArchiveIndex> {
    if zip.len() > limits.max_entries {
        report.push(
            OaaValidationSeverity::Fatal,
            "security.resource_limits",
            "Archive entry count exceeds the configured limit.",
            report.input.display().to_string(),
            None,
            None,
        );
    }
    let mut seen = BTreeSet::new();
    let mut file_paths = BTreeSet::new();
    let mut total_uncompressed_size = 0u64;
    for index in 0..zip.len() {
        let file = zip.by_index_raw(index)?;
        let path = file.name().to_string();
        total_uncompressed_size = total_uncompressed_size.saturating_add(file.size());
        if file.size() > limits.max_entry_size {
            report.push(
                OaaValidationSeverity::Fatal,
                "security.resource_limits",
                "Archive entry exceeds the configured individual file size limit.",
                path.clone(),
                None,
                None,
            );
        }
        if !seen.insert(path.clone()) {
            report.push(
                OaaValidationSeverity::Fatal,
                "package.duplicate_entries",
                "Archive contains duplicate entries with the same path.",
                path.clone(),
                None,
                None,
            );
        }
        if file.encrypted() {
            report.push(
                OaaValidationSeverity::Fatal,
                "package.encrypted_entries",
                "Archive entry is encrypted.",
                path.clone(),
                None,
                None,
            );
        }
        if !matches!(
            file.compression(),
            CompressionMethod::Stored | CompressionMethod::Deflated
        ) {
            report.push(
                OaaValidationSeverity::Warning,
                "package.compression_method",
                "Archive entry does not use Store or Deflate compression.",
                path.clone(),
                None,
                None,
            );
        }
        if file.is_dir() {
            validate_archive_directory_path(report, &path);
        } else {
            validate_archive_path(report, &path, "archive entry", None, None);
            validate_archive_name_encoding(report, &file);
            validate_archive_name_normalization(report, &path);
            file_paths.insert(path);
        }
    }
    if total_uncompressed_size > limits.max_uncompressed_size {
        report.push(
            OaaValidationSeverity::Fatal,
            "security.resource_limits",
            "Archive uncompressed size exceeds the configured limit.",
            report.input.display().to_string(),
            None,
            None,
        );
    }

    if !zip.is_empty() {
        let first = zip.by_index_raw(0)?;
        if first.name() != "mimetype" {
            report.push(
                OaaValidationSeverity::Warning,
                "package.mimetype_first",
                "`mimetype` is not the first ZIP entry.",
                first.name().to_string(),
                None,
                None,
            );
        }
    }
    if let Some(mimetype_index) = zip.index_for_name("mimetype") {
        let mimetype = zip.by_index_raw(mimetype_index)?;
        if mimetype.compression() != CompressionMethod::Stored {
            report.push(
                OaaValidationSeverity::Warning,
                "package.mimetype_stored",
                "`mimetype` is not stored without compression.",
                "mimetype",
                None,
                None,
            );
        }
    }
    Ok(ArchiveIndex { file_paths })
}

fn validate_archive_name_encoding(report: &mut OaaValidationReport, file: &zip::read::ZipFile<'_>) {
    if let Ok(raw_name) = std::str::from_utf8(file.name_raw()) {
        if raw_name.bytes().any(|byte| byte > 127) && raw_name != file.name() {
            report.push(
                OaaValidationSeverity::Fatal,
                "paths.utf8_names",
                "Non-ASCII archive entry name is not marked as UTF-8.",
                file.name().to_string(),
                None,
                None,
            );
        }
    }
}

fn validate_archive_name_normalization(report: &mut OaaValidationReport, path: &str) {
    if path.nfc().collect::<String>() != path {
        report.push(
            OaaValidationSeverity::Warning,
            "paths.nfc",
            "Archive entry path is not Unicode NFC.",
            path,
            None,
            None,
        );
    }
}

fn validate_archive_directory_path(report: &mut OaaValidationReport, path: &str) {
    let trimmed = path.trim_end_matches('/');
    if trimmed.is_empty() {
        return;
    }
    validate_archive_path(report, trimmed, "archive directory entry", None, None);
}

fn validate_archive_path(
    report: &mut OaaValidationReport,
    path: &str,
    label: &str,
    manifest: Option<&str>,
    json_pointer: Option<String>,
) -> bool {
    validate_archive_path_with_rule(
        report,
        path,
        label,
        "paths.safe_archive_path",
        manifest,
        json_pointer,
    )
}

fn validate_archive_path_with_rule(
    report: &mut OaaValidationReport,
    path: &str,
    label: &str,
    rule_id: &str,
    manifest: Option<&str>,
    json_pointer: Option<String>,
) -> bool {
    let mut safe = true;
    if path.is_empty() {
        safe = false;
    }
    if path.starts_with('/') {
        safe = false;
    }
    if path.contains('\\') {
        safe = false;
    }
    if path
        .split('/')
        .any(|segment| segment.is_empty() || segment == "." || segment == "..")
    {
        safe = false;
    }
    if !safe {
        report.push(
            OaaValidationSeverity::Fatal,
            rule_id,
            format!("Unsafe {label}: {path}"),
            path,
            manifest,
            json_pointer,
        );
    }
    safe
}

fn validate_mimetype(zip: &mut ZipArchive<fs::File>, report: &mut OaaValidationReport) {
    let mut file = match zip.by_name("mimetype") {
        Ok(file) => file,
        Err(zip::result::ZipError::FileNotFound) => {
            report.push(
                OaaValidationSeverity::Fatal,
                "package.mimetype_present",
                "Archive is missing required root `mimetype` file.",
                "mimetype",
                None,
                None,
            );
            return;
        }
        Err(error) => {
            report.push(
                OaaValidationSeverity::Fatal,
                "package.mimetype_present",
                format!("Could not read OAA mimetype: {error}"),
                "mimetype",
                None,
                None,
            );
            return;
        }
    };
    let mut data = Vec::new();
    if let Err(error) = file.read_to_end(&mut data) {
        report.push(
            OaaValidationSeverity::Fatal,
            "package.mimetype_present",
            format!("Could not read OAA mimetype: {error}"),
            "mimetype",
            None,
            None,
        );
        return;
    }
    if data != OAA_MEDIA_TYPE.as_bytes() {
        report.push(
            OaaValidationSeverity::Fatal,
            "package.mimetype_value",
            "Root `mimetype` value is not exact.",
            "mimetype",
            None,
            None,
        );
    }
}

fn parse_manifest(
    zip: &mut ZipArchive<fs::File>,
    report: &mut OaaValidationReport,
    path: &str,
) -> Option<Value> {
    let mut file = match zip.by_name(path) {
        Ok(file) => file,
        Err(zip::result::ZipError::FileNotFound) => return None,
        Err(error) => {
            report.push(
                OaaValidationSeverity::Fatal,
                "manifests.json_object",
                format!("Manifest cannot be read as UTF-8 JSON: {error}"),
                path,
                Some(path),
                None,
            );
            return None;
        }
    };
    let mut text = String::new();
    if let Err(error) = file.read_to_string(&mut text) {
        report.push(
            OaaValidationSeverity::Fatal,
            "manifests.json_object",
            format!("Manifest cannot be read as UTF-8 JSON: {error}"),
            path,
            Some(path),
            None,
        );
        return None;
    }
    if text.starts_with('\u{feff}') {
        report.push(
            OaaValidationSeverity::Warning,
            "manifests.byte_order_mark",
            "Manifest starts with a byte order mark.",
            path,
            Some(path),
            None,
        );
    }
    let text = text.trim_start_matches('\u{feff}');
    if let Some(member) = duplicate_json_member(text) {
        report.push(
            OaaValidationSeverity::Fatal,
            "manifests.duplicate_json_members",
            format!("Manifest JSON object contains duplicate member name `{member}`."),
            path,
            Some(path),
            None,
        );
        return None;
    }
    let value = match serde_json::from_str::<Value>(text) {
        Ok(value) => value,
        Err(error) => {
            report.push(
                OaaValidationSeverity::Fatal,
                "manifests.json_object",
                format!("Manifest is not valid UTF-8 JSON: {error}"),
                path,
                Some(path),
                None,
            );
            return None;
        }
    };
    if !value.is_object() {
        report.push(
            OaaValidationSeverity::Fatal,
            "manifests.json_object",
            "Manifest top level is not a JSON object.",
            path,
            Some(path),
            None,
        );
        return None;
    }
    validate_schema_version(report, &value, path);
    scan_manifest_for_local_paths(report, &value, path, "");
    Some(value)
}

fn validate_schema_version(report: &mut OaaValidationReport, manifest: &Value, path: &str) {
    match manifest.get("schema_version") {
        None => report.push(
            OaaValidationSeverity::Fatal,
            "manifests.schema_version_required",
            "Manifest is missing `schema_version`.",
            path,
            Some(path),
            Some("/schema_version".to_string()),
        ),
        Some(Value::String(version)) if version == OAA_SCHEMA_VERSION => {}
        Some(_) => report.push(
            OaaValidationSeverity::Fatal,
            "manifests.schema_version_supported",
            "Manifest `schema_version` is not supported by this validator.",
            path,
            Some(path),
            Some("/schema_version".to_string()),
        ),
    }
}

fn validate_collection(
    zip: &mut ZipArchive<fs::File>,
    index: &ArchiveIndex,
    report: &mut OaaValidationReport,
    collection: &Value,
    manifests: &mut BTreeMap<String, Value>,
) {
    let Some(object) = collection.as_object() else {
        return;
    };
    validate_unknown_optional_fields(
        report,
        object,
        ".oacollection",
        &[
            "schema_version",
            "id",
            "name",
            "external_links",
            "galleries",
            "artworks",
            "extensions",
        ],
    );
    require_fields(
        report,
        object,
        ".oacollection",
        &["schema_version", "id", "name", "galleries", "artworks"],
        "collection.required_fields",
    );
    validate_required_string(report, collection, "id", ".oacollection", "/id");
    validate_required_string(report, collection, "name", ".oacollection", "/name");
    validate_external_links(
        report,
        collection.get("external_links"),
        ".oacollection",
        "/external_links",
    );
    validate_extensions(
        report,
        collection.get("extensions"),
        ".oacollection",
        "/extensions",
        object,
        &[
            "schema_version",
            "id",
            "name",
            "external_links",
            "galleries",
            "artworks",
        ],
    );

    let gallery_refs = validate_collection_refs(report, collection, "galleries", "gallery");
    let artwork_refs = validate_collection_refs(report, collection, "artworks", "artwork");
    let collection_artwork_ids = artwork_refs
        .iter()
        .filter_map(|reference| string_field(reference, "id"))
        .collect::<BTreeSet<_>>();

    for gallery_ref in gallery_refs {
        let Some(path) = string_field(gallery_ref, "path") else {
            continue;
        };
        if !index.file_paths.contains(path) {
            report.push(
                OaaValidationSeverity::Fatal,
                "collection.gallery_manifest_present",
                "Referenced gallery manifest is missing.",
                path,
                Some(".oacollection"),
                None,
            );
            continue;
        }
        let Some(manifest) = parse_manifest(zip, report, path) else {
            continue;
        };
        manifests.insert(path.to_string(), manifest.clone());
        validate_gallery(report, &manifest, path, &collection_artwork_ids);
        if manifest.get("id").and_then(Value::as_str) != string_field(gallery_ref, "id") {
            report.push(
                OaaValidationSeverity::Fatal,
                "collection.gallery_manifest_id_match",
                "Gallery manifest `id` does not match collection reference.",
                path,
                Some(path),
                Some("/id".to_string()),
            );
        }
    }

    for artwork_ref in artwork_refs {
        let Some(path) = string_field(artwork_ref, "path") else {
            continue;
        };
        if !index.file_paths.contains(path) {
            report.push(
                OaaValidationSeverity::Fatal,
                "collection.artwork_manifest_present",
                "Referenced artwork manifest is missing.",
                path,
                Some(".oacollection"),
                None,
            );
            continue;
        }
        let Some(manifest) = parse_manifest(zip, report, path) else {
            continue;
        };
        manifests.insert(path.to_string(), manifest.clone());
        validate_artwork(index, report, &manifest, path);
        if manifest.get("id").and_then(Value::as_str) != string_field(artwork_ref, "id") {
            report.push(
                OaaValidationSeverity::Fatal,
                "collection.artwork_manifest_id_match",
                "Artwork manifest `id` does not match collection reference.",
                path,
                Some(path),
                Some("/id".to_string()),
            );
        }
    }
}

fn require_fields(
    report: &mut OaaValidationReport,
    object: &Map<String, Value>,
    path: &str,
    fields: &[&str],
    rule_id: &str,
) {
    for field in fields {
        if !object.contains_key(*field) {
            report.push(
                OaaValidationSeverity::Fatal,
                rule_id,
                format!("Required field `{field}` is missing."),
                path,
                Some(path),
                Some(format!("/{field}")),
            );
        }
    }
}

fn validate_required_string(
    report: &mut OaaValidationReport,
    value: &Value,
    key: &str,
    manifest: &str,
    pointer: &str,
) {
    match value.get(key) {
        Some(Value::String(text)) if !text.trim().is_empty() => {}
        Some(Value::String(text)) if text.is_empty() => report.push(
            OaaValidationSeverity::Fatal,
            "manifests.required_string_not_empty",
            format!("Required string `{key}` is empty."),
            manifest,
            Some(manifest),
            Some(pointer.to_string()),
        ),
        Some(Value::String(_)) => report.push(
            OaaValidationSeverity::Fatal,
            "manifests.required_identifier_not_whitespace",
            format!("Required identifier `{key}` is whitespace-only."),
            manifest,
            Some(manifest),
            Some(pointer.to_string()),
        ),
        _ => report.push(
            OaaValidationSeverity::Fatal,
            "manifests.field_type",
            format!("Required field `{key}` is not a string."),
            manifest,
            Some(manifest),
            Some(pointer.to_string()),
        ),
    }
}

fn validate_collection_refs<'a>(
    report: &mut OaaValidationReport,
    collection: &'a Value,
    key: &str,
    ref_type: &str,
) -> Vec<&'a Value> {
    let object_rule = if ref_type == "gallery" {
        "collection.gallery_refs_objects"
    } else {
        "collection.artwork_refs_objects"
    };
    let unique_id_rule = if ref_type == "gallery" {
        "collection.unique_gallery_ids"
    } else {
        "collection.unique_artwork_ids"
    };
    let unique_path_rule = if ref_type == "gallery" {
        "collection.unique_gallery_paths"
    } else {
        "collection.unique_artwork_paths"
    };
    let Some(refs) = collection.get(key) else {
        return Vec::new();
    };
    let Some(refs) = refs.as_array() else {
        report.push(
            OaaValidationSeverity::Fatal,
            "collection.required_fields",
            format!("Collection `{key}` field is not an array."),
            ".oacollection",
            Some(".oacollection"),
            Some(format!("/{key}")),
        );
        return Vec::new();
    };

    let mut valid_refs = Vec::new();
    let mut ids = Vec::new();
    let mut paths = Vec::new();
    for (index, reference) in refs.iter().enumerate() {
        let pointer = format!("/{key}/{index}");
        let Some(object) = reference.as_object() else {
            report.push(
                OaaValidationSeverity::Fatal,
                object_rule,
                format!("Collection `{key}[]` entry is not an object."),
                ".oacollection",
                Some(".oacollection"),
                Some(pointer),
            );
            continue;
        };
        valid_refs.push(reference);
        validate_required_string(
            report,
            reference,
            "id",
            ".oacollection",
            &format!("/{key}/{index}/id"),
        );
        if let Some(id) = reference.get("id").and_then(Value::as_str) {
            ids.push(id.to_string());
        }
        match reference.get("path").and_then(Value::as_str) {
            Some(path) => {
                paths.push(path.to_string());
                validate_manifest_path(
                    report,
                    path,
                    ".oacollection",
                    format!("/{key}/{index}/path"),
                );
            }
            None => report.push(
                OaaValidationSeverity::Fatal,
                "paths.manifest_path_safe",
                "Manifest path is not a string.",
                ".oacollection",
                Some(".oacollection"),
                Some(format!("/{key}/{index}/path")),
            ),
        }
        validate_extensions(
            report,
            reference.get("extensions"),
            ".oacollection",
            &format!("/{key}/{index}/extensions"),
            object,
            &["id", "path"],
        );
    }
    add_duplicate_issues(
        report,
        &ids,
        unique_id_rule,
        ".oacollection",
        &format!("/{key}"),
    );
    add_duplicate_issues(
        report,
        &paths,
        unique_path_rule,
        ".oacollection",
        &format!("/{key}"),
    );
    valid_refs
}

fn validate_manifest_path(
    report: &mut OaaValidationReport,
    path: &str,
    manifest: &str,
    pointer: String,
) {
    validate_archive_path_with_rule(
        report,
        path,
        "manifest path",
        "paths.manifest_path_safe",
        Some(manifest),
        Some(pointer.clone()),
    );
    if path != path.trim() {
        report.push(
            OaaValidationSeverity::Fatal,
            "paths.manifest_path_safe",
            "Manifest path contains leading or trailing whitespace and is not trimmed before resolution.",
            path,
            Some(manifest),
            Some(pointer),
        );
    }
}

fn add_duplicate_issues(
    report: &mut OaaValidationReport,
    values: &[String],
    rule_id: &str,
    manifest: &str,
    pointer: &str,
) {
    let mut counts = BTreeMap::new();
    for value in values {
        *counts.entry(value).or_insert(0usize) += 1;
    }
    for (value, count) in counts {
        if count > 1 {
            report.push(
                OaaValidationSeverity::Fatal,
                rule_id,
                format!("Duplicate value `{value}`."),
                manifest,
                Some(manifest),
                Some(pointer.to_string()),
            );
        }
    }
}

fn validate_gallery(
    report: &mut OaaValidationReport,
    manifest: &Value,
    path: &str,
    collection_artwork_ids: &BTreeSet<&str>,
) {
    let Some(object) = manifest.as_object() else {
        return;
    };
    validate_unknown_optional_fields(
        report,
        object,
        path,
        &[
            "schema_version",
            "id",
            "name",
            "external_links",
            "artworks",
            "extensions",
        ],
    );
    require_fields(
        report,
        object,
        path,
        &["schema_version", "id", "name", "artworks"],
        "gallery.required_fields",
    );
    validate_required_string(report, manifest, "id", path, "/id");
    validate_required_string(report, manifest, "name", path, "/name");
    validate_external_links(
        report,
        manifest.get("external_links"),
        path,
        "/external_links",
    );
    validate_extensions(
        report,
        manifest.get("extensions"),
        path,
        "/extensions",
        object,
        &["schema_version", "id", "name", "external_links", "artworks"],
    );

    let Some(artworks) = manifest.get("artworks") else {
        return;
    };
    let Some(artworks) = artworks.as_array() else {
        report.push(
            OaaValidationSeverity::Fatal,
            "gallery.required_fields",
            "Gallery `artworks` field is not an array.",
            path,
            Some(path),
            Some("/artworks".to_string()),
        );
        return;
    };
    let mut ids = Vec::new();
    for (index, reference) in artworks.iter().enumerate() {
        let pointer = format!("/artworks/{index}");
        let Some(object) = reference.as_object() else {
            report.push(
                OaaValidationSeverity::Fatal,
                "gallery.artwork_refs_objects",
                "Gallery `artworks[]` entry is not an object.",
                path,
                Some(path),
                Some(pointer),
            );
            continue;
        };
        let mutable_fields = [
            "title",
            "external_links",
            "artist_credits",
            "media",
            "private_metadata",
            "public_metadata",
            "files",
        ];
        if mutable_fields
            .iter()
            .any(|field| object.contains_key(*field))
        {
            report.push(
                OaaValidationSeverity::Fatal,
                "gallery.no_mutable_artwork_metadata",
                "Gallery artwork reference duplicates mutable artwork metadata.",
                path,
                Some(path),
                Some(pointer.clone()),
            );
        }
        validate_required_string(report, reference, "id", path, &format!("{pointer}/id"));
        if let Some(id) = reference.get("id").and_then(Value::as_str) {
            ids.push(id.to_string());
            if !collection_artwork_ids.contains(id) {
                report.push(
                    OaaValidationSeverity::Fatal,
                    "gallery.artwork_refs_resolve",
                    "Gallery artwork reference does not resolve to a collection artwork ID.",
                    path,
                    Some(path),
                    Some(format!("{pointer}/id")),
                );
            }
        }
        validate_extensions(
            report,
            reference.get("extensions"),
            path,
            &format!("{pointer}/extensions"),
            object,
            &["id"],
        );
    }
    add_duplicate_issues(
        report,
        &ids,
        "gallery.unique_artwork_ids",
        path,
        "/artworks",
    );
}

fn validate_artwork(
    index: &ArchiveIndex,
    report: &mut OaaValidationReport,
    manifest: &Value,
    path: &str,
) {
    let Some(object) = manifest.as_object() else {
        return;
    };
    validate_unknown_optional_fields(
        report,
        object,
        path,
        &[
            "schema_version",
            "id",
            "title",
            "external_links",
            "public_metadata",
            "private_metadata",
            "files",
            "extensions",
        ],
    );
    require_fields(
        report,
        object,
        path,
        &["schema_version", "id", "title", "files"],
        "artwork.required_fields",
    );
    validate_required_string(report, manifest, "id", path, "/id");
    validate_required_string(report, manifest, "title", path, "/title");
    validate_external_links(
        report,
        manifest.get("external_links"),
        path,
        "/external_links",
    );
    validate_extensions(
        report,
        manifest.get("extensions"),
        path,
        "/extensions",
        object,
        &[
            "schema_version",
            "id",
            "title",
            "external_links",
            "public_metadata",
            "private_metadata",
            "files",
        ],
    );
    validate_public_metadata(report, manifest.get("public_metadata"), path);
    validate_private_metadata(report, manifest.get("private_metadata"), path);
    validate_files(index, report, manifest.get("files"), path);
}

fn validate_public_metadata(
    report: &mut OaaValidationReport,
    metadata: Option<&Value>,
    path: &str,
) {
    let Some(metadata) = metadata else {
        return;
    };
    let Some(object) = metadata.as_object() else {
        report.push(
            OaaValidationSeverity::Fatal,
            "artwork.public_metadata_object",
            "Artwork `public_metadata` is not an object.",
            path,
            Some(path),
            Some("/public_metadata".to_string()),
        );
        return;
    };
    if let Some(status) = metadata.get("publication_status") {
        if !matches!(
            status.as_str(),
            Some("published_art") | Some("unpublished_art")
        ) {
            report.push(
                OaaValidationSeverity::Fatal,
                "artwork.publication_status",
                "Base `publication_status` is not an allowed OAA value.",
                path,
                Some(path),
                Some("/public_metadata/publication_status".to_string()),
            );
        }
    }
    if let Some(is_public) = metadata.get("is_public") {
        if !is_public.is_boolean() {
            report.push(
                OaaValidationSeverity::Fatal,
                "manifests.field_type",
                "Manifest does not match the OAA JSON Schema at `/public_metadata/is_public`: value is not a boolean.",
                path,
                Some(path),
                Some("/public_metadata/is_public".to_string()),
            );
        }
    }
    if let Some(credits) = metadata.get("artist_credits") {
        let Some(credits) = credits.as_array() else {
            report.push(
                OaaValidationSeverity::Fatal,
                "artwork.artist_credit_objects",
                "`artist_credits` is not an array.",
                path,
                Some(path),
                Some("/public_metadata/artist_credits".to_string()),
            );
            return;
        };
        for (index, credit) in credits.iter().enumerate() {
            let pointer = format!("/public_metadata/artist_credits/{index}");
            let Some(credit_object) = credit.as_object() else {
                report.push(
                    OaaValidationSeverity::Fatal,
                    "artwork.artist_credit_objects",
                    "`artist_credits[]` entry is not an object.",
                    path,
                    Some(path),
                    Some(pointer),
                );
                continue;
            };
            if !["display_name", "first_name", "last_name", "role"]
                .iter()
                .any(|field| {
                    credit
                        .get(*field)
                        .and_then(Value::as_str)
                        .is_some_and(|value| !value.is_empty())
                })
            {
                report.push(
                    OaaValidationSeverity::Fatal,
                    "artwork.artist_credit_objects",
                    "`artist_credits[]` entry contains no artist name or role fields.",
                    path,
                    Some(path),
                    Some(pointer.clone()),
                );
            }
            validate_extensions(
                report,
                credit.get("extensions"),
                path,
                &format!("{pointer}/extensions"),
                credit_object,
                &["display_name", "first_name", "last_name", "role"],
            );
        }
    }
    validate_extensions(
        report,
        metadata.get("extensions"),
        path,
        "/public_metadata/extensions",
        object,
        &[
            "description",
            "for_sale_status",
            "media",
            "artwork_type",
            "publication_status",
            "is_public",
            "artist_credits",
        ],
    );
}

fn validate_private_metadata(
    report: &mut OaaValidationReport,
    metadata: Option<&Value>,
    path: &str,
) {
    let Some(metadata) = metadata else {
        return;
    };
    let Some(object) = metadata.as_object() else {
        report.push(
            OaaValidationSeverity::Fatal,
            "artwork.private_metadata_object",
            "Artwork `private_metadata` is not an object.",
            path,
            Some(path),
            Some("/private_metadata".to_string()),
        );
        return;
    };
    validate_extensions(
        report,
        metadata.get("extensions"),
        path,
        "/private_metadata/extensions",
        object,
        &[
            "purchase_price",
            "estimated_value",
            "purchase_date",
            "provenance",
            "personal_notes",
        ],
    );
}

fn validate_unknown_optional_fields(
    report: &mut OaaValidationReport,
    object: &Map<String, Value>,
    manifest: &str,
    allowed: &[&str],
) {
    for key in object.keys() {
        if !allowed.contains(&key.as_str()) {
            report.push(
                OaaValidationSeverity::Info,
                "manifests.unknown_optional_fields",
                format!("Unknown optional field `{key}` is ignored for OAA interpretation."),
                manifest,
                Some(manifest),
                Some(format!("/{}", escape_json_pointer(key))),
            );
        }
    }
}

fn validate_files(
    index: &ArchiveIndex,
    report: &mut OaaValidationReport,
    files: Option<&Value>,
    artwork_manifest: &str,
) {
    let Some(files) = files else {
        return;
    };
    let Some(files) = files.as_array() else {
        report.push(
            OaaValidationSeverity::Fatal,
            "artwork.required_fields",
            "Artwork `files` field is not an array.",
            artwork_manifest,
            Some(artwork_manifest),
            Some("/files".to_string()),
        );
        return;
    };
    let artwork_dir = artwork_manifest
        .rsplit_once('/')
        .map(|(dir, _)| dir)
        .unwrap_or("");
    let mut ids = Vec::new();
    let mut primary_count = 0usize;
    for (file_index, file_entry) in files.iter().enumerate() {
        let pointer = format!("/files/{file_index}");
        let Some(object) = file_entry.as_object() else {
            report.push(
                OaaValidationSeverity::Fatal,
                "files.entries_objects",
                "`files[]` entry is not an object.",
                artwork_manifest,
                Some(artwork_manifest),
                Some(pointer),
            );
            continue;
        };
        validate_required_string(
            report,
            file_entry,
            "id",
            artwork_manifest,
            &format!("{pointer}/id"),
        );
        if let Some(id) = file_entry.get("id").and_then(Value::as_str) {
            ids.push(id.to_string());
        }
        if file_entry.get("is_primary") == Some(&Value::Bool(true)) {
            primary_count += 1;
        }
        match file_entry.get("relative_path").and_then(Value::as_str) {
            Some(relative_path) => {
                if validate_archive_path_with_rule(
                    report,
                    relative_path,
                    "artwork file relative path",
                    "files.relative_path_safe",
                    Some(artwork_manifest),
                    Some(format!("{pointer}/relative_path")),
                ) {
                    let resolved = if artwork_dir.is_empty() {
                        relative_path.to_string()
                    } else {
                        format!("{artwork_dir}/{relative_path}")
                    };
                    if validate_archive_path_with_rule(
                        report,
                        &resolved,
                        "resolved artwork file path",
                        "files.relative_path_safe",
                        Some(artwork_manifest),
                        Some(format!("{pointer}/relative_path")),
                    ) && !index.file_paths.contains(&resolved)
                    {
                        report.push(
                            OaaValidationSeverity::Error,
                            "files.relative_path_exists",
                            format!("Missing referenced OAA archive entry: {resolved}"),
                            resolved,
                            Some(artwork_manifest),
                            Some(format!("{pointer}/relative_path")),
                        );
                    }
                }
            }
            None => report.push(
                OaaValidationSeverity::Fatal,
                "files.relative_path_safe",
                "Manifest path is not a string.",
                artwork_manifest,
                Some(artwork_manifest),
                Some(format!("{pointer}/relative_path")),
            ),
        }
        match file_entry.get("file_kind").and_then(Value::as_str) {
            Some(kind) if KNOWN_FILE_KINDS.contains(&kind) => {}
            _ => report.push(
                OaaValidationSeverity::Fatal,
                "files.file_kind",
                "Base `file_kind` is not an allowed OAA value.",
                artwork_manifest,
                Some(artwork_manifest),
                Some(format!("{pointer}/file_kind")),
            ),
        }
        if let Some(role) = file_entry.get("image_role") {
            if !matches!(role.as_str(), Some(role) if KNOWN_IMAGE_ROLES.contains(&role)) {
                report.push(
                    OaaValidationSeverity::Fatal,
                    "files.image_role",
                    "Base `image_role` is not an allowed OAA value.",
                    artwork_manifest,
                    Some(artwork_manifest),
                    Some(format!("{pointer}/image_role")),
                );
            }
        }
        validate_high_risk_media(report, file_entry, artwork_manifest, &pointer);
        validate_external_links(
            report,
            file_entry.get("external_links"),
            artwork_manifest,
            &format!("{pointer}/external_links"),
        );
        validate_extensions(
            report,
            file_entry.get("extensions"),
            artwork_manifest,
            &format!("{pointer}/extensions"),
            object,
            &[
                "id",
                "file_name",
                "relative_path",
                "file_kind",
                "size_bytes",
                "width",
                "height",
                "format",
                "media_type",
                "is_primary",
                "image_role",
                "external_links",
            ],
        );
    }
    add_duplicate_issues(
        report,
        &ids,
        "files.unique_file_ids",
        artwork_manifest,
        "/files",
    );
    if primary_count > 1 {
        report.push(
            OaaValidationSeverity::Warning,
            "files.multiple_primary",
            "More than one file entry has `is_primary: true`.",
            artwork_manifest,
            Some(artwork_manifest),
            Some("/files".to_string()),
        );
    }
}

fn validate_high_risk_media(
    report: &mut OaaValidationReport,
    file_entry: &Value,
    manifest: &str,
    pointer: &str,
) {
    let name = file_entry
        .get("relative_path")
        .or_else(|| file_entry.get("file_name"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let lower_name = name.to_ascii_lowercase();
    let media_type = file_entry
        .get("media_type")
        .and_then(Value::as_str)
        .unwrap_or("");
    if HIGH_RISK_EXTENSIONS
        .iter()
        .any(|extension| lower_name.ends_with(extension))
        || HIGH_RISK_MEDIA_TYPES.contains(&media_type)
    {
        report.push(
            OaaValidationSeverity::Warning,
            "security.high_risk_media",
            "Embedded file type may have active or high-risk behavior.",
            manifest,
            Some(manifest),
            Some(pointer.to_string()),
        );
    }
}

fn validate_external_links(
    report: &mut OaaValidationReport,
    links: Option<&Value>,
    manifest: &str,
    pointer: &str,
) {
    let Some(links) = links else {
        return;
    };
    if links.is_null() {
        return;
    }
    let Some(links) = links.as_array() else {
        report.push(
            OaaValidationSeverity::Fatal,
            "external_links.object",
            "`external_links` is not an array of objects.",
            manifest,
            Some(manifest),
            Some(pointer.to_string()),
        );
        return;
    };
    for (index, link) in links.iter().enumerate() {
        let item_pointer = format!("{pointer}/{index}");
        let Some(object) = link.as_object() else {
            report.push(
                OaaValidationSeverity::Fatal,
                "external_links.object",
                "`external_links[]` entry is not an object.",
                manifest,
                Some(manifest),
                Some(item_pointer),
            );
            continue;
        };
        match link.get("provider").and_then(Value::as_str) {
            Some(provider) if valid_external_site_key(provider) => {
                if !KNOWN_EXTERNAL_SITE_KEYS.contains(&provider) {
                    report.push(
                        OaaValidationSeverity::Info,
                        "external_links.unknown_provider",
                        "External link site key is unknown and will be treated generically.",
                        manifest,
                        Some(manifest),
                        Some(format!("{item_pointer}/provider")),
                    );
                }
            }
            _ => report.push(
                OaaValidationSeverity::Fatal,
                "external_links.provider",
                "External link site identifier violates the OAA external-link key grammar.",
                manifest,
                Some(manifest),
                Some(format!("{item_pointer}/provider")),
            ),
        }
        match link.get("id").and_then(Value::as_str) {
            Some(id) if !id.is_empty() => {}
            _ => report.push(
                OaaValidationSeverity::Fatal,
                "external_links.id",
                "External link `id` is missing or empty.",
                manifest,
                Some(manifest),
                Some(format!("{item_pointer}/id")),
            ),
        }
        match link.get("url") {
            Some(Value::String(url)) if url.is_empty() || Url::parse(url).is_ok() => {}
            Some(Value::String(_)) => report.push(
                OaaValidationSeverity::Warning,
                "external_links.url",
                "External link `url` is non-empty but not absolute.",
                manifest,
                Some(manifest),
                Some(format!("{item_pointer}/url")),
            ),
            _ => report.push(
                OaaValidationSeverity::Fatal,
                "external_links.url",
                "External link `url` is not a string.",
                manifest,
                Some(manifest),
                Some(format!("{item_pointer}/url")),
            ),
        }
        validate_extensions(
            report,
            link.get("extensions"),
            manifest,
            &format!("{item_pointer}/extensions"),
            object,
            &["provider", "id", "url"],
        );
    }
}

fn valid_external_site_key(value: &str) -> bool {
    !value.is_empty()
        && !value.starts_with('.')
        && !value.ends_with('.')
        && !value.contains("..")
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'_' | b'-')
        })
}

fn validate_extensions(
    report: &mut OaaValidationReport,
    extensions: Option<&Value>,
    manifest: &str,
    pointer: &str,
    parent: &Map<String, Value>,
    base_fields: &[&str],
) {
    let Some(extensions) = extensions else {
        return;
    };
    let Some(blocks) = extensions.as_object() else {
        report.push(
            OaaValidationSeverity::Fatal,
            "extensions.container_object",
            "`extensions` value is not an object.",
            manifest,
            Some(manifest),
            Some(pointer.to_string()),
        );
        return;
    };
    for (name, block) in blocks {
        let block_pointer = format!("{pointer}/{}", escape_json_pointer(name));
        if !KNOWN_EXTENSION_BLOCKS.contains(&name.as_str()) {
            report.push(
                OaaValidationSeverity::Info,
                "extensions.unknown_block",
                "Extension block is unknown and will be ignored for OAA interpretation.",
                manifest,
                Some(manifest),
                Some(block_pointer.clone()),
            );
        }
        let Some(block) = block.as_object() else {
            report.push(
                OaaValidationSeverity::Fatal,
                "extensions.block_object",
                "Extension block value is not an object.",
                manifest,
                Some(manifest),
                Some(block_pointer),
            );
            continue;
        };
        for field in base_fields {
            if parent.contains_key(*field) && block.contains_key(*field) {
                report.push(
                    OaaValidationSeverity::Fatal,
                    "extensions.no_base_field_shadow",
                    format!("Extension block field `{field}` shadows a present OAA base field."),
                    manifest,
                    Some(manifest),
                    Some(format!("{block_pointer}/{}", escape_json_pointer(field))),
                );
            }
        }
        if block.contains_key("extensions") {
            report.push(
                OaaValidationSeverity::Fatal,
                "extensions.no_nested_extensions",
                "Extension block contains a nested `extensions` container.",
                manifest,
                Some(manifest),
                Some(block_pointer),
            );
        }
    }
}

fn scan_manifest_for_local_paths(
    report: &mut OaaValidationReport,
    value: &Value,
    manifest: &str,
    pointer: &str,
) {
    match value {
        Value::Object(object) => {
            for (key, item) in object {
                let pointer = if pointer.is_empty() {
                    format!("/{}", escape_json_pointer(key))
                } else {
                    format!("{pointer}/{}", escape_json_pointer(key))
                };
                scan_manifest_for_local_paths(report, item, manifest, &pointer);
            }
        }
        Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                scan_manifest_for_local_paths(
                    report,
                    item,
                    manifest,
                    &format!("{pointer}/{index}"),
                );
            }
        }
        Value::String(value) if is_apparent_local_path(value) => report.push(
            OaaValidationSeverity::Fatal,
            "security.local_path_in_manifest",
            "Manifest value contains an apparent absolute local filesystem path.",
            manifest,
            Some(manifest),
            Some(pointer.to_string()),
        ),
        _ => {}
    }
}

fn is_apparent_local_path(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    lower.starts_with("file://")
        || value.starts_with("\\\\")
        || [
            "/Users", "/home", "/var", "/tmp", "/Volumes", "/mnt", "/opt", "/etc",
        ]
        .iter()
        .any(|prefix| value == *prefix || value.starts_with(&format!("{prefix}/")))
        || (value.len() >= 3
            && value.as_bytes()[1] == b':'
            && matches!(value.as_bytes()[2], b'\\' | b'/'))
}

fn validate_schema_versions_match(
    report: &mut OaaValidationReport,
    manifests: &BTreeMap<String, Value>,
) {
    let versions = manifests
        .iter()
        .filter_map(|(path, manifest)| {
            manifest
                .get("schema_version")
                .and_then(Value::as_str)
                .map(|version| (path, version))
        })
        .collect::<Vec<_>>();
    let unique_versions = versions
        .iter()
        .map(|(_, version)| *version)
        .collect::<BTreeSet<_>>();
    if unique_versions.len() > 1 {
        for (path, _) in versions {
            report.push(
                OaaValidationSeverity::Warning,
                "manifests.same_schema_versions",
                "Manifest schema versions in this archive do not all match.",
                path,
                Some(path),
                None,
            );
        }
    }
}

fn string_field<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str)
}

fn duplicate_json_member(text: &str) -> Option<String> {
    JsonDuplicateScanner::new(text).scan_value().ok().flatten()
}

struct JsonDuplicateScanner {
    chars: Vec<char>,
    index: usize,
}

impl JsonDuplicateScanner {
    fn new(text: &str) -> Self {
        Self {
            chars: text.chars().collect(),
            index: 0,
        }
    }

    fn scan_value(&mut self) -> std::result::Result<Option<String>, ()> {
        self.skip_whitespace();
        match self.peek() {
            Some('{') => self.scan_object(),
            Some('[') => self.scan_array(),
            Some('"') => self.scan_string().map(|_| None),
            Some(_) => {
                self.skip_primitive();
                Ok(None)
            }
            None => Ok(None),
        }
    }

    fn scan_object(&mut self) -> std::result::Result<Option<String>, ()> {
        self.expect('{')?;
        self.skip_whitespace();
        let mut keys = BTreeSet::new();
        if self.consume('}') {
            return Ok(None);
        }
        loop {
            self.skip_whitespace();
            let key = self.scan_string()?;
            if !keys.insert(key.clone()) {
                return Ok(Some(key));
            }
            self.skip_whitespace();
            self.expect(':')?;
            if let Some(duplicate) = self.scan_value()? {
                return Ok(Some(duplicate));
            }
            self.skip_whitespace();
            if self.consume('}') {
                return Ok(None);
            }
            self.expect(',')?;
        }
    }

    fn scan_array(&mut self) -> std::result::Result<Option<String>, ()> {
        self.expect('[')?;
        self.skip_whitespace();
        if self.consume(']') {
            return Ok(None);
        }
        loop {
            if let Some(duplicate) = self.scan_value()? {
                return Ok(Some(duplicate));
            }
            self.skip_whitespace();
            if self.consume(']') {
                return Ok(None);
            }
            self.expect(',')?;
        }
    }

    fn scan_string(&mut self) -> std::result::Result<String, ()> {
        self.expect('"')?;
        let mut output = String::new();
        while let Some(ch) = self.next() {
            match ch {
                '"' => return Ok(output),
                '\\' => match self.next().ok_or(())? {
                    '"' => output.push('"'),
                    '\\' => output.push('\\'),
                    '/' => output.push('/'),
                    'b' => output.push('\u{0008}'),
                    'f' => output.push('\u{000c}'),
                    'n' => output.push('\n'),
                    'r' => output.push('\r'),
                    't' => output.push('\t'),
                    'u' => {
                        let mut value = 0u32;
                        for _ in 0..4 {
                            value =
                                value * 16 + self.next().and_then(|ch| ch.to_digit(16)).ok_or(())?;
                        }
                        if let Some(ch) = char::from_u32(value) {
                            output.push(ch);
                        }
                    }
                    _ => return Err(()),
                },
                _ => output.push(ch),
            }
        }
        Err(())
    }

    fn skip_primitive(&mut self) {
        while let Some(ch) = self.peek() {
            if matches!(ch, ',' | ']' | '}') {
                break;
            }
            self.index += 1;
        }
    }

    fn skip_whitespace(&mut self) {
        while self.peek().is_some_and(char::is_whitespace) {
            self.index += 1;
        }
    }

    fn expect(&mut self, expected: char) -> std::result::Result<(), ()> {
        if self.consume(expected) {
            Ok(())
        } else {
            Err(())
        }
    }

    fn consume(&mut self, expected: char) -> bool {
        if self.peek() == Some(expected) {
            self.index += 1;
            true
        } else {
            false
        }
    }

    fn next(&mut self) -> Option<char> {
        let ch = self.peek()?;
        self.index += 1;
        Some(ch)
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.index).copied()
    }
}

fn escape_json_pointer(value: &str) -> String {
    value.replace('~', "~0").replace('/', "~1")
}
