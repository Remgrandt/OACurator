use super::{
    artwork_manifest_from_detail, merge_manifest_only_entries, AppError, ArtworkManifest, Catalog,
    Result,
};
use crate::manifest::{read_json_manifest, write_json_manifest, write_new_json_manifest};
use chrono::Utc;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const ARTWORK_PROJECTION_KIND: &str = "artwork";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ManifestProjectionIssue {
    pub owner_kind: String,
    pub owner_stable_id: String,
    pub owner_id: Option<i64>,
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
        self.project_artwork_internal(artwork_id, false, None)
    }

    pub fn project_new_artwork(&self, artwork_id: i64) -> Result<()> {
        self.project_artwork_internal(artwork_id, false, Some(false))
    }

    pub fn reconcile_artwork(&self, artwork_id: i64) -> Result<()> {
        self.project_artwork_internal(artwork_id, true, Some(true))
    }

    fn project_artwork_internal(
        &self,
        artwork_id: i64,
        preserve_manifest_only: bool,
        overwrite: Option<bool>,
    ) -> Result<()> {
        let (manifest_path, mut manifest) = self.artwork_manifest_for_write(artwork_id)?;
        if preserve_manifest_only {
            if let Ok(existing) = read_json_manifest::<ArtworkManifest>(&manifest_path) {
                merge_manifest_only_entries(&mut manifest, existing);
            }
        }

        let write_result = if overwrite.unwrap_or_else(|| manifest_path.exists()) {
            write_json_manifest(&manifest_path, &manifest)
        } else {
            write_new_json_manifest(&manifest_path, &manifest)
        };
        match write_result {
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

    pub(crate) fn artwork_manifest_for_write(
        &self,
        artwork_id: i64,
    ) -> Result<(PathBuf, ArtworkManifest)> {
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
        Ok((
            manifest_path,
            artwork_manifest_from_detail(&detail, &asset_folder),
        ))
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
                    issue.owner_kind, issue.owner_stable_id
                ));
                continue;
            }
            let Some(artwork_id) = self
                .catalog
                .artwork_id_for_manifest_path(&issue.manifest_path)?
            else {
                report.failed += 1;
                report.messages.push(format!(
                    "Artwork {} is not currently loaded; its failed manifest write remains recorded at {}",
                    issue.owner_stable_id,
                    issue.manifest_path.display()
                ));
                continue;
            };
            match projector.reconcile_artwork(artwork_id) {
                Ok(()) => report.repaired += 1,
                Err(error) => {
                    report.failed += 1;
                    report.messages.push(format!(
                        "Failed to repair {} {}: {}",
                        issue.owner_kind, issue.owner_stable_id, error
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
        let owner_stable_id = match owner_kind {
            ARTWORK_PROJECTION_KIND => conn
                .query_row(
                    "SELECT COALESCE(artwork_stable_id, canonical_id) FROM artwork WHERE id = ?1",
                    params![owner_id],
                    |row| row.get::<_, String>(0),
                )
                .optional()?
                .unwrap_or_else(|| format!("{owner_kind}:{owner_id}")),
            _ => format!("{owner_kind}:{owner_id}"),
        };
        conn.execute(
            "INSERT INTO manifest_projection_state
             (owner_kind, owner_stable_id, owner_id, manifest_path, error, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(owner_kind, manifest_path) DO UPDATE SET
               owner_stable_id = excluded.owner_stable_id,
               owner_id = excluded.owner_id,
               error = excluded.error,
               updated_at = excluded.updated_at",
            params![
                owner_kind,
                owner_stable_id,
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
        let manifest_path = match owner_kind {
            ARTWORK_PROJECTION_KIND => conn
                .query_row(
                    "SELECT artwork_manifest_path FROM artwork WHERE id = ?1",
                    params![owner_id],
                    |row| row.get::<_, Option<String>>(0),
                )
                .optional()?
                .flatten(),
            _ => None,
        };
        conn.execute(
            "DELETE FROM manifest_projection_state
             WHERE owner_kind = ?1
               AND ((?2 IS NOT NULL AND manifest_path = ?2) OR owner_id = ?3)",
            params![owner_kind, manifest_path, owner_id],
        )?;
        Ok(())
    }

    pub fn dirty_manifest_projections(&self) -> Result<Vec<ManifestProjectionIssue>> {
        let conn = self.lock()?;
        let mut statement = conn.prepare(
            "SELECT owner_kind, owner_stable_id, owner_id, manifest_path, error, updated_at
             FROM manifest_projection_state
             ORDER BY updated_at, owner_kind, owner_stable_id",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(ManifestProjectionIssue {
                owner_kind: row.get(0)?,
                owner_stable_id: row.get(1)?,
                owner_id: row.get(2)?,
                manifest_path: PathBuf::from(row.get::<_, String>(3)?),
                error: row.get(4)?,
                updated_at: row.get(5)?,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(AppError::from)
    }

    fn artwork_id_for_manifest_path(&self, manifest_path: &std::path::Path) -> Result<Option<i64>> {
        let conn = self.lock()?;
        conn.query_row(
            "SELECT id FROM artwork WHERE artwork_manifest_path = ?1",
            params![manifest_path.to_string_lossy()],
            |row| row.get(0),
        )
        .optional()
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
        assert_eq!(issues[0].owner_id, Some(42));
        assert_eq!(issues[0].owner_stable_id, "artwork:42");
        assert_eq!(issues[0].error, "write failed");

        catalog.init().expect("reinitialize");
        let issues = catalog
            .dirty_manifest_projections()
            .expect("durable dirty issues");
        assert_eq!(issues.len(), 1);
        assert_eq!(
            issues[0].manifest_path,
            temp.path().join("missing.oaartwork")
        );

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
