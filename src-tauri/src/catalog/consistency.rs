use super::{Catalog, ManifestProjectionIssue};
use crate::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MissingArtworkManifest {
    pub artwork_id: i64,
    pub canonical_id: String,
    pub title: String,
    pub manifest_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MissingArtworkFile {
    pub artwork_id: i64,
    pub file_asset_id: i64,
    pub path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct CatalogConsistencyReport {
    pub artworks_checked: usize,
    pub missing_artwork_manifests: Vec<MissingArtworkManifest>,
    pub missing_artwork_files: Vec<MissingArtworkFile>,
    pub dirty_manifest_projections: Vec<ManifestProjectionIssue>,
}

pub struct CatalogConsistencyCheck<'a> {
    catalog: &'a Catalog,
}

impl<'a> CatalogConsistencyCheck<'a> {
    pub fn new(catalog: &'a Catalog) -> Self {
        Self { catalog }
    }

    pub fn run(&self) -> Result<CatalogConsistencyReport> {
        let summaries = self.catalog.list_artworks()?;
        let mut report = CatalogConsistencyReport {
            artworks_checked: summaries.len(),
            dirty_manifest_projections: self.catalog.dirty_manifest_projections()?,
            ..CatalogConsistencyReport::default()
        };

        for summary in summaries {
            if !summary
                .manifest_path
                .as_ref()
                .is_some_and(|path| path.is_file())
            {
                report
                    .missing_artwork_manifests
                    .push(MissingArtworkManifest {
                        artwork_id: summary.id,
                        canonical_id: summary.canonical_id.clone(),
                        title: summary.title.clone(),
                        manifest_path: summary.manifest_path.clone(),
                    });
            }

            let detail = self.catalog.artwork_detail(summary.id)?;
            for file in detail.file_assets {
                if !file.current_path.is_file() {
                    report.missing_artwork_files.push(MissingArtworkFile {
                        artwork_id: detail.id,
                        file_asset_id: file.id,
                        path: file.current_path,
                    });
                }
            }
        }

        Ok(report)
    }
}
