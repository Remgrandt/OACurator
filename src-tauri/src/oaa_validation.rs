mod zip_content;
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
use zip_content::ReadError;

pub const OAA_MEDIA_TYPE: &str = "application/vnd.original-art-archive+zip";

pub const OAA_SCHEMA_VERSION: &str = "1.0";
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
    pub valid: Option<bool>,
    pub status: String,
    pub complete: bool,
    pub schema_version: Option<String>,
    #[serde(skip)]
    limits: OaaValidationLimits,
    pub issues: Vec<OaaValidationIssue>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct OaaValidationLimits {
    pub max_archive_size: u64,
    pub max_uncompressed_size: u64,
    pub max_entries: usize,
    pub max_entry_size: u64,
    pub max_manifest_size: u64,
    pub max_json_depth: usize,
    pub max_directory_size: u64,
}

impl Default for OaaValidationLimits {
    fn default() -> Self {
        Self {
            max_archive_size: 2 * 1024 * 1024 * 1024,
            max_uncompressed_size: 4 * 1024 * 1024 * 1024,
            max_entries: 100_000,
            max_entry_size: 1024 * 1024 * 1024,
            max_manifest_size: 10 * 1024 * 1024,
            max_json_depth: 100,
            max_directory_size: 16 * 1024 * 1024,
        }
    }
}

