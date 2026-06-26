use super::{
    artwork_manifest_from_detail, merge_manifest_only_entries, AppError, ArtworkManifest, Catalog,
    Result,
};
use crate::manifest::{read_json_manifest, write_json_manifest};
use chrono::Utc;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const ARTWORK_PROJECTION_KIND: &str = "artwork";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ManifestProjectionIssue {
    pub owner_kind: String,
    pub owner_id: i64,
    pub manifest_path: PathBuf,
    pub error: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ManifestRepairReport {
    pub repaired: usize,
    pub failed: usize,
    pub messages: Vec<String>,
}

pub struct ManifestProjector<'a> {
    catalog: &'a Catalog,
}

impl<'a> ManifestProjector<'a> {
    pub fn new(catalog: &'a Catalog) -> Self {
        Self { catalog }
    }

    pub fn project_artwork(&self, artwork_id: i64) -> Result<()> {
        self.project_artwork_internal(artwork_id, false)
    }

    pub fn reconcile_artwork(&self, artwork_id: i64) -> Result<()> {
        self.project_artwork_internal(artwork_id, true)
    }

    fn project_artwork_internal(
        &self,
        artwork_id: i64,
        preserve_manifest_only: bool,
    ) -> Result<()> {
        let detail = self.catalog.artwork_detail(artwork_id)?;
        let Some(manifest_path) = self.catalog.artwork_manifest_path(artwork_id)? else {
            return Err(AppError::Message(format!(
                "Artwork {} does not have an .oaartwork manifest path",
                detail.canonical_id
            )));
        };
        let asset_folder = manifest_path
            .parent()
            .unwrap_or(&manifest_path)
            .to_path_buf();
        let mut manifest = artwork_manifest_from_detail(&detail, &asset_folder);
        if preserve_manifest_only {
            if let Ok(existing) = read_json_manifest::<ArtworkManifest>(&manifest_path) {
                merge_manifest_only_entries(&mut manifest, existing);
            }
        }

        match write_json_manifest(&manifest_path, &manifest) {
            Ok(()) => {
                self.catalog
                    .clear_manifest_projection_dirty(ARTWORK_PROJECTION_KIND, artwork_id)?;
                Ok(())
            }
            Err(error) => {
                let message = error.to_string();
                let _ = self.catalog.mark_manifest_projection_dirty(
                    ARTWORK_PROJECTION_KIND,
                    artwork_id,
                    &manifest_path,
                    &message,
                );
                Err(AppError::Message(message))
            }
        }
    }
}

pub struct ManifestRepairService<'a> {
    catalog: &'a Catalog,
}

impl<'a> ManifestRepairService<'a> {
    pub fn new(catalog: &'a Catalog) -> Self {
        Self { catalog }
    }

    pub fn repair_dirty_projections(&self) -> Result<ManifestRepairReport> {
        let projector = ManifestProjector::new(self.catalog);
        let mut report = ManifestRepairReport::default();
        for issue in self.catalog.dirty_manifest_projections()? {
            if issue.owner_kind != ARTWORK_PROJECTION_KIND {
                report.failed += 1;
                report.messages.push(format!(
                    "Unsupported dirty manifest projection kind: {} {}",
                    issue.owner_kind, issue.owner_id
                ));
                continue;
            }
            match projector.reconcile_artwork(issue.owner_id) {
                Ok(()) => report.repaired += 1,
                Err(error) => {
                    report.failed += 1;
                    report.messages.push(format!(
                        "Failed to repair {} {}: {}",
                        issue.owner_kind, issue.owner_id, error
                    ));
                }
            }
        }
        Ok(report)
    }
}

impl Catalog {
    pub(crate) fn mark_manifest_projection_dirty(
        &self,
        owner_kind: &str,
        owner_id: i64,
        manifest_path: &std::path::Path,
        error: &str,
    ) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        let conn = self.lock()?;
        conn.execute(
            "INSERT INTO manifest_projection_state
             (owner_kind, owner_id, manifest_path, error, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(owner_kind, owner_id) DO UPDATE SET
               manifest_path = excluded.manifest_path,
               error = excluded.error,
               updated_at = excluded.updated_at",
            params![
                owner_kind,
                owner_id,
                manifest_path.to_string_lossy(),
                error,
                now
            ],
        )?;
        Ok(())
    }

    pub(crate) fn clear_manifest_projection_dirty(
        &self,
        owner_kind: &str,
        owner_id: i64,
    ) -> Result<()> {
        let conn = self.lock()?;
        conn.execute(
            "DELETE FROM manifest_projection_state WHERE owner_kind = ?1 AND owner_id = ?2",
            params![owner_kind, owner_id],
        )?;
        Ok(())
    }

    pub fn dirty_manifest_projections(&self) -> Result<Vec<ManifestProjectionIssue>> {
        let conn = self.lock()?;
        let mut statement = conn.prepare(
            "SELECT owner_kind, owner_id, manifest_path, error, updated_at
             FROM manifest_projection_state
             ORDER BY updated_at, owner_kind, owner_id",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(ManifestProjectionIssue {
                owner_kind: row.get(0)?,
                owner_id: row.get(1)?,
                manifest_path: PathBuf::from(row.get::<_, String>(2)?),
                error: row.get(3)?,
                updated_at: row.get(4)?,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(AppError::from)
    }
}

#[cfg(test)]
mod tests {
    use super::ManifestRepairService;
    use crate::catalog::Catalog;
    use tempfile::tempdir;

    #[test]
    fn dirty_manifest_projection_state_round_trips() {
        let temp = tempdir().expect("tempdir");
        let catalog = Catalog::open(temp.path().join("catalog.sqlite3")).expect("catalog");
        catalog.init().expect("init");
        catalog
            .mark_manifest_projection_dirty(
                "artwork",
                42,
                &temp.path().join("missing.oaartwork"),
                "write failed",
            )
            .expect("mark dirty");

        let issues = catalog.dirty_manifest_projections().expect("dirty issues");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].owner_kind, "artwork");
        assert_eq!(issues[0].owner_id, 42);
        assert_eq!(issues[0].error, "write failed");

        catalog
            .clear_manifest_projection_dirty("artwork", 42)
            .expect("clear dirty");
        assert!(catalog
            .dirty_manifest_projections()
            .expect("dirty issues")
            .is_empty());
    }

    #[test]
    fn repair_report_counts_unsupported_dirty_kinds() {
        let temp = tempdir().expect("tempdir");
        let catalog = Catalog::open(temp.path().join("catalog.sqlite3")).expect("catalog");
        catalog.init().expect("init");
        catalog
            .mark_manifest_projection_dirty(
                "collection",
                1,
                &temp.path().join(".oacollection"),
                "write failed",
            )
            .expect("mark dirty");

        let report = ManifestRepairService::new(&catalog)
            .repair_dirty_projections()
            .expect("repair report");
        assert_eq!(report.repaired, 0);
        assert_eq!(report.failed, 1);
    }
}
