use crate::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::Path;
use tempfile::NamedTempFile;

pub const SCHEMA_VERSION: &str = "0.1";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestReference {
    pub id: String,
    pub name: String,
    pub path: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extensions: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtworkManifestReference {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extensions: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalLinkManifest {
    pub provider: String,
    pub id: String,
    pub url: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extensions: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionManifest {
    pub schema_version: String,
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub external_links: Vec<ExternalLinkManifest>,
    pub galleries: Vec<ManifestReference>,
    pub artworks: Vec<ArtworkManifestReference>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extensions: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GalleryManifest {
    pub schema_version: String,
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub external_links: Vec<ExternalLinkManifest>,
    pub artworks: Vec<ArtworkManifestReference>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extensions: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtworkManifest {
    pub schema_version: String,
    pub id: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub external_links: Vec<ExternalLinkManifest>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public_metadata: Option<ArtworkPublicMetadata>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub private_metadata: Option<ArtworkPrivateMetadata>,
    pub files: Vec<ArtworkFileManifest>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extensions: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtworkPublicMetadata {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub for_sale_status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artwork_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub publication_status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_public: Option<bool>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artist_credits: Vec<ArtworkArtistCredit>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extensions: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtworkPrivateMetadata {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub purchase_price: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub estimated_value: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub purchase_date: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provenance: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub personal_notes: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extensions: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtworkArtistCredit {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extensions: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtworkFileManifest {
    pub id: String,
    pub relative_path: String,
    pub file_kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dpi_x: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dpi_y: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_primary: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_role: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub external_links: Vec<ExternalLinkManifest>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extensions: BTreeMap<String, Value>,
}

pub fn read_json_manifest<T>(path: &Path) -> Result<T>
where
    T: for<'de> Deserialize<'de>,
{
    let contents = fs::read_to_string(path)?;
    Ok(serde_json::from_str(&contents)?)
}

pub fn write_json_manifest<T>(path: &Path, manifest: &T) -> Result<()>
where
    T: Serialize,
{
    staged_json_manifest(path, manifest)?.persist(true)
}

pub fn write_new_json_manifest<T>(path: &Path, manifest: &T) -> Result<()>
where
    T: Serialize,
{
    staged_json_manifest(path, manifest)?.persist(false)
}

pub struct StagedJsonManifest {
    target: std::path::PathBuf,
    temporary: NamedTempFile,
}

impl StagedJsonManifest {
    pub fn persist(self, overwrite: bool) -> Result<()> {
        let result = if overwrite {
            self.temporary.persist(&self.target)
        } else {
            self.temporary.persist_noclobber(&self.target)
        };
        result.map(|_| ()).map_err(|error| {
            crate::AppError::Message(format!(
                "Could not install completed manifest {}: {}",
                self.target.display(),
                error.error
            ))
        })
    }
}

pub fn staged_json_manifest<T>(path: &Path, manifest: &T) -> Result<StagedJsonManifest>
where
    T: Serialize,
{
    let contents = format!("{}\n", serde_json::to_string_pretty(manifest)?);
    let parent = path.parent().ok_or_else(|| {
        crate::AppError::Message(format!("Manifest path has no parent: {}", path.display()))
    })?;
    fs::create_dir_all(parent)?;
    let mut temporary = NamedTempFile::new_in(parent)?;
    temporary.write_all(contents.as_bytes())?;
    temporary.as_file_mut().flush()?;
    temporary.as_file().sync_all()?;
    Ok(StagedJsonManifest {
        target: path.to_path_buf(),
        temporary,
    })
}

#[cfg(test)]
mod tests {
    use super::{read_json_manifest, write_json_manifest, write_new_json_manifest};
    use serde_json::{json, Value};
    use tempfile::tempdir;

    #[test]
    fn completed_updates_replace_existing_json() {
        let temp = tempdir().expect("tempdir");
        let path = temp.path().join(".oaartwork");
        write_json_manifest(&path, &json!({"version": 1})).expect("first write");
        write_json_manifest(&path, &json!({"version": 2})).expect("replacement");
        let value: Value = read_json_manifest(&path).expect("read replacement");
        assert_eq!(value, json!({"version": 2}));
    }

    #[test]
    fn new_manifest_write_never_clobbers_an_existing_file() {
        let temp = tempdir().expect("tempdir");
        let path = temp.path().join(".oaartwork");
        write_new_json_manifest(&path, &json!({"owner": "orphan"})).expect("first write");
        assert!(write_new_json_manifest(&path, &json!({"owner": "new"})).is_err());
        let value: Value = read_json_manifest(&path).expect("read original");
        assert_eq!(value, json!({"owner": "orphan"}));
    }
}