impl OaaValidationReport {
    fn new(input: &Path) -> Self {
        Self {
            input: input.to_path_buf(),
            valid: Some(true),
            status: "valid".into(),
            complete: false,
            schema_version: None,
            limits: OaaValidationLimits::default(),
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
            self.valid = Some(false);
            self.status = "invalid".into();
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

    fn v1(&self) -> bool {
        self.schema_version.as_deref() == Some("1.0")
    }

    fn stopped(&mut self, status: &str, rule: &str, message: &str, path: &str) {
        if self.valid != Some(false) {
            self.valid = None;
            self.status = status.into();
        }
        self.complete = false;
        self.push(OaaValidationSeverity::Info, rule, message, path, None, None);
    }

    fn read_error(&mut self, error: ReadError, path: &str) {
        match error {
            ReadError::Capacity => self.stopped(
                "capacity_exceeded",
                "security.resource_limits",
                "Input exceeds configured processing limits.",
                path,
            ),
            ReadError::Unsupported => self.stopped(
                "unsupported",
                "manifests.schema_version_supported",
                "Archive encoding is unsupported.",
                path,
            ),
            ReadError::Io => self.stopped(
                "io_error",
                "package.archive_readable",
                "Input could not be completely read.",
                path,
            ),
            ReadError::Malformed => self.push(
                OaaValidationSeverity::Fatal,
                "package.archive_readable",
                "Archive content is malformed or fails integrity verification.",
                path,
                None,
                None,
            ),
        }
    }

    pub fn first_blocking_issue(&self) -> Option<&OaaValidationIssue> {
        self.issues
            .iter()
            .find(|issue| issue.severity.fails_import())
    }
}

struct ArchiveIndex {
    file_paths: BTreeSet<String>,
    file_sizes: BTreeMap<String, u64>,
}

pub fn ensure_oaa_archive_valid(path: &Path) -> Result<()> {
    let report = validate_oaa_archive_file(path)?;
    if report.valid != Some(true) || !report.complete {
        let message = report
            .first_blocking_issue()
            .or_else(|| report.issues.last())
            .map(|issue| issue.message.as_str())
            .unwrap_or("Validation did not complete");
        return Err(AppError::Message(format!(
            "OAA {}: {message}",
            report.status
        )));
    }
    Ok(())
}

pub fn validate_oaa_archive_file(path: &Path) -> Result<OaaValidationReport> {
    validate_oaa_archive_file_with_limits(path, OaaValidationLimits::default())
}

/// Preflight a local metadata upgrade. Files may have changed since their metadata
/// was saved; exact byte sizes are enforced when validating packaged archives.
pub(crate) fn ensure_upgrade_manifests_valid(
    collection_path: &Path,
    documents: &BTreeMap<String, Value>,
) -> Result<()> {
    let mut report = OaaValidationReport::new(collection_path);
    report.schema_version = Some(OAA_SCHEMA_VERSION.into());
    let mut index = ArchiveIndex {
        file_paths: documents.keys().cloned().collect(),
        file_sizes: BTreeMap::new(),
    };
    for (path, document) in documents {
        validate_schema_version(&mut report, document, path);
        validate_v1_fields(&mut report, document, path);
        if let Some(files) = document.get("files").and_then(Value::as_array) {
            let parent = path.rsplit_once('/').map(|(p, _)| p).unwrap_or("");
            for file in files {
                if let Some(relative) = file.get("relative_path").and_then(Value::as_str) {
                    index.file_paths.insert(format!("{parent}/{relative}"));
                }
            }
        }
    }
    let collection = documents.get(".oacollection").ok_or_else(|| {
        AppError::Message("OAA upgrade is missing the collection manifest".into())
    })?;
    validate_collection(
        &mut |_, path| documents.get(path).cloned(),
        &index,
        &mut report,
        collection,
        &mut BTreeMap::new(),
    );
    if let Some(issue) = report.first_blocking_issue().or_else(|| {
        (report.status != "valid")
            .then(|| report.issues.last())
            .flatten()
    }) {
        return Err(AppError::Message(format!(
            "Cannot upgrade OAA collection: {} ({}: {}). Original manifests were retained.",
            issue.message, issue.path, issue.rule_id
        )));
    }
    Ok(())
}

/// Change only the root version token. Re-serializing a Value would normalize
/// unknown numbers, whitespace and extension data belonging to other software.
pub(crate) fn prepare_manifest_upgrade(text: &str) -> Result<(Value, String)> {
    let json = text.trim_start_matches('\u{feff}');
    if !within_json_depth(json, OaaValidationLimits::default().max_json_depth) {
        return Err(AppError::Message(
            "OAA manifest nesting limit exceeded".into(),
        ));
    }
    if let Some(member) = duplicate_json_member(json) {
        return Err(AppError::Message(format!(
            "Duplicate OAA JSON member: {member}"
        )));
    }
    let mut value: Value = serde_json::from_str(json)?;
    match value.get("schema_version").and_then(Value::as_str) {
        Some("1.0") => return Ok((value, text.into())),
        Some("0.1") => {}
        _ => {
            return Err(AppError::Message(
                "Unsupported OAA manifest version; upgrade stopped".into(),
            ))
        }
    }
    let mut scanner = JsonDuplicateScanner::new(json);
    let replaced = (|| {
        scanner.skip_whitespace();
        scanner.expect('{')?;
        loop {
            scanner.skip_whitespace();
            let key = scanner.scan_string()?;
            scanner.skip_whitespace();
            scanner.expect(':')?;
            scanner.skip_whitespace();
            let start = scanner.index;
            scanner.scan_value()?;
            if key == "schema_version" {
                return Ok::<_, ()>(format!(
                    "{}{}\"1.0\"{}",
                    &text[..text.len() - json.len()],
                    scanner.chars[..start].iter().collect::<String>(),
                    scanner.chars[scanner.index..].iter().collect::<String>()
                ));
            }
            scanner.skip_whitespace();
            scanner.expect(',')?;
        }
    })()
    .map_err(|_| AppError::Message("Cannot locate OAA version token".into()))?;
    value["schema_version"] = Value::String(OAA_SCHEMA_VERSION.into());
    Ok((value, replaced))
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
    report.limits = limits;
    let duplicates = match zip_content::preflight(path, limits) {
        Ok(duplicates) => duplicates,
        Err(error) => {
            report.read_error(error, "archive");
            return Ok(report);
        }
    };
    let mut zip = match ZipArchive::new(fs::File::open(path)?) {
        Ok(zip) => zip,
        Err(_) => {
            report.read_error(ReadError::Malformed, "archive");
            return Ok(report);
        }
    };
    if duplicates.contains(".oacollection") {
        report.push(
            OaaValidationSeverity::Fatal,
            "package.duplicate_entries",
            "Ambiguous duplicate root manifest.",
            ".oacollection",
            None,
            None,
        );
        return Ok(report);
    }
    let Some(collection) = parse_manifest(&mut zip, &mut report, ".oacollection") else {
        if zip.index_for_name(".oacollection").is_none() {
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
    if report.status != "valid" {
        return Ok(report);
    }
    for path in duplicates {
        report.push(
            OaaValidationSeverity::Fatal,
            "package.duplicate_entries",
            "Duplicate archive entry.",
            path,
            None,
            None,
        );
    }
    let index = validate_zip_package(&mut zip, &mut report, limits)?;
    if report.status != "valid" {
        return Ok(report);
    }
    let mut total = 0;
    for index in 0..zip.len() {
        let remaining = limits
            .max_uncompressed_size
            .saturating_sub(total)
            .min(limits.max_entry_size);
        match zip_content::read_entry(&mut zip, path, index, remaining, false) {
            Ok((size, _)) => total += size,
            Err(error) => {
                report.read_error(error, "archive");
                return Ok(report);
            }
        }
    }
    validate_mimetype(&mut zip, &mut report);
    let mut manifests = BTreeMap::from([(".oacollection".into(), collection.clone())]);
    validate_collection(
        &mut |report, path| parse_manifest(&mut zip, report, path),
        &index,
        &mut report,
        &collection,
        &mut manifests,
    );
    validate_schema_versions_match(&mut report, &manifests);
    report.complete = matches!(report.status.as_str(), "valid" | "invalid");
    Ok(report)
}

fn validate_zip_package(
    zip: &mut ZipArchive<fs::File>,
    report: &mut OaaValidationReport,
    limits: OaaValidationLimits,
) -> Result<ArchiveIndex> {
    if zip.len() > limits.max_entries {
        report.stopped(
            "capacity_exceeded",
            "security.resource_limits",
            "Archive entry count exceeds the configured limit.",
            &report.input.display().to_string(),
        );
    }
    let mut ranges = Vec::new();
    let mut seen = BTreeSet::new();
    let mut file_sizes = BTreeMap::new();
    let mut file_paths = BTreeSet::new();
    let mut total_uncompressed_size = 0u64;
    for index in 0..zip.len() {
        let file = zip.by_index_raw(index)?;
        let path = file.name().to_string();
        ranges.push((
            file.header_start(),
            file.data_start().saturating_add(file.compressed_size()),
        ));
        total_uncompressed_size = total_uncompressed_size.saturating_add(file.size());
        if file.size() > limits.max_entry_size {
            report.stopped(
                "capacity_exceeded",
                "security.resource_limits",
                "Archive entry exceeds the configured individual file size limit.",
                &path.clone(),
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
                OaaValidationSeverity::Fatal,
                "package.compression_method",
                "Archive entry does not use Store or Deflate compression.",
                path.clone(),
                None,
                None,
            );
        }
        let kind = file.unix_mode().unwrap_or(0) & 0o170000;
        if !matches!(kind, 0 | 0o100000 | 0o040000)
            || (kind == 0o040000 && !file.is_dir())
            || (kind == 0o100000 && file.is_dir())
        {
            report.push(
                OaaValidationSeverity::Fatal,
                "package.entry_type",
                "Archive entry is not a regular file or directory.",
                &path,
                None,
                None,
            );
        }
        validate_archive_name_encoding(report, &file);
        validate_archive_name_normalization(report, &path);
        if file.is_dir() {
            validate_archive_directory_path(report, &path);
        } else {
            validate_archive_path(report, &path, "archive entry", None, None);
            file_sizes.insert(path.clone(), file.size());
            file_paths.insert(path);
        }
    }
    ranges.sort_unstable();
    if ranges.windows(2).any(|pair| pair[0].1 > pair[1].0) {
        report.push(
            OaaValidationSeverity::Fatal,
            "package.archive_readable",
            "ZIP entries overlap.",
            "archive",
            None,
            None,
        );
    }
    if total_uncompressed_size > limits.max_uncompressed_size {
        report.stopped(
            "capacity_exceeded",
            "security.resource_limits",
            "Archive uncompressed size exceeds the configured limit.",
            &report.input.display().to_string(),
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
    for name in &seen {
        let parts: Vec<_> = name.trim_end_matches('/').split('/').collect();
        if (name.ends_with('/') && file_paths.contains(name.trim_end_matches('/')))
            || (1..parts.len()).any(|n| file_paths.contains(&parts[..n].join("/")))
        {
            report.push(
                OaaValidationSeverity::Fatal,
                "package.path_conflict",
                "Archive file/directory conflict.",
                name,
                None,
                None,
            );
        }
    }
    Ok(ArchiveIndex {
        file_paths,
        file_sizes,
    })
}

fn validate_archive_name_encoding(report: &mut OaaValidationReport, file: &zip::read::ZipFile<'_>) {
    if std::str::from_utf8(file.name_raw()).is_err()
        || std::str::from_utf8(file.name_raw()).is_ok_and(|raw_name| {
            raw_name.bytes().any(|byte| byte > 127) && raw_name != file.name()
        })
    {
        report.push(
            OaaValidationSeverity::Fatal,
            "paths.utf8_names",
            "Non-ASCII archive entry name is not valid UTF-8.",
            file.name(),
            None,
            None,
        );
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
    validate_archive_path(
        report,
        path.strip_suffix('/').unwrap_or(path),
        "archive directory entry",
        None,
        None,
    );
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
    if path.starts_with('/')
        || (path.len() > 1
            && path.as_bytes()[0].is_ascii_alphabetic()
            && path.as_bytes()[1] == b':')
    {
        safe = false;
    }
    if path.contains('\\') {
        safe = false;
    }
    if path.split('/').any(|segment| {
        segment.is_empty() || segment == "." || segment == ".." || segment.contains('\0')
    }) {
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
    if let Err(error) = file
        .by_ref()
        .take(OAA_MEDIA_TYPE.len() as u64 + 1)
        .read_to_end(&mut data)
    {
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
    let index = zip.index_for_name(path)?;
    if let Ok(file) = zip.by_index_raw(index) {
        if file.encrypted() {
            report.push(
                OaaValidationSeverity::Fatal,
                "package.encrypted_entries",
                "Archive entry is encrypted.",
                path,
                None,
                None,
            );
            return None;
        }
        if !matches!(file.unix_mode().unwrap_or(0) & 0o170000, 0 | 0o100000) || file.is_dir() {
            report.push(
                OaaValidationSeverity::Fatal,
                "package.entry_type",
                "Manifest is not a regular file.",
                path,
                None,
                None,
            );
            return None;
        }
    }
    let limit = report
        .limits
        .max_manifest_size
        .min(report.limits.max_entry_size)
        .min(report.limits.max_uncompressed_size);
    let bytes = match zip_content::read_entry(zip, &report.input, index, limit, true) {
        Ok((_, bytes)) => bytes,
        Err(error) => {
            report.read_error(error, path);
            return None;
        }
    };
    let text = match String::from_utf8(bytes) {
        Ok(text) => text,
        Err(_) => {
            report.push(
                OaaValidationSeverity::Fatal,
                "manifests.json_object",
                "Manifest is not UTF-8 JSON.",
                path,
                Some(path),
                None,
            );
            return None;
        }
    };
    if !within_json_depth(&text, report.limits.max_json_depth) {
        report.stopped(
            "capacity_exceeded",
            "security.resource_limits",
            "Manifest exceeds configured JSON nesting limit.",
            path,
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
            if error.to_string().contains("number out of range") {
                report.stopped(
                    "capacity_exceeded",
                    "security.resource_limits",
                    "JSON number exceeds supported numeric range.",
                    path,
                );
                return None;
            }
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
    if path == ".oacollection" {
        report.schema_version = value
            .get("schema_version")
            .and_then(Value::as_str)
            .map(str::to_owned);
    }
    validate_schema_version(report, &value, path);
    if report.v1() {
        validate_v1_fields(report, &value, path);
    }
    scan_manifest_for_local_paths(report, &value, path, "");
    Some(value)
}

fn validate_schema_version(report: &mut OaaValidationReport, manifest: &Value, path: &str) {
    if path != ".oacollection"
        && report.v1()
        && manifest.get("schema_version").and_then(Value::as_str) != Some("1.0")
    {
        report.push(
            OaaValidationSeverity::Fatal,
            "manifests.schema_version_supported",
            "Referenced manifest must use schema_version 1.0.",
            path,
            Some(path),
            Some("/schema_version".into()),
        );
        return;
    }
    match manifest.get("schema_version") {
        None => report.push(
            OaaValidationSeverity::Fatal,
            "manifests.schema_version_required",
            "Manifest is missing `schema_version`.",
            path,
            Some(path),
            Some("/schema_version".to_string()),
        ),
        Some(Value::String(version)) if matches!(version.as_str(), "1.0" | "0.1") => {}
        Some(Value::String(version)) if !version.is_empty() => report.stopped(
            "unsupported",
            "manifests.schema_version_supported",
            "Manifest version is not supported.",
            path,
        ),
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
    read_manifest: &mut impl FnMut(&mut OaaValidationReport, &str) -> Option<Value>,
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

    if report.valid == Some(false) {
        return;
    }
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
        let Some(manifest) = read_manifest(report, path) else {
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

    if report.status == "capacity_exceeded" {
        return;
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
        let Some(manifest) = read_manifest(report, path) else {
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
    if !report.v1() && path != path.trim() {
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
    if let Some(status) = metadata
        .get("publication_status")
        .filter(|v| !report.v1() || !v.is_null())
    {
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
    if let Some(is_public) = metadata
        .get("is_public")
        .filter(|v| !report.v1() || !v.is_null())
    {
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
        if report.v1() {
            if let (Some(relative), Some(size)) = (
                file_entry.get("relative_path").and_then(Value::as_str),
                file_entry.get("size_bytes").filter(|v| !v.is_null()),
            ) {
                let resolved = if artwork_dir.is_empty() {
                    relative.into()
                } else {
                    format!("{artwork_dir}/{relative}")
                };
                if index
                    .file_sizes
                    .get(&resolved)
                    .is_some_and(|actual| size.as_f64() != Some(*actual as f64))
                {
                    report.push(
                        OaaValidationSeverity::Fatal,
                        "files.size_bytes",
                        "Declared file size does not match embedded bytes.",
                        artwork_manifest,
                        Some(artwork_manifest),
                        Some(format!("{pointer}/size_bytes")),
                    );
                }
            }
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
        if let Some(role) = file_entry
            .get("image_role")
            .filter(|v| !report.v1() || !v.is_null())
        {
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
    if links.is_null() && !report.v1() {
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
            Some(id) if !id.trim().is_empty() => {}
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
            Some(Value::String(url))
                if url.is_empty()
                    || (if report.v1() {
                        valid_absolute_uri(url)
                    } else {
                        Url::parse(url).is_ok()
                    }) => {}
            Some(Value::String(_)) => report.push(
                if report.v1() {
                    OaaValidationSeverity::Fatal
                } else {
                    OaaValidationSeverity::Warning
                },
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
        let check_shadow = !report.v1();
        for field in base_fields.iter().filter(|_| check_shadow) {
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
        if !report.v1() && block.contains_key("extensions") {
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
            if report.v1() {
                OaaValidationSeverity::Warning
            } else {
                OaaValidationSeverity::Fatal
            },
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
        let start = self.index;
        self.expect('"')?;
        while let Some(ch) = self.next() {
            if ch == '\\' {
                self.next().ok_or(())?;
            } else if ch == '"' {
                return serde_json::from_str(
                    &self.chars[start..self.index].iter().collect::<String>(),
                )
                .map_err(|_| ());
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

fn within_json_depth(text: &str, limit: usize) -> bool {
    let (mut depth, mut string, mut escaped) = (0usize, false, false);
    for byte in text.bytes() {
        if string {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                string = false;
            }
        } else {
            match byte {
                b'"' => string = true,
                b'{' | b'[' => {
                    depth += 1;
                    if depth > limit.min(100) {
                        return false;
                    }
                }
                b'}' | b']' => depth = depth.saturating_sub(1),
                _ => {}
            }
        }
    }
    true
}

fn valid_absolute_uri(value: &str) -> bool {
    if !value.is_ascii()
        || value
            .bytes()
            .any(|c| c <= 32 || c >= 127 || b"<>\"{}|\\^`".contains(&c))
    {
        return false;
    }
    let bytes = value.as_bytes();
    for (i, c) in bytes.iter().enumerate() {
        if *c == b'%'
            && (i + 2 >= bytes.len()
                || !bytes[i + 1].is_ascii_hexdigit()
                || !bytes[i + 2].is_ascii_hexdigit())
        {
            return false;
        }
    }
    let Some((scheme, rest)) = value.split_once(':') else {
        return false;
    };
    if scheme.is_empty()
        || !scheme.as_bytes()[0].is_ascii_alphabetic()
        || !scheme
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"+.-".contains(&c))
        || scheme.eq_ignore_ascii_case("file")
        || (scheme.len() == 1 && rest.starts_with('/'))
        || rest.is_empty()
    {
        return false;
    }
    let (body, fragment) = rest.split_once('#').unwrap_or((rest, ""));
    if fragment.contains('#') || fragment.contains(['[', ']']) {
        return false;
    }
    let (body, query) = body.split_once('?').unwrap_or((body, ""));
    if query.contains(['[', ']']) {
        return false;
    }
    if let Some(authority_path) = body.strip_prefix("//") {
        let (authority, path) = authority_path
            .split_once('/')
            .unwrap_or((authority_path, ""));
        if path.contains(['[', ']']) || authority.matches('@').count() > 1 {
            return false;
        }
        if authority
            .split_once('@')
            .is_some_and(|(user, _)| user.contains(['[', ']']))
        {
            return false;
        }
        let hostport = authority
            .rsplit_once('@')
            .map(|(_, host)| host)
            .unwrap_or(authority);
        if let Some(literal) = hostport.strip_prefix('[') {
            let Some((host, tail)) = literal.split_once(']') else {
                return false;
            };
            if !(host.parse::<std::net::Ipv6Addr>().is_ok()
                || host
                    .strip_prefix(['v', 'V'])
                    .and_then(|h| h.split_once('.'))
                    .is_some_and(|(version, address)| {
                        !version.is_empty()
                            && version.bytes().all(|b| b.is_ascii_hexdigit())
                            && !address.is_empty()
                            && address.bytes().all(|b| {
                                b.is_ascii_alphanumeric() || b"-._~!$&'()*+,;=:".contains(&b)
                            })
                    }))
                || !(tail.is_empty()
                    || tail
                        .strip_prefix(':')
                        .is_some_and(|port| port.bytes().all(|b| b.is_ascii_digit())))
            {
                return false;
            }
        } else {
            if hostport.contains(['[', ']']) {
                return false;
            }
            if let Some((host, port)) = hostport.split_once(':') {
                if host.contains(':') || !port.bytes().all(|b| b.is_ascii_digit()) {
                    return false;
                }
            }
        }
        if matches!(scheme.to_ascii_lowercase().as_str(), "http" | "https")
            && (hostport.is_empty() || hostport.starts_with(':'))
        {
            return false;
        }
    } else if body.contains(['[', ']'])
        || matches!(scheme.to_ascii_lowercase().as_str(), "http" | "https")
    {
        return false;
    }
    true
}

fn validate_v1_fields(report: &mut OaaValidationReport, manifest: &Value, path: &str) {
    fn field(
        report: &mut OaaValidationReport,
        object: &Value,
        name: &str,
        kind: &str,
        nullable: bool,
        path: &str,
        prefix: &str,
    ) {
        let Some(value) = object.get(name) else {
            return;
        };
        if nullable && value.is_null() {
            return;
        }
        let valid = match kind {
            "string" => value.is_string(),
            "boolean" => value.is_boolean(),
            "integer" => value
                .as_f64()
                .is_some_and(|v| v.is_finite() && v >= 0.0 && v.fract() == 0.0),
            "positive_integer" => value
                .as_f64()
                .is_some_and(|v| v.is_finite() && v > 0.0 && v.fract() == 0.0),
            "positive" => value.as_f64().is_some_and(|v| v.is_finite() && v > 0.0),
            _ => false,
        };
        if valid
            && matches!(kind, "integer" | "positive_integer")
            && value.as_i64().is_none()
            && value.as_f64().is_some_and(|n| n >= 9223372036854775808.0)
        {
            report.stopped(
                "capacity_exceeded",
                "security.resource_limits",
                "File metadata integer exceeds signed 64-bit capacity.",
                path,
            );
        }
        if !valid {
            report.push(
                OaaValidationSeverity::Fatal,
                "manifests.field_type",
                "Field has an invalid JSON type or value.",
                path,
                Some(path),
                Some(format!("{prefix}/{name}")),
            );
        }
    }
    if path == ".oacollection" {
        if let Some(refs) = manifest.get("galleries").and_then(Value::as_array) {
            for (i, r) in refs.iter().enumerate() {
                field(
                    report,
                    r,
                    "name",
                    "string",
                    false,
                    path,
                    &format!("/galleries/{i}"),
                );
            }
        }
        for (key, suffix) in [("galleries", "/.oagallery"), ("artworks", "/.oaartwork")] {
            if let Some(refs) = manifest.get(key).and_then(Value::as_array) {
                for r in refs {
                    if r.get("path")
                        .and_then(Value::as_str)
                        .is_some_and(|p| !p.ends_with(suffix) || p.len() == suffix.len())
                    {
                        report.push(
                            OaaValidationSeverity::Fatal,
                            "paths.manifest_path_safe",
                            "Reference does not locate the required manifest kind.",
                            path,
                            Some(path),
                            Some(format!("/{key}")),
                        );
                    }
                }
            }
        }
    }
    if let Some(public) = manifest.get("public_metadata") {
        for key in ["description", "for_sale_status", "media", "artwork_type"] {
            field(
                report,
                public,
                key,
                "string",
                true,
                path,
                "/public_metadata",
            );
        }
        if let Some(credits) = public.get("artist_credits").and_then(Value::as_array) {
            for (i, credit) in credits.iter().enumerate() {
                for key in ["display_name", "first_name", "last_name", "role"] {
                    field(
                        report,
                        credit,
                        key,
                        "string",
                        true,
                        path,
                        &format!("/public_metadata/artist_credits/{i}"),
                    );
                }
            }
        }
    }
    if let Some(private) = manifest.get("private_metadata") {
        for key in [
            "purchase_price",
            "estimated_value",
            "purchase_date",
            "provenance",
            "personal_notes",
        ] {
            field(
                report,
                private,
                key,
                "string",
                true,
                path,
                "/private_metadata",
            );
        }
        if let Some(date) = private.get("purchase_date").and_then(Value::as_str) {
            if date.starts_with("0000")
                || date.len() != 10
                || chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d")
                    .ok()
                    .is_none_or(|d| d.format("%Y-%m-%d").to_string() != date)
            {
                report.push(
                    OaaValidationSeverity::Fatal,
                    "manifests.field_type",
                    "Purchase date is not a real YYYY-MM-DD calendar date.",
                    path,
                    Some(path),
                    Some("/private_metadata/purchase_date".into()),
                );
            }
        }
    }
    if let Some(files) = manifest.get("files").and_then(Value::as_array) {
        for (i, file) in files.iter().enumerate() {
            let prefix = format!("/files/{i}");
            field(report, file, "file_name", "string", false, path, &prefix);
            for key in ["format", "media_type"] {
                field(report, file, key, "string", true, path, &prefix);
            }
            field(report, file, "is_primary", "boolean", true, path, &prefix);
            field(report, file, "size_bytes", "integer", true, path, &prefix);
            for key in ["width", "height"] {
                field(report, file, key, "positive_integer", true, path, &prefix);
            }
            for key in ["dpi_x", "dpi_y"] {
                field(report, file, key, "positive", true, path, &prefix);
            }
        }
    }
}
