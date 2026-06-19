// Copyright (c) 2026 Remgrandt Works. All rights reserved.

use crate::image_metadata::{read_image_metadata, ImageMetadata};
use crate::manifest::{
    read_json_manifest, write_json_manifest, ArtworkArtistCredit, ArtworkFileManifest,
    ArtworkManifest, ArtworkManifestReference, ArtworkPrivateMetadata, ArtworkPublicMetadata,
    CollectionManifest, ExternalLinkManifest, GalleryManifest, ManifestReference, SCHEMA_VERSION,
};
use crate::path_safety::validate_file_name_component;
use crate::{AppError, Result};
use chrono::{NaiveDate, NaiveDateTime, Utc};
use rusqlite::types::Value as SqlValue;
use rusqlite::{params, params_from_iter, Connection, OptionalExtension};
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Instant;

mod consistency;
mod delete;
mod manifests;
mod models;

pub use consistency::{
    CatalogConsistencyCheck, CatalogConsistencyReport, MissingArtworkFile, MissingArtworkManifest,
};
use delete::{
    delete_candidate_for_derived_asset, file_source_kind_delete_reason, move_path_to_trash,
    pretrash_managed_files_or_abort, push_unique_delete_candidate,
    trash_delete_candidate_if_exists, trash_file_if_exists,
};
pub use manifests::{
    ManifestProjectionIssue, ManifestProjector, ManifestRepairReport, ManifestRepairService,
};
pub use models::*;

const RECENT_COLLECTIONS_SETTING: &str = "recent_collections_json";
const RECENT_COLLECTION_LIMIT: usize = 12;
const SUMMARY_QUERY_CHUNK_SIZE: usize = 500;
const ARTWORK_ID_LABEL_PREFERENCE_SETTING: &str = "artwork_id_label_preference";
const DEFAULT_ATTACH_MODE_SETTING: &str = "default_attach_mode";
const DEFAULT_PNG_EXPORT_VARIANT_SETTING: &str = "default_png_export_variant";
const DEFAULT_PROVIDER_FOCUS_SETTING: &str = "default_provider_focus";
const THEME_PREFERENCE_SETTING: &str = "theme";
const STARTUP_BEHAVIOR_SETTING: &str = "startup_behavior";
const DEFAULT_WORKSPACE_ROOT_SETTING: &str = "default_workspace_root";
const RAREMARQ_CSV_EXPORT_SCOPE_SETTING: &str = "raremarq_csv_export_scope";
const RAREMARQ_CSV_URL_MODE_SETTING: &str = "raremarq_csv_url_mode";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ManifestRewriteDebugCounts {
    pub gallery: usize,
}

#[derive(Clone)]
pub struct Catalog {
    conn: Arc<Mutex<Connection>>,
    manifest_rewrite_debug_counts: Arc<Mutex<ManifestRewriteDebugCounts>>,
}

const MEDIA_TYPE_OPTIONS: &[(&str, &str)] = &[
    ("12", "Colored Pencils"),
    ("11", "Computer Art"),
    ("1", "Ink Wash"),
    ("2", "Marker"),
    ("3", "Mixed Media"),
    ("4", "Paint - Acrylic"),
    ("5", "Paint - Oil"),
    ("6", "Paint - Watercolor"),
    ("13", "Pastels"),
    ("7", "Pen and Ink"),
    ("8", "Pencil"),
    ("9", "Photograph"),
];

const ART_TYPE_OPTIONS: &[(&str, &str)] = &[
    ("18", "Animation"),
    ("16", "Color Guide"),
    ("10", "Comic Strip"),
    ("2", "Commission"),
    ("21", "Complete Story"),
    ("8", "Convention Sketch"),
    ("1", "Cover"),
    ("12", "Double Page Splash"),
    ("13", "Double Page Spread"),
    ("23", "Illustration"),
    ("3", "Interior Page"),
    ("25", "Mystery Sketch"),
    ("26", "Mystery Sketch Card"),
    ("5", "Other"),
    ("22", "Partial Story"),
    ("20", "Photograph"),
    ("6", "Pin Up"),
    ("9", "Prelim"),
    ("14", "Recreation"),
    ("15", "Remarked Item"),
    ("11", "Sketch Card"),
    ("24", "Sketch Cover"),
    ("7", "Sketchbook"),
    ("4", "Splash Page"),
    ("17", "Title Page"),
    ("19", "Trading Card Art"),
];

const PUBLICATION_STATUS_OPTIONS: &[(&str, &str)] = &[
    ("2", "Unpublished Art"),
    ("1", "Published Art"),
    ("3", "CAF Member Art"),
];

const ARTIST_ROLE_OPTIONS: &[(&str, &str)] = &[
    ("1", "Penciller"),
    ("3", "Colorist"),
    ("5", "Inker"),
    ("6", "All"),
    ("7", "Painter"),
    ("8", "Layouts"),
    ("9", "Finisher"),
    ("10", "Assistant"),
    ("11", "Letterer"),
    ("12", "Writer"),
    ("13", "Restorer"),
];

impl ArtworkIdLabelPreference {
    pub fn as_setting_value(self) -> &'static str {
        match self {
            Self::Oac => "oac",
            Self::PreferCaf => "caf",
            Self::PreferSnikt => "snikt",
            Self::PreferRaremarq => "raremarq",
        }
    }

    pub fn from_setting_value(value: &str) -> Result<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "" | "oac" => Ok(Self::Oac),
            "caf" => Ok(Self::PreferCaf),
            "snikt" => Ok(Self::PreferSnikt),
            "raremarq" => Ok(Self::PreferRaremarq),
            _ => Err(AppError::Message(
                "Artwork ID preference must be oac, caf, snikt, or raremarq".to_string(),
            )),
        }
    }
}

impl CollectionOpenProfiler {
    fn from_env(manifest_path: &Path) -> Self {
        let enabled = cfg!(debug_assertions) && env_flag("OACURATOR_DEBUG_PROFILE_COLLECTION_OPEN");
        Self {
            enabled,
            output_path: env::var_os("OACURATOR_DEBUG_PROFILE_PATH")
                .map(PathBuf::from)
                .or_else(|| {
                    enabled.then(|| env::temp_dir().join("OACurator-collection-open-profile.json"))
                }),
            profile: CollectionOpenDebugProfile {
                collection_path: manifest_path.to_string_lossy().to_string(),
                ..CollectionOpenDebugProfile::default()
            },
        }
    }

    fn finish(&mut self, collection_name: &str, total_started: Instant) -> Result<()> {
        if !self.enabled {
            return Ok(());
        }
        self.profile.collection_name = Some(collection_name.to_string());
        self.profile.total_ms = elapsed_ms(total_started);
        let Some(output_path) = self.output_path.as_ref() else {
            return Ok(());
        };
        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(output_path, serde_json::to_vec_pretty(&self.profile)?)?;
        Ok(())
    }
}

pub(crate) struct CatalogBatchTransaction<'a> {
    catalog: &'a Catalog,
    committed: bool,
}

pub(crate) struct CanonicalIdAllocator {
    next_number: i64,
}

impl CanonicalIdAllocator {
    fn new(conn: &Connection) -> rusqlite::Result<Self> {
        let mut statement = conn.prepare(
            "SELECT canonical_id FROM artwork
             UNION ALL
             SELECT artwork_stable_id FROM artwork WHERE artwork_stable_id IS NOT NULL",
        )?;
        let ids = statement.query_map([], |row| row.get::<_, String>(0))?;
        let next_number = ids
            .filter_map(std::result::Result::ok)
            .filter_map(|id| id.strip_prefix("OAC-")?.parse::<i64>().ok())
            .max()
            .unwrap_or(0)
            + 1;
        Ok(Self { next_number })
    }

    fn next_id(&mut self) -> String {
        let id = format!("OAC-{:05}", self.next_number);
        self.next_number += 1;
        id
    }
}

impl<'a> CatalogBatchTransaction<'a> {
    pub(crate) fn begin(catalog: &'a Catalog) -> Result<Self> {
        {
            let conn = catalog.lock()?;
            conn.execute_batch("BEGIN IMMEDIATE TRANSACTION;")?;
        }
        Ok(Self {
            catalog,
            committed: false,
        })
    }

    pub(crate) fn commit(mut self) -> Result<()> {
        {
            let conn = self.catalog.lock()?;
            conn.execute_batch("COMMIT;")?;
        }
        self.committed = true;
        Ok(())
    }
}

impl Drop for CatalogBatchTransaction<'_> {
    fn drop(&mut self) {
        if !self.committed {
            if let Ok(conn) = self.catalog.lock() {
                let _ = conn.execute_batch("ROLLBACK;");
            }
        }
    }
}

pub fn artist_role_label_for_id(role_id: &str) -> Option<&'static str> {
    controlled_label(ARTIST_ROLE_OPTIONS, role_id)
}

pub fn artist_role_id_for_label(label: &str) -> Option<&'static str> {
    controlled_id_for_label(ARTIST_ROLE_OPTIONS, label)
}

pub fn media_type_id_for_label(label: &str) -> Option<&'static str> {
    controlled_id_for_label(MEDIA_TYPE_OPTIONS, label)
}

pub fn art_type_id_for_label(label: &str) -> Option<&'static str> {
    controlled_id_for_label(ART_TYPE_OPTIONS, label)
}

impl Catalog {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        if let Some(parent) = path.as_ref().parent() {
            fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
            manifest_rewrite_debug_counts: Arc::new(Mutex::new(
                ManifestRewriteDebugCounts::default(),
            )),
        })
    }

    pub(crate) fn begin_batch_transaction(&self) -> Result<CatalogBatchTransaction<'_>> {
        CatalogBatchTransaction::begin(self)
    }

    pub(crate) fn canonical_id_allocator(&self) -> Result<CanonicalIdAllocator> {
        let conn = self.lock()?;
        CanonicalIdAllocator::new(&conn).map_err(AppError::from)
    }

    pub fn init(&self) -> Result<()> {
        let conn = self.lock()?;
        conn.execute_batch(
            r#"
            PRAGMA foreign_keys = ON;

            CREATE TABLE IF NOT EXISTS app_setting (
              key TEXT PRIMARY KEY,
              value TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS artwork (
              id INTEGER PRIMARY KEY AUTOINCREMENT,
              canonical_id TEXT NOT NULL UNIQUE,
              artwork_stable_id TEXT,
              artwork_manifest_path TEXT,
              title TEXT NOT NULL,
              description TEXT,
              for_sale_status TEXT NOT NULL DEFAULT 'NFS',
              media_type_id TEXT NOT NULL DEFAULT '7',
              media TEXT,
              art_type_id TEXT NOT NULL DEFAULT '3',
              format TEXT,
              publication_status_id TEXT NOT NULL DEFAULT '2',
              active INTEGER NOT NULL DEFAULT 1,
              illustration_exchange INTEGER NOT NULL DEFAULT 0,
              ix_for_sale INTEGER NOT NULL DEFAULT 0,
              source_folder TEXT NOT NULL UNIQUE,
              source_context TEXT NOT NULL DEFAULT '',
              caf_csv_image_link TEXT,
              caf_csv_added_to_caf TEXT,
              snikt_csv_created_date TEXT,
              created_at TEXT NOT NULL,
              updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS collection (
              id INTEGER PRIMARY KEY AUTOINCREMENT,
              stable_id TEXT NOT NULL UNIQUE,
              name TEXT NOT NULL,
              manifest_path TEXT NOT NULL UNIQUE,
              caf_collection_id TEXT,
              snikt_collection_id TEXT,
              raremarq_collection_id TEXT,
              created_at TEXT NOT NULL,
              updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS gallery (
              id INTEGER PRIMARY KEY AUTOINCREMENT,
              stable_id TEXT NOT NULL UNIQUE,
              name TEXT NOT NULL,
              manifest_path TEXT NOT NULL UNIQUE,
              caf_gallery_room_id TEXT,
              snikt_gallery_id TEXT,
              snikt_gallery_inherits_collection INTEGER NOT NULL DEFAULT 1,
              raremarq_gallery_id TEXT,
              created_at TEXT NOT NULL,
              updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS collection_gallery (
              collection_id INTEGER NOT NULL REFERENCES collection(id) ON DELETE CASCADE,
              gallery_id INTEGER NOT NULL REFERENCES gallery(id) ON DELETE CASCADE,
              sort_order INTEGER NOT NULL DEFAULT 0,
              PRIMARY KEY (collection_id, gallery_id)
            );

            CREATE TABLE IF NOT EXISTS gallery_artwork (
              gallery_id INTEGER NOT NULL REFERENCES gallery(id) ON DELETE CASCADE,
              artwork_id INTEGER NOT NULL REFERENCES artwork(id) ON DELETE CASCADE,
              sort_order INTEGER NOT NULL DEFAULT 0,
              PRIMARY KEY (gallery_id, artwork_id)
            );

            CREATE TABLE IF NOT EXISTS file_asset (
              id INTEGER PRIMARY KEY AUTOINCREMENT,
              artwork_id INTEGER NOT NULL REFERENCES artwork(id) ON DELETE CASCADE,
              original_path TEXT NOT NULL,
              current_path TEXT NOT NULL UNIQUE,
              relative_path TEXT NOT NULL,
              file_name TEXT NOT NULL,
              extension TEXT NOT NULL,
              size_bytes INTEGER NOT NULL,
              width INTEGER,
              height INTEGER,
              dpi_x REAL,
              dpi_y REAL,
              metadata_checked_at TEXT,
              modified_at TEXT,
              image_role TEXT,
              source_kind TEXT NOT NULL DEFAULT 'linked',
              is_primary INTEGER NOT NULL DEFAULT 0,
              display_order INTEGER NOT NULL DEFAULT 0
            );

            CREATE TABLE IF NOT EXISTS derived_asset (
              id INTEGER PRIMARY KEY AUTOINCREMENT,
              artwork_id INTEGER NOT NULL REFERENCES artwork(id) ON DELETE CASCADE,
              source_file_asset_id INTEGER REFERENCES file_asset(id) ON DELETE SET NULL,
              derivative_type TEXT NOT NULL,
              format TEXT NOT NULL,
              path TEXT NOT NULL UNIQUE,
              width INTEGER NOT NULL,
              height INTEGER NOT NULL,
              image_role TEXT,
              created_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS artist (
              id INTEGER PRIMARY KEY AUTOINCREMENT,
              name TEXT NOT NULL UNIQUE
            );

            CREATE TABLE IF NOT EXISTS artwork_artist (
              artwork_id INTEGER NOT NULL REFERENCES artwork(id) ON DELETE CASCADE,
              artist_id INTEGER NOT NULL REFERENCES artist(id) ON DELETE CASCADE,
              role TEXT,
              first_name TEXT,
              last_name TEXT,
              role_id TEXT,
              sort_order INTEGER NOT NULL,
              PRIMARY KEY (artwork_id, artist_id, role)
            );

            CREATE TABLE IF NOT EXISTS term_media (
              value TEXT PRIMARY KEY
            );

            CREATE TABLE IF NOT EXISTS term_format (
              value TEXT PRIMARY KEY
            );

            CREATE TABLE IF NOT EXISTS external_link (
              id INTEGER PRIMARY KEY AUTOINCREMENT,
              artwork_id INTEGER NOT NULL REFERENCES artwork(id) ON DELETE CASCADE,
              link_type TEXT NOT NULL,
              external_id TEXT,
              url TEXT NOT NULL,
              extensions_json TEXT,
              UNIQUE (artwork_id, link_type)
            );

            CREATE TABLE IF NOT EXISTS file_asset_external_link (
              id INTEGER PRIMARY KEY AUTOINCREMENT,
              file_asset_id INTEGER NOT NULL REFERENCES file_asset(id) ON DELETE CASCADE,
              provider TEXT NOT NULL,
              external_id TEXT NOT NULL,
              url TEXT NOT NULL,
              extensions_json TEXT,
              UNIQUE (file_asset_id, provider, external_id)
            );

            CREATE TABLE IF NOT EXISTS oaa_extension_block (
              id INTEGER PRIMARY KEY AUTOINCREMENT,
              owner_kind TEXT NOT NULL,
              owner_id INTEGER NOT NULL,
              provider TEXT NOT NULL,
              json TEXT NOT NULL,
              UNIQUE (owner_kind, owner_id, provider)
            );

            CREATE TABLE IF NOT EXISTS private_metadata (
              artwork_id INTEGER PRIMARY KEY REFERENCES artwork(id) ON DELETE CASCADE,
              purchase_price TEXT,
              estimated_value TEXT,
              purchase_date TEXT,
              personal_notes TEXT,
              provenance TEXT
            );

            CREATE TABLE IF NOT EXISTS snikt_metadata (
              artwork_id INTEGER PRIMARY KEY REFERENCES artwork(id) ON DELETE CASCADE,
              art_type TEXT,
              comic_publisher TEXT,
              series_title TEXT,
              issue_number TEXT,
              series_page_number TEXT,
              year TEXT,
              character TEXT,
              subcategory TEXT,
              animation_studio TEXT,
              episode_number TEXT,
              episode_title TEXT,
              published_date TEXT,
              strip_title TEXT,
              is_sunday_strip INTEGER NOT NULL DEFAULT 0,
              other TEXT,
              tags TEXT,
              is_nsfw INTEGER NOT NULL DEFAULT 0,
              is_for_sale INTEGER NOT NULL DEFAULT 0,
              price TEXT,
              is_open_to_offers INTEGER NOT NULL DEFAULT 0
            );

            CREATE TABLE IF NOT EXISTS file_operation_log (
              id INTEGER PRIMARY KEY AUTOINCREMENT,
              artwork_id INTEGER NOT NULL REFERENCES artwork(id) ON DELETE CASCADE,
              file_asset_id INTEGER REFERENCES file_asset(id) ON DELETE SET NULL,
              old_path TEXT NOT NULL,
              new_path TEXT NOT NULL,
              result TEXT NOT NULL,
              message TEXT,
              created_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS manifest_projection_state (
              owner_kind TEXT NOT NULL,
              owner_id INTEGER NOT NULL,
              manifest_path TEXT NOT NULL,
              error TEXT NOT NULL,
              updated_at TEXT NOT NULL,
              PRIMARY KEY (owner_kind, owner_id)
            );

            CREATE VIRTUAL TABLE IF NOT EXISTS artwork_search_fts
            USING fts5(
              artwork_id UNINDEXED,
              search_text,
              tokenize = 'unicode61'
            );

            CREATE INDEX IF NOT EXISTS idx_artwork_title ON artwork(title);
            CREATE INDEX IF NOT EXISTS idx_artwork_canonical ON artwork(canonical_id);
            CREATE INDEX IF NOT EXISTS idx_collection_name ON collection(name);
            CREATE INDEX IF NOT EXISTS idx_gallery_name ON gallery(name);
            CREATE INDEX IF NOT EXISTS idx_gallery_artwork_artwork ON gallery_artwork(artwork_id);
            CREATE INDEX IF NOT EXISTS idx_file_asset_artwork ON file_asset(artwork_id);
            CREATE INDEX IF NOT EXISTS idx_derived_asset_artwork ON derived_asset(artwork_id);
            CREATE INDEX IF NOT EXISTS idx_file_asset_external_link_asset ON file_asset_external_link(file_asset_id);
            CREATE INDEX IF NOT EXISTS idx_oaa_extension_block_owner ON oaa_extension_block(owner_kind, owner_id);
            CREATE INDEX IF NOT EXISTS idx_manifest_projection_state_updated_at ON manifest_projection_state(updated_at);
            "#,
        )?;
        add_column_if_missing(&conn, "external_link", "extensions_json", "TEXT")?;
        add_column_if_missing(&conn, "artwork", "caf_csv_image_link", "TEXT")?;
        add_column_if_missing(&conn, "artwork", "caf_csv_added_to_caf", "TEXT")?;
        add_column_if_missing(&conn, "artwork", "snikt_csv_created_date", "TEXT")?;
        add_column_if_missing(
            &conn,
            "gallery",
            "snikt_gallery_inherits_collection",
            "INTEGER NOT NULL DEFAULT 1",
        )?;
        conn.execute_batch(
            r#"
            CREATE UNIQUE INDEX IF NOT EXISTS idx_external_link_provider_id
              ON external_link(link_type, external_id)
              WHERE external_id IS NOT NULL;
            "#,
        )?;
        drop(conn);
        self.clear_working_catalog()?;
        Ok(())
    }

    fn clear_working_catalog(&self) -> Result<()> {
        let conn = self.lock()?;
        conn.execute_batch(
            r#"
            DELETE FROM file_operation_log;
            DELETE FROM manifest_projection_state;
            DELETE FROM artwork_search_fts;
            DELETE FROM file_asset_external_link;
            DELETE FROM oaa_extension_block;
            DELETE FROM private_metadata;
            DELETE FROM snikt_metadata;
            DELETE FROM artwork_artist;
            DELETE FROM external_link;
            DELETE FROM derived_asset;
            DELETE FROM file_asset;
            DELETE FROM gallery_artwork;
            DELETE FROM collection_gallery;
            DELETE FROM artwork;
            DELETE FROM artist;
            DELETE FROM gallery;
            DELETE FROM collection;
            DELETE FROM term_media;
            DELETE FROM term_format;
            "#,
        )?;
        set_setting_locked(&conn, "active_workspace_mode", "none")?;
        set_setting_locked(&conn, "active_collection_id", "")?;
        set_setting_locked(&conn, "active_gallery_id", "")?;
        Ok(())
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        let conn = self.lock()?;
        set_setting_locked(&conn, key, value)?;
        Ok(())
    }

    pub fn setting(&self, key: &str) -> Result<Option<String>> {
        let conn = self.lock()?;
        Ok(conn
            .query_row(
                "SELECT value FROM app_setting WHERE key = ?1",
                params![key],
                |row| row.get(0),
            )
            .optional()?)
    }

    pub fn recent_collections(&self) -> Result<Vec<RecentCollection>> {
        let conn = self.lock()?;
        recent_collections_locked(&conn)
    }

    pub fn app_preferences(&self, default_workspace_root: &str) -> Result<AppPreferences> {
        let conn = self.lock()?;
        Ok(AppPreferences {
            default_attach_mode: setting_or_default_locked(
                &conn,
                DEFAULT_ATTACH_MODE_SETTING,
                "copy",
            )?,
            default_png_export_variant: normalize_png_export_variant_setting(
                &setting_or_default_locked(&conn, DEFAULT_PNG_EXPORT_VARIANT_SETTING, "basic")?,
            )?,
            default_provider_focus: setting_or_default_locked(
                &conn,
                DEFAULT_PROVIDER_FOCUS_SETTING,
                "all",
            )?,
            artwork_id_label_preference: artwork_id_label_preference_locked(&conn)?
                .as_setting_value()
                .to_string(),
            theme: setting_or_default_locked(&conn, THEME_PREFERENCE_SETTING, "dracula")?,
            startup_behavior: setting_or_default_locked(
                &conn,
                STARTUP_BEHAVIOR_SETTING,
                "reopen_last",
            )?,
            default_workspace_root: setting_or_default_locked(
                &conn,
                DEFAULT_WORKSPACE_ROOT_SETTING,
                default_workspace_root,
            )?,
            raremarq_csv_export_scope: setting_or_default_locked(
                &conn,
                RAREMARQ_CSV_EXPORT_SCOPE_SETTING,
                "untracked",
            )?,
            raremarq_csv_url_mode: setting_or_default_locked(
                &conn,
                RAREMARQ_CSV_URL_MODE_SETTING,
                "generic_url",
            )?,
        })
    }

    pub fn set_app_preferences(&self, preferences: AppPreferences) -> Result<()> {
        let default_attach_mode = validate_setting_choice(
            &preferences.default_attach_mode,
            &["copy", "link"],
            "Default attach mode",
        )?;
        let default_png_export_variant =
            normalize_png_export_variant_setting(&preferences.default_png_export_variant)?;
        let default_provider_focus = validate_setting_choice(
            &preferences.default_provider_focus,
            &["all", "caf", "snikt", "raremarq"],
            "Default provider focus",
        )?;
        let artwork_id_label_preference =
            ArtworkIdLabelPreference::from_setting_value(&preferences.artwork_id_label_preference)?;
        let theme = validate_setting_choice(&preferences.theme, &["dracula", "alucard"], "Theme")?;
        let startup_behavior = validate_setting_choice(
            &preferences.startup_behavior,
            &["reopen_last", "show_start_window", "start_empty"],
            "Startup behavior",
        )?;
        let raremarq_csv_export_scope = validate_setting_choice(
            &preferences.raremarq_csv_export_scope,
            &["all", "untracked"],
            "Raremarq CSV export scope",
        )?;
        let raremarq_csv_url_mode = validate_setting_choice(
            &preferences.raremarq_csv_url_mode,
            &["generic_url", "blank", "tmpfiles"],
            "Raremarq CSV URL mode",
        )?;
        let default_workspace_root = preferences.default_workspace_root.trim();

        let conn = self.lock()?;
        set_setting_locked(&conn, DEFAULT_ATTACH_MODE_SETTING, &default_attach_mode)?;
        set_setting_locked(
            &conn,
            DEFAULT_PNG_EXPORT_VARIANT_SETTING,
            &default_png_export_variant,
        )?;
        set_setting_locked(
            &conn,
            DEFAULT_PROVIDER_FOCUS_SETTING,
            &default_provider_focus,
        )?;
        set_setting_locked(
            &conn,
            ARTWORK_ID_LABEL_PREFERENCE_SETTING,
            artwork_id_label_preference.as_setting_value(),
        )?;
        set_setting_locked(&conn, THEME_PREFERENCE_SETTING, &theme)?;
        set_setting_locked(&conn, STARTUP_BEHAVIOR_SETTING, &startup_behavior)?;
        set_setting_locked(
            &conn,
            DEFAULT_WORKSPACE_ROOT_SETTING,
            default_workspace_root,
        )?;
        set_setting_locked(
            &conn,
            RAREMARQ_CSV_EXPORT_SCOPE_SETTING,
            &raremarq_csv_export_scope,
        )?;
        set_setting_locked(&conn, RAREMARQ_CSV_URL_MODE_SETTING, &raremarq_csv_url_mode)?;
        Ok(())
    }

    fn record_recent_collection(&self, collection: &CollectionSummary) -> Result<()> {
        let conn = self.lock()?;
        let mut recent = recent_collections_locked(&conn)?;
        let path = collection.manifest_path.clone();
        recent.retain(|item| item.path != path);
        recent.insert(
            0,
            RecentCollection {
                name: collection.name.clone(),
                path,
                last_opened_at: Utc::now().to_rfc3339(),
            },
        );
        recent.truncate(RECENT_COLLECTION_LIMIT);
        let value = serde_json::to_string(&recent)?;
        set_setting_locked(&conn, RECENT_COLLECTIONS_SETTING, &value)?;
        Ok(())
    }

    pub fn reset_manifest_rewrite_debug_counts(&self) {
        if let Ok(mut counts) = self.manifest_rewrite_debug_counts.lock() {
            *counts = ManifestRewriteDebugCounts::default();
        }
    }

    pub fn manifest_rewrite_debug_counts(&self) -> ManifestRewriteDebugCounts {
        self.manifest_rewrite_debug_counts
            .lock()
            .map(|counts| *counts)
            .unwrap_or_default()
    }

    pub fn artwork_id_label_preference(&self) -> Result<ArtworkIdLabelPreference> {
        self.setting(ARTWORK_ID_LABEL_PREFERENCE_SETTING)?
            .as_deref()
            .map(ArtworkIdLabelPreference::from_setting_value)
            .unwrap_or(Ok(ArtworkIdLabelPreference::Oac))
    }

    pub fn set_artwork_id_label_preference(
        &self,
        preference: ArtworkIdLabelPreference,
    ) -> Result<()> {
        self.set_setting(
            ARTWORK_ID_LABEL_PREFERENCE_SETTING,
            preference.as_setting_value(),
        )
    }

    pub fn create_collection(&self, name: &str, manifest_path: &Path) -> Result<CollectionSummary> {
        self.create_collection_with_caf_collection_id(name, manifest_path, None)
    }

    pub fn create_collection_with_caf_collection_id(
        &self,
        name: &str,
        manifest_path: &Path,
        caf_collection_id: Option<&str>,
    ) -> Result<CollectionSummary> {
        self.create_collection_with_provider_ids(name, manifest_path, caf_collection_id, None, None)
    }

    pub fn create_collection_with_provider_ids(
        &self,
        name: &str,
        manifest_path: &Path,
        caf_collection_id: Option<&str>,
        snikt_collection_id: Option<&str>,
        raremarq_collection_id: Option<&str>,
    ) -> Result<CollectionSummary> {
        let now = Utc::now().to_rfc3339();
        let stable_id = stable_id("collection");
        let name = normalized_name(name, "Untitled Collection");
        let caf_collection_id =
            normalize_caf_collection_id(caf_collection_id, "CAF Collection ID")?;
        let snikt_collection_id =
            normalize_snikt_collection_id(snikt_collection_id, "SNIKT.com Collection ID")?;
        let raremarq_collection_id =
            normalize_raremarq_collection_id(raremarq_collection_id, "Raremarq Collection ID")?;
        let manifest_path =
            manifest_path_from_input(manifest_path, &name, "oacollection", "Untitled Collection");
        let path_string = manifest_path.to_string_lossy().to_string();
        let id = {
            let conn = self.lock()?;
            conn.execute(
                "INSERT INTO collection (stable_id, name, manifest_path, caf_collection_id, snikt_collection_id, raremarq_collection_id, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    stable_id,
                    name,
                    path_string,
                    caf_collection_id,
                    snikt_collection_id,
                    raremarq_collection_id,
                    now,
                    now
                ],
            )?;
            conn.last_insert_rowid()
        };
        let collection = self.collection_summary(id)?;
        self.rewrite_collection_manifest(id)?;
        self.set_setting("active_workspace_mode", "collection")?;
        self.set_setting("active_collection_id", &id.to_string())?;
        self.record_recent_collection(&collection)?;
        Ok(collection)
    }

    pub fn open_collection(&self, manifest_path: &Path) -> Result<CollectionSummary> {
        self.open_collection_with_progress(manifest_path, |_| {})
    }

    pub fn open_collection_with_progress<F>(
        &self,
        manifest_path: &Path,
        mut progress: F,
    ) -> Result<CollectionSummary>
    where
        F: FnMut(WorkspaceLoadProgress),
    {
        let total_started = Instant::now();
        let mut profiler = CollectionOpenProfiler::from_env(manifest_path);
        let manifest_read_started = Instant::now();
        let manifest: CollectionManifest = read_json_manifest(manifest_path)?;
        profiler.profile.collection_manifest_read_ms = elapsed_ms(manifest_read_started);
        progress(WorkspaceLoadProgress {
            phase: "open_collection".to_string(),
            message: format!(
                "Opening Collection: loading artwork 0 of {}",
                manifest.artworks.len()
            ),
            artworks_total: manifest.artworks.len(),
            artworks_loaded: 0,
            current_artwork: None,
            done: manifest.artworks.is_empty(),
        });
        let batch_transaction = CatalogBatchTransaction::begin(self)?;
        let reset_started = Instant::now();
        self.clear_working_catalog()?;
        profiler.profile.reset_ms = elapsed_ms(reset_started);
        let now = Utc::now().to_rfc3339();
        let path_string = manifest_path.to_string_lossy().to_string();
        let caf_collection_id = provider_id(&manifest.external_links, "com.comicartfans");
        let snikt_collection_id = provider_id(&manifest.external_links, "com.snikt");
        let raremarq_collection_id = provider_id(&manifest.external_links, "com.raremarq");
        let upsert_started = Instant::now();
        let id = {
            let conn = self.lock()?;
            conn.execute(
                "INSERT INTO collection (stable_id, name, manifest_path, caf_collection_id, snikt_collection_id, raremarq_collection_id, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                 ON CONFLICT(manifest_path) DO UPDATE SET
                   stable_id = excluded.stable_id,
                   name = excluded.name,
                   caf_collection_id = excluded.caf_collection_id,
                   snikt_collection_id = excluded.snikt_collection_id,
                   raremarq_collection_id = excluded.raremarq_collection_id,
                   updated_at = excluded.updated_at",
                params![
                    manifest.id,
                    manifest.name,
                    path_string,
                    caf_collection_id,
                    snikt_collection_id,
                    raremarq_collection_id,
                    now,
                    now
                ],
            )?;
            conn.query_row(
                "SELECT id FROM collection WHERE manifest_path = ?1",
                params![path_string],
                |row| row.get(0),
            )?
        };
        profiler.profile.collection_upsert_ms = elapsed_ms(upsert_started);
        self.save_manifest_extension_blocks("collection", id, &manifest.extensions)?;
        let mut opened_gallery_manifests = Vec::new();
        profiler.profile.galleries_total = manifest.galleries.len();
        for reference in &manifest.galleries {
            let gallery_path = resolve_manifest_reference_path(manifest_path, &reference.path);
            if gallery_path.exists() {
                let read_started = Instant::now();
                let gallery_manifest: GalleryManifest = read_json_manifest(&gallery_path)?;
                profiler.profile.gallery_manifest_read_ms += elapsed_ms(read_started);
                let upsert_started = Instant::now();
                let gallery = self.open_gallery_without_activating(&gallery_path)?;
                self.link_gallery_to_collection_session_only(id, gallery.id)?;
                self.save_manifest_extension_blocks(
                    "gallery",
                    gallery.id,
                    &gallery_manifest.extensions,
                )?;
                profiler.profile.gallery_upsert_and_link_ms += elapsed_ms(upsert_started);
                opened_gallery_manifests.push((gallery, gallery_manifest));
            }
        }
        self.open_collection_artworks_from_manifest_tree(
            manifest_path,
            &manifest.artworks,
            &opened_gallery_manifests,
            &mut profiler.profile,
            &mut progress,
        )?;
        if let Some((gallery, _)) = opened_gallery_manifests.first() {
            self.set_setting("active_gallery_id", &gallery.id.to_string())?;
        }
        self.set_setting("active_workspace_mode", "collection")?;
        self.set_setting("active_collection_id", &id.to_string())?;
        batch_transaction.commit()?;
        let collection = self.collection_summary(id)?;
        self.record_recent_collection(&collection)?;
        profiler.finish(&collection.name, total_started)?;
        Ok(collection)
    }

    fn open_collection_artworks_from_manifest_tree(
        &self,
        collection_manifest_path: &Path,
        artwork_references: &[ArtworkManifestReference],
        gallery_manifests: &[(GallerySummary, GalleryManifest)],
        profile: &mut CollectionOpenDebugProfile,
        progress: &mut impl FnMut(WorkspaceLoadProgress),
    ) -> Result<()> {
        profile.artworks_total = artwork_references.len();
        let mut gallery_membership_by_artwork: BTreeMap<String, Vec<i64>> = BTreeMap::new();
        for (gallery, manifest) in gallery_manifests {
            for artwork in &manifest.artworks {
                gallery_membership_by_artwork
                    .entry(artwork.id.clone())
                    .or_default()
                    .push(gallery.id);
            }
        }
        let fallback_gallery_id = gallery_manifests.first().map(|(gallery, _)| gallery.id);

        let total = artwork_references.len();
        for (index, reference) in artwork_references.iter().enumerate() {
            let loaded = index + 1;
            let Some(relative_path) = reference.path.as_deref() else {
                progress(WorkspaceLoadProgress {
                    phase: "open_collection".to_string(),
                    message: format!("Opening Collection artwork {loaded} of {total}"),
                    artworks_total: total,
                    artworks_loaded: loaded,
                    current_artwork: None,
                    done: loaded == total,
                });
                continue;
            };
            let artwork_manifest_path =
                resolve_manifest_reference_path(collection_manifest_path, relative_path);
            if !artwork_manifest_path.exists() {
                progress(WorkspaceLoadProgress {
                    phase: "open_collection".to_string(),
                    message: format!("Opening Collection artwork {loaded} of {total}"),
                    artworks_total: total,
                    artworks_loaded: loaded,
                    current_artwork: None,
                    done: loaded == total,
                });
                continue;
            }
            let read_started = Instant::now();
            let artwork_manifest: ArtworkManifest = read_json_manifest(&artwork_manifest_path)?;
            profile.artwork_manifest_read_ms += elapsed_ms(read_started);
            profile.files_seen += artwork_manifest.files.len();
            if artwork_manifest.id != reference.id {
                return Err(AppError::Message(format!(
                    "Artwork manifest ID mismatch for {}",
                    artwork_manifest_path.display()
                )));
            }

            let mut member_gallery_ids = gallery_membership_by_artwork
                .get(&artwork_manifest.id)
                .cloned()
                .unwrap_or_default();
            if member_gallery_ids.is_empty() {
                if let Some(gallery_id) = fallback_gallery_id {
                    member_gallery_ids.push(gallery_id);
                }
            }

            let row_started = Instant::now();
            let artwork_id =
                self.upsert_artwork_manifest_row(&artwork_manifest_path, &artwork_manifest)?;
            profile.artwork_row_upsert_ms += elapsed_ms(row_started);
            let membership_started = Instant::now();
            for gallery_id in member_gallery_ids {
                self.link_artwork_to_gallery_session_only(gallery_id, artwork_id)?;
            }
            profile.artwork_membership_ms += elapsed_ms(membership_started);
            let payload_started = Instant::now();
            self.import_artwork_manifest_payload(
                artwork_id,
                &artwork_manifest_path,
                &artwork_manifest,
                profile,
            )?;
            profile.artwork_payload_ms += elapsed_ms(payload_started);
            progress(WorkspaceLoadProgress {
                phase: "open_collection".to_string(),
                message: format!(
                    "Opening Collection artwork {loaded} of {}: {}",
                    total, artwork_manifest.title
                ),
                artworks_total: total,
                artworks_loaded: loaded,
                current_artwork: Some(artwork_manifest.title.clone()),
                done: loaded == total,
            });
        }
        Ok(())
    }

    fn upsert_artwork_manifest_row(
        &self,
        manifest_path: &Path,
        manifest: &ArtworkManifest,
    ) -> Result<i64> {
        let now = Utc::now().to_rfc3339();
        let path_string = manifest_path.to_string_lossy().to_string();
        let existing_id = {
            let conn = self.lock()?;
            let by_manifest_path = conn
                .query_row(
                    "SELECT id FROM artwork WHERE artwork_manifest_path = ?1",
                    params![&path_string],
                    |row| row.get::<_, i64>(0),
                )
                .optional()?;
            if by_manifest_path.is_some() {
                by_manifest_path
            } else {
                conn.query_row(
                    "SELECT id FROM artwork WHERE canonical_id = ?1",
                    params![&manifest.id],
                    |row| row.get::<_, i64>(0),
                )
                .optional()?
            }
        };

        let conn = self.lock()?;
        if let Some(artwork_id) = existing_id {
            conn.execute(
                "UPDATE artwork SET
                   canonical_id = ?1,
                   artwork_stable_id = ?2,
                   title = ?3,
                   source_folder = ?4,
                   artwork_manifest_path = ?5,
                   updated_at = ?6
                 WHERE id = ?7",
                params![
                    &manifest.id,
                    &manifest.id,
                    &manifest.title,
                    &path_string,
                    &path_string,
                    &now,
                    artwork_id
                ],
            )?;
            Ok(artwork_id)
        } else {
            conn.execute(
                "INSERT INTO artwork
                 (canonical_id, artwork_stable_id, title, source_folder, source_context, artwork_manifest_path, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    &manifest.id,
                    &manifest.id,
                    &manifest.title,
                    &path_string,
                    "OAA manifest",
                    &path_string,
                    &now,
                    &now
                ],
            )?;
            Ok(conn.last_insert_rowid())
        }
    }

    fn import_artwork_manifest_payload(
        &self,
        artwork_id: i64,
        manifest_path: &Path,
        manifest: &ArtworkManifest,
        profile: &mut CollectionOpenDebugProfile,
    ) -> Result<()> {
        self.save_metadata_from_artwork_manifest(artwork_id, manifest)?;
        self.save_artwork_external_links_from_manifest(artwork_id, &manifest.external_links)?;
        self.save_manifest_extension_blocks("artwork", artwork_id, &manifest.extensions)?;
        if let Some(public_metadata) = manifest.public_metadata.as_ref() {
            self.save_manifest_extension_blocks(
                "artwork_public_metadata",
                artwork_id,
                &public_metadata.extensions,
            )?;
        }
        if let Some(private_metadata) = manifest.private_metadata.as_ref() {
            self.save_manifest_extension_blocks(
                "artwork_private_metadata",
                artwork_id,
                &private_metadata.extensions,
            )?;
        }
        profile.files_imported +=
            self.import_file_assets_from_artwork_manifest(artwork_id, manifest_path, manifest)?;
        Ok(())
    }

    fn save_metadata_from_artwork_manifest(
        &self,
        artwork_id: i64,
        manifest: &ArtworkManifest,
    ) -> Result<()> {
        let public = manifest.public_metadata.as_ref();
        let private = manifest.private_metadata.as_ref();
        let public_extensions = public.map(|metadata| &metadata.extensions);
        let caf_extension =
            public_extensions.and_then(|extensions| extensions.get("com.comicartfans"));
        let caf_csv_image_link = caf_extension
            .and_then(|extension| extension.get("csv_image_link"))
            .and_then(serde_json::Value::as_str)
            .map(str::to_string);
        let caf_csv_added_to_caf = caf_extension
            .and_then(|extension| extension.get("csv_added_to_caf"))
            .and_then(serde_json::Value::as_str)
            .map(str::to_string);
        let snikt_csv_created_date = public_extensions
            .and_then(|extensions| extensions.get("com.snikt"))
            .and_then(|extension| extension.get("metadata"))
            .and_then(|metadata| metadata.get("csv_created_date"))
            .and_then(serde_json::Value::as_str)
            .map(str::to_string);
        let publication_status_id = caf_extension
            .and_then(|extension| extension.get("publication_status_id"))
            .and_then(serde_json::Value::as_str)
            .map(str::to_string)
            .or_else(
                || match public.and_then(|metadata| metadata.publication_status.as_deref()) {
                    Some("published_art") => Some("1".to_string()),
                    Some("unpublished_art") => Some("2".to_string()),
                    _ => None,
                },
            );
        let update = MetadataUpdate {
            artwork_id,
            title: manifest.title.clone(),
            description: public.and_then(|metadata| metadata.description.clone()),
            for_sale_status: public.and_then(|metadata| metadata.for_sale_status.clone()),
            media_type_id: caf_extension
                .and_then(|extension| extension.get("media_type_id"))
                .and_then(serde_json::Value::as_str)
                .map(str::to_string)
                .or_else(|| {
                    public
                        .and_then(|metadata| metadata.media.as_deref())
                        .and_then(media_type_id_for_label)
                        .map(str::to_string)
                }),
            art_type_id: caf_extension
                .and_then(|extension| extension.get("art_type_id"))
                .and_then(serde_json::Value::as_str)
                .map(str::to_string)
                .or_else(|| {
                    public
                        .and_then(|metadata| metadata.artwork_type.as_deref())
                        .and_then(art_type_id_for_label)
                        .map(str::to_string)
                }),
            publication_status_id,
            active: public
                .and_then(|metadata| metadata.is_public)
                .unwrap_or(true),
            illustration_exchange: caf_extension
                .and_then(|extension| extension.get("illustration_exchange"))
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false),
            ix_for_sale: caf_extension
                .and_then(|extension| extension.get("ix_for_sale"))
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false),
            artist_credits: public
                .map(|metadata| {
                    metadata
                        .artist_credits
                        .iter()
                        .map(|credit| ArtistCreditUpdate {
                            first_name: credit.first_name.clone(),
                            last_name: credit.last_name.clone(),
                            role_id: credit
                                .role
                                .as_deref()
                                .and_then(artist_role_id_for_label)
                                .map(str::to_string),
                        })
                        .collect()
                })
                .unwrap_or_default(),
            media: public.and_then(|metadata| metadata.media.clone()),
            format: public.and_then(|metadata| metadata.artwork_type.clone()),
            caf_url: provider_url(&manifest.external_links, "com.comicartfans"),
            snikt_url: provider_url(&manifest.external_links, "com.snikt"),
            raremarq_url: provider_url(&manifest.external_links, "com.raremarq"),
            generic_url: app_extension_string(&manifest.extensions, "generic_url"),
            snikt_metadata: public_extensions
                .and_then(|extensions| extensions.get("com.snikt"))
                .and_then(|extension| extension.get("metadata"))
                .cloned()
                .map(serde_json::from_value)
                .transpose()?,
            purchase_price: private.and_then(|metadata| metadata.purchase_price.clone()),
            estimated_value: private.and_then(|metadata| metadata.estimated_value.clone()),
            purchase_date: private.and_then(|metadata| metadata.purchase_date.clone()),
            provenance: private.and_then(|metadata| metadata.provenance.clone()),
            personal_notes: private.and_then(|metadata| metadata.personal_notes.clone()),
        };
        self.save_metadata_session_only(update)?;
        self.update_caf_csv_tracking(
            artwork_id,
            caf_csv_image_link.as_deref(),
            caf_csv_added_to_caf.as_deref(),
        )?;
        self.update_snikt_csv_tracking(artwork_id, snikt_csv_created_date.as_deref())
    }

    fn save_artwork_external_links_from_manifest(
        &self,
        artwork_id: i64,
        links: &[ExternalLinkManifest],
    ) -> Result<()> {
        for link in links {
            let extensions_value = if link.extensions.is_empty() {
                None
            } else {
                Some(serde_json::Value::Object(
                    link.extensions.clone().into_iter().collect(),
                ))
            };
            self.upsert_artwork_external_link(
                artwork_id,
                oac_link_type_for_provider(&link.provider),
                Some(&link.id),
                &link.url,
                extensions_value.as_ref(),
            )?;
        }
        Ok(())
    }

    fn import_file_assets_from_artwork_manifest(
        &self,
        artwork_id: i64,
        manifest_path: &Path,
        manifest: &ArtworkManifest,
    ) -> Result<usize> {
        let artwork_folder = manifest_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();
        let mut source_file_ids_by_manifest_id: BTreeMap<String, i64> = BTreeMap::new();
        let mut imported = 0usize;
        for file in &manifest.files {
            let relative_path = PathBuf::from(
                file.relative_path
                    .replace('/', std::path::MAIN_SEPARATOR_STR),
            );
            if relative_path.is_absolute()
                || relative_path.components().any(|component| {
                    matches!(
                        component,
                        std::path::Component::ParentDir
                            | std::path::Component::Prefix(_)
                            | std::path::Component::RootDir
                    )
                })
            {
                continue;
            }
            let file_path = artwork_folder.join(relative_path);
            if !path_is_at_or_under(&artwork_folder, &file_path) || !file_path.exists() {
                continue;
            }
            if file.file_kind == "derivative" && file.width.is_some() && file.height.is_some() {
                let source_file_asset_id = app_extension_string(&file.extensions, "source_file_id")
                    .and_then(|source_file_id| {
                        source_file_ids_by_manifest_id.get(&source_file_id).copied()
                    });
                let derivative = self.add_derived_asset_session_only(
                    artwork_id,
                    DerivedAssetInsert {
                        source_file_asset_id,
                        derivative_type: app_extension_string(&file.extensions, "derivative_type")
                            .as_deref()
                            .unwrap_or("oaa_derivative"),
                        format: file
                            .format
                            .as_deref()
                            .or_else(|| file_path.extension().and_then(|value| value.to_str()))
                            .unwrap_or("file"),
                        path: &file_path,
                        width: file.width.unwrap_or_default(),
                        height: file.height.unwrap_or_default(),
                        image_role: file.image_role.as_deref(),
                    },
                )?;
                self.save_manifest_extension_blocks(
                    "derived_asset",
                    derivative.id,
                    &file.extensions,
                )?;
                imported += 1;
            } else {
                let file_asset_id = self.upsert_file_asset_with_known_metadata(
                    artwork_id,
                    FileAssetKnownMetadataInsert {
                        original_path: &file_path,
                        root: &artwork_folder,
                        path: &file_path,
                        is_primary: file.is_primary.unwrap_or(false),
                        source_kind: app_extension_string(&file.extensions, "source_kind")
                            .as_deref()
                            .unwrap_or("copied"),
                        metadata: FileAssetMetadata {
                            width: file.width,
                            height: file.height,
                            dpi_x: file.dpi_x,
                            dpi_y: file.dpi_y,
                        },
                    },
                )?;
                if let Some(image_role) = file.image_role.as_deref() {
                    self.update_file_asset_image_role_session_only(
                        file_asset_id,
                        Some(image_role),
                    )?;
                }
                for link in &file.external_links {
                    let extensions_value = if link.extensions.is_empty() {
                        None
                    } else {
                        Some(serde_json::Value::Object(
                            link.extensions.clone().into_iter().collect(),
                        ))
                    };
                    self.upsert_file_asset_external_link(
                        file_asset_id,
                        &link.provider,
                        &link.id,
                        &link.url,
                        extensions_value.as_ref(),
                    )?;
                }
                self.save_manifest_extension_blocks("file_asset", file_asset_id, &file.extensions)?;
                source_file_ids_by_manifest_id.insert(file.id.clone(), file_asset_id);
                imported += 1;
            }
        }
        Ok(imported)
    }

    fn save_manifest_extension_blocks(
        &self,
        owner_kind: &str,
        owner_id: i64,
        extensions: &BTreeMap<String, serde_json::Value>,
    ) -> Result<()> {
        for (provider, value) in extensions {
            self.save_oaa_extension_block(owner_kind, owner_id, provider, value)?;
        }
        Ok(())
    }

    pub fn ensure_collection_caf_collection_id(
        &self,
        collection_id: i64,
        caf_collection_id: &str,
    ) -> Result<CollectionSummary> {
        let caf_collection_id =
            normalize_caf_collection_id(Some(caf_collection_id), "CAF Collection ID")?
                .ok_or_else(|| AppError::Message("CAF Collection ID is required".to_string()))?;
        self.ensure_collection_provider_id(
            collection_id,
            "caf_collection_id",
            "CAF Collection",
            &caf_collection_id,
        )
    }

    pub fn set_collection_caf_collection_id(
        &self,
        collection_id: i64,
        caf_collection_id: &str,
    ) -> Result<CollectionSummary> {
        let caf_collection_id =
            normalize_caf_collection_id(Some(caf_collection_id), "CAF Collection ID")?
                .ok_or_else(|| AppError::Message("CAF Collection ID is required".to_string()))?;
        let now = Utc::now().to_rfc3339();
        {
            let conn = self.lock()?;
            conn.execute(
                "UPDATE collection SET caf_collection_id = ?1, updated_at = ?2 WHERE id = ?3",
                params![caf_collection_id, now, collection_id],
            )?;
        }
        self.rewrite_collection_manifest(collection_id)?;
        self.set_setting("active_workspace_mode", "collection")?;
        self.set_setting("active_collection_id", &collection_id.to_string())?;
        self.collection_summary(collection_id)
    }

    pub fn ensure_collection_snikt_collection_id(
        &self,
        collection_id: i64,
        snikt_collection_id: &str,
    ) -> Result<CollectionSummary> {
        let snikt_collection_id =
            normalize_snikt_collection_id(Some(snikt_collection_id), "SNIKT.com Collection ID")?
                .ok_or_else(|| {
                    AppError::Message("SNIKT.com Collection ID is required".to_string())
                })?;
        self.ensure_collection_provider_id(
            collection_id,
            "snikt_collection_id",
            "SNIKT.com Collection",
            &snikt_collection_id,
        )
    }

    pub fn ensure_collection_raremarq_collection_id(
        &self,
        collection_id: i64,
        raremarq_collection_id: &str,
    ) -> Result<CollectionSummary> {
        let raremarq_collection_id = normalize_raremarq_collection_id(
            Some(raremarq_collection_id),
            "Raremarq Collection ID",
        )?
        .ok_or_else(|| AppError::Message("Raremarq Collection ID is required".to_string()))?;
        self.ensure_collection_provider_id(
            collection_id,
            "raremarq_collection_id",
            "Raremarq Collection",
            &raremarq_collection_id,
        )
    }

    fn ensure_collection_provider_id(
        &self,
        collection_id: i64,
        column_name: &str,
        label: &str,
        provider_id: &str,
    ) -> Result<CollectionSummary> {
        let collection = self.collection_summary(collection_id)?;
        let existing = match column_name {
            "caf_collection_id" => collection.caf_collection_id.as_deref(),
            "snikt_collection_id" => collection.snikt_collection_id.as_deref(),
            "raremarq_collection_id" => collection.raremarq_collection_id.as_deref(),
            _ => {
                return Err(AppError::Message(format!(
                    "Unsupported Collection provider field: {column_name}"
                )))
            }
        };

        if let Some(existing) = existing {
            if existing != provider_id {
                return Err(AppError::Message(format!(
                    "Open Collection \"{}\" is already linked to {label} {existing}; close it before importing {label} {provider_id}.",
                    collection.name
                )));
            }
            self.set_setting("active_workspace_mode", "collection")?;
            self.set_setting("active_collection_id", &collection_id.to_string())?;
            return Ok(collection);
        }

        let now = Utc::now().to_rfc3339();
        {
            let conn = self.lock()?;
            match column_name {
                "caf_collection_id" => {
                    conn.execute(
                        "UPDATE collection SET caf_collection_id = ?1, updated_at = ?2 WHERE id = ?3",
                        params![provider_id, now, collection_id],
                    )?;
                }
                "snikt_collection_id" => {
                    conn.execute(
                        "UPDATE collection SET snikt_collection_id = ?1, updated_at = ?2 WHERE id = ?3",
                        params![provider_id, now, collection_id],
                    )?;
                }
                "raremarq_collection_id" => {
                    conn.execute(
                        "UPDATE collection SET raremarq_collection_id = ?1, updated_at = ?2 WHERE id = ?3",
                        params![provider_id, now, collection_id],
                    )?;
                }
                _ => unreachable!(),
            }
        }
        self.rewrite_collection_manifest(collection_id)?;
        self.set_setting("active_workspace_mode", "collection")?;
        self.set_setting("active_collection_id", &collection_id.to_string())?;
        self.collection_summary(collection_id)
    }

    pub fn save_collection_provider_ids(
        &self,
        collection_id: i64,
        caf_collection_id: Option<&str>,
        snikt_collection_id: Option<&str>,
        raremarq_collection_id: Option<&str>,
    ) -> Result<CollectionSummary> {
        let caf_collection_id =
            normalize_caf_collection_id(caf_collection_id, "CAF Collection ID")?;
        let snikt_collection_id =
            normalize_snikt_collection_id(snikt_collection_id, "SNIKT.com Collection ID")?;
        let raremarq_collection_id =
            normalize_raremarq_collection_id(raremarq_collection_id, "Raremarq Collection ID")?;
        let now = Utc::now().to_rfc3339();
        {
            let conn = self.lock()?;
            conn.execute(
                "UPDATE collection
                 SET caf_collection_id = ?1,
                     snikt_collection_id = ?2,
                     raremarq_collection_id = ?3,
                     updated_at = ?4
                 WHERE id = ?5",
                params![
                    caf_collection_id,
                    snikt_collection_id,
                    raremarq_collection_id,
                    now,
                    collection_id
                ],
            )?;
        }
        self.rewrite_collection_manifest(collection_id)?;
        self.set_setting("active_workspace_mode", "collection")?;
        self.set_setting("active_collection_id", &collection_id.to_string())?;
        self.collection_summary(collection_id)
    }

    pub fn create_gallery(&self, name: &str, manifest_path: &Path) -> Result<GallerySummary> {
        self.create_gallery_with_provider_ids(name, manifest_path, None, None, true)
    }

    pub fn create_gallery_with_caf_gallery_room_id(
        &self,
        name: &str,
        manifest_path: &Path,
        caf_gallery_room_id: Option<&str>,
    ) -> Result<GallerySummary> {
        self.create_gallery_with_provider_ids(name, manifest_path, caf_gallery_room_id, None, true)
    }

    pub fn create_gallery_with_provider_ids(
        &self,
        name: &str,
        manifest_path: &Path,
        caf_gallery_room_id: Option<&str>,
        raremarq_gallery_id: Option<&str>,
        snikt_gallery_inherits_collection: bool,
    ) -> Result<GallerySummary> {
        let now = Utc::now().to_rfc3339();
        let stable_id = stable_id("gallery");
        let name = normalized_name(name, "Untitled Gallery");
        let caf_gallery_room_id =
            normalize_caf_gallery_room_id(caf_gallery_room_id, "CAF Gallery Room ID")?;
        let raremarq_gallery_id =
            normalize_raremarq_gallery_id(raremarq_gallery_id, "Raremarq Gallery ID")?;
        let manifest_path =
            manifest_path_from_input(manifest_path, &name, "oagallery", "Untitled Gallery");
        let path_string = manifest_path.to_string_lossy().to_string();
        let id = {
            let conn = self.lock()?;
            conn.execute(
                "INSERT INTO gallery (stable_id, name, manifest_path, caf_gallery_room_id, snikt_gallery_inherits_collection, raremarq_gallery_id, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    stable_id,
                    name,
                    path_string,
                    caf_gallery_room_id,
                    snikt_gallery_inherits_collection,
                    raremarq_gallery_id,
                    now,
                    now
                ],
            )?;
            conn.last_insert_rowid()
        };
        let gallery = self.gallery_summary(id)?;
        self.rewrite_gallery_manifest(id)?;
        self.set_setting("active_workspace_mode", "loose")?;
        self.set_setting("active_gallery_id", &id.to_string())?;
        Ok(gallery)
    }

    pub fn open_gallery(&self, manifest_path: &Path) -> Result<GallerySummary> {
        self.clear_working_catalog()?;
        let collection = self.open_gallery_without_activating(manifest_path)?;
        self.set_setting("active_workspace_mode", "loose")?;
        self.set_setting("active_gallery_id", &collection.id.to_string())?;
        Ok(collection)
    }

    pub fn link_gallery_to_collection(&self, collection_id: i64, gallery_id: i64) -> Result<()> {
        self.link_gallery_to_collection_session_only(collection_id, gallery_id)?;
        self.set_setting("active_workspace_mode", "collection")?;
        self.set_setting("active_collection_id", &collection_id.to_string())?;
        self.set_setting("active_gallery_id", &gallery_id.to_string())?;
        self.rewrite_collection_manifest(collection_id)
    }

    fn link_gallery_to_collection_session_only(
        &self,
        collection_id: i64,
        gallery_id: i64,
    ) -> Result<()> {
        let sort_order = self.collection_gallery_count(collection_id)?;
        {
            let conn = self.lock()?;
            conn.execute(
                "INSERT OR IGNORE INTO collection_gallery (collection_id, gallery_id, sort_order)
                 VALUES (?1, ?2, ?3)",
                params![collection_id, gallery_id, sort_order],
            )?;
        }
        Ok(())
    }

    pub(crate) fn linked_caf_gallery_for_collection(
        &self,
        collection_id: i64,
        caf_gallery_room_id: &str,
        expected_manifest_path: &Path,
    ) -> Result<Option<GallerySummary>> {
        let path_string = expected_manifest_path.to_string_lossy().to_string();
        let conn = self.lock()?;
        if let Some(gallery) = conn
            .query_row(
                r#"
                SELECT g.id, g.stable_id, g.name, g.manifest_path, g.caf_gallery_room_id, g.snikt_gallery_id, g.snikt_gallery_inherits_collection, g.raremarq_gallery_id
                FROM gallery g
                INNER JOIN collection_gallery cg ON cg.gallery_id = g.id
                WHERE cg.collection_id = ?1 AND g.caf_gallery_room_id = ?2
                ORDER BY cg.sort_order, g.id
                LIMIT 1
                "#,
                params![collection_id, caf_gallery_room_id],
                gallery_from_row,
            )
            .optional()?
        {
            return Ok(Some(gallery));
        }

        conn.query_row(
            r#"
            SELECT g.id, g.stable_id, g.name, g.manifest_path, g.caf_gallery_room_id, g.snikt_gallery_id, g.snikt_gallery_inherits_collection, g.raremarq_gallery_id
            FROM gallery g
            INNER JOIN collection_gallery cg ON cg.gallery_id = g.id
            WHERE cg.collection_id = ?1 AND g.manifest_path = ?2
            ORDER BY cg.sort_order, g.id
            LIMIT 1
            "#,
            params![collection_id, path_string],
            gallery_from_row,
        )
        .optional()
        .map_err(AppError::from)
    }

    pub(crate) fn mark_gallery_as_caf_gallery(
        &self,
        gallery_id: i64,
        caf_gallery_room_id: &str,
    ) -> Result<GallerySummary> {
        let caf_gallery_room_id =
            normalize_caf_gallery_room_id(Some(caf_gallery_room_id), "CAF Gallery Room ID")?
                .ok_or_else(|| AppError::Message("CAF Gallery Room ID is required".to_string()))?;
        let now = Utc::now().to_rfc3339();
        {
            let conn = self.lock()?;
            conn.execute(
                "UPDATE gallery SET caf_gallery_room_id = ?1, updated_at = ?2 WHERE id = ?3",
                params![caf_gallery_room_id, now, gallery_id],
            )?;
        }
        self.rewrite_gallery_manifest(gallery_id)?;
        self.gallery_summary(gallery_id)
    }

    pub(crate) fn mark_gallery_as_snikt_gallery(
        &self,
        gallery_id: i64,
        snikt_gallery_id: &str,
    ) -> Result<GallerySummary> {
        let snikt_gallery_id =
            validate_external_text_id(Some(snikt_gallery_id), "SNIKT.com Gallery ID")?
                .ok_or_else(|| AppError::Message("SNIKT.com Gallery ID is required".to_string()))?;
        let now = Utc::now().to_rfc3339();
        {
            let conn = self.lock()?;
            conn.execute(
                "UPDATE gallery SET snikt_gallery_id = ?1, snikt_gallery_inherits_collection = 0, updated_at = ?2 WHERE id = ?3",
                params![snikt_gallery_id, now, gallery_id],
            )?;
        }
        self.rewrite_gallery_manifest(gallery_id)?;
        self.gallery_summary(gallery_id)
    }

    pub(crate) fn mark_gallery_as_raremarq_gallery(
        &self,
        gallery_id: i64,
        raremarq_gallery_id: &str,
    ) -> Result<GallerySummary> {
        let raremarq_gallery_id =
            normalize_raremarq_gallery_id(Some(raremarq_gallery_id), "Raremarq Gallery ID")?
                .ok_or_else(|| AppError::Message("Raremarq Gallery ID is required".to_string()))?;
        let now = Utc::now().to_rfc3339();
        {
            let conn = self.lock()?;
            conn.execute(
                "UPDATE gallery SET raremarq_gallery_id = ?1, updated_at = ?2 WHERE id = ?3",
                params![raremarq_gallery_id, now, gallery_id],
            )?;
        }
        self.rewrite_gallery_manifest(gallery_id)?;
        self.gallery_summary(gallery_id)
    }

    pub fn save_gallery_provider_ids(
        &self,
        gallery_id: i64,
        caf_gallery_room_id: Option<&str>,
        raremarq_gallery_id: Option<&str>,
        snikt_gallery_inherits_collection: Option<bool>,
    ) -> Result<GallerySummary> {
        let caf_gallery_room_id =
            normalize_caf_gallery_room_id(caf_gallery_room_id, "CAF Gallery Room ID")?;
        let raremarq_gallery_id =
            normalize_raremarq_gallery_id(raremarq_gallery_id, "Raremarq Gallery ID")?;
        let current = self.gallery_summary(gallery_id)?;
        let snikt_gallery_inherits_collection =
            snikt_gallery_inherits_collection.unwrap_or(current.snikt_gallery_inherits_collection);
        let snikt_gallery_id = if snikt_gallery_inherits_collection {
            None
        } else {
            current.snikt_gallery_id
        };
        let now = Utc::now().to_rfc3339();
        {
            let conn = self.lock()?;
            conn.execute(
                "UPDATE gallery
                 SET caf_gallery_room_id = ?1,
                     raremarq_gallery_id = ?2,
                     snikt_gallery_inherits_collection = ?3,
                     snikt_gallery_id = ?4,
                     updated_at = ?5
                 WHERE id = ?6",
                params![
                    caf_gallery_room_id,
                    raremarq_gallery_id,
                    snikt_gallery_inherits_collection,
                    snikt_gallery_id,
                    now,
                    gallery_id
                ],
            )?;
        }
        self.rewrite_gallery_manifest(gallery_id)?;
        self.gallery_summary(gallery_id)
    }

    pub fn merge_gallery_into(&self, request: GalleryMergeUpdate) -> Result<GallerySummary> {
        let collection_id = request.collection_id;
        let source_gallery_id = request.source_gallery_id;
        let target_gallery_id = request.target_gallery_id;
        let snikt_gallery_inherits_collection = request.snikt_gallery_inherits_collection;
        if source_gallery_id == target_gallery_id {
            return Err(AppError::Message(
                "Choose a different target Gallery to merge into".to_string(),
            ));
        }
        let name = validate_workspace_name(&request.name, "Gallery name")?;
        let caf_gallery_room_id = normalize_caf_gallery_room_id(
            request.caf_gallery_room_id.as_deref(),
            "CAF Gallery Room ID",
        )?;
        let raremarq_gallery_id = normalize_raremarq_gallery_id(
            request.raremarq_gallery_id.as_deref(),
            "Raremarq Gallery ID",
        )?;
        let _ = self.collection_summary(collection_id)?;
        let source_gallery = self.gallery_summary(source_gallery_id)?;
        let target_gallery = self.gallery_summary(target_gallery_id)?;
        let target_snikt_gallery_id = if snikt_gallery_inherits_collection {
            None
        } else {
            target_gallery.snikt_gallery_id.clone()
        };
        let now = Utc::now().to_rfc3339();
        let mut source_deleted = false;

        {
            let mut conn = self.lock()?;
            let tx = conn.transaction()?;
            for (gallery_id, label) in [
                (source_gallery_id, "Source Gallery"),
                (target_gallery_id, "Target Gallery"),
            ] {
                let linked_count: i64 = tx.query_row(
                    "SELECT COUNT(*) FROM collection_gallery WHERE collection_id = ?1 AND gallery_id = ?2",
                    params![collection_id, gallery_id],
                    |row| row.get(0),
                )?;
                if linked_count == 0 {
                    return Err(AppError::Message(format!(
                        "{label} is not in the selected Collection"
                    )));
                }
            }

            let source_artwork_ids = {
                let mut statement = tx.prepare(
                    "SELECT artwork_id FROM gallery_artwork WHERE gallery_id = ?1 ORDER BY sort_order, artwork_id",
                )?;
                let rows = statement.query_map(params![source_gallery_id], |row| row.get(0))?;
                rows.collect::<std::result::Result<Vec<i64>, _>>()?
            };
            let mut existing_target_artwork_ids = {
                let mut statement = tx.prepare(
                    "SELECT artwork_id FROM gallery_artwork WHERE gallery_id = ?1 ORDER BY sort_order, artwork_id",
                )?;
                let rows = statement.query_map(params![target_gallery_id], |row| row.get(0))?;
                rows.collect::<std::result::Result<BTreeSet<i64>, _>>()?
            };
            let mut next_sort_order: i64 = tx.query_row(
                "SELECT COALESCE(MAX(sort_order) + 1, 0) FROM gallery_artwork WHERE gallery_id = ?1",
                params![target_gallery_id],
                |row| row.get(0),
            )?;

            tx.execute(
                "UPDATE gallery
                 SET name = ?1,
                     caf_gallery_room_id = ?2,
                     raremarq_gallery_id = ?3,
                     snikt_gallery_inherits_collection = ?4,
                     snikt_gallery_id = ?5,
                     updated_at = ?6
                 WHERE id = ?7",
                params![
                    name,
                    caf_gallery_room_id,
                    raremarq_gallery_id,
                    snikt_gallery_inherits_collection,
                    target_snikt_gallery_id,
                    now,
                    target_gallery_id
                ],
            )?;

            for artwork_id in source_artwork_ids {
                if existing_target_artwork_ids.insert(artwork_id) {
                    tx.execute(
                        "INSERT INTO gallery_artwork (gallery_id, artwork_id, sort_order)
                         VALUES (?1, ?2, ?3)",
                        params![target_gallery_id, artwork_id, next_sort_order],
                    )?;
                    next_sort_order += 1;
                }
            }

            tx.execute(
                "DELETE FROM collection_gallery WHERE collection_id = ?1 AND gallery_id = ?2",
                params![collection_id, source_gallery_id],
            )?;
            let remaining_source_collections: i64 = tx.query_row(
                "SELECT COUNT(*) FROM collection_gallery WHERE gallery_id = ?1",
                params![source_gallery_id],
                |row| row.get(0),
            )?;
            if remaining_source_collections == 0 {
                tx.execute(
                    "DELETE FROM gallery_artwork WHERE gallery_id = ?1",
                    params![source_gallery_id],
                )?;
                tx.execute(
                    "DELETE FROM gallery WHERE id = ?1",
                    params![source_gallery_id],
                )?;
                source_deleted = true;
            }
            tx.commit()?;
        }

        self.set_setting("active_workspace_mode", "collection")?;
        self.set_setting("active_collection_id", &collection_id.to_string())?;
        self.set_setting("active_gallery_id", &target_gallery_id.to_string())?;
        self.rewrite_gallery_manifest(target_gallery_id)?;
        self.rewrite_collections_for_gallery(target_gallery_id)?;
        if source_deleted {
            self.remove_manifest_file_or_managed_folder_if_unreferenced(
                &source_gallery.manifest_path,
            )?;
        }
        Ok(GallerySummary {
            name,
            caf_gallery_room_id,
            snikt_gallery_id: target_snikt_gallery_id,
            snikt_gallery_inherits_collection,
            raremarq_gallery_id,
            ..target_gallery
        })
    }

    pub fn merge_artwork_into(&self, request: ArtworkMergeUpdate) -> Result<ArtworkDetail> {
        let collection_id = request.collection_id;
        let source_gallery_id = request.source_gallery_id;
        let source_artwork_id = request.source_artwork_id;
        let target_artwork_id = request.target_artwork_id;
        if source_artwork_id == target_artwork_id {
            return Err(AppError::Message(
                "Choose a different target Artwork to merge into".to_string(),
            ));
        }
        if request.metadata.artwork_id != target_artwork_id {
            return Err(AppError::Message(
                "Merged metadata must target the destination Artwork".to_string(),
            ));
        }
        let _ = self.collection_summary(collection_id)?;
        let _ = self.gallery_summary(source_gallery_id)?;
        let _ = self.artwork_summary(source_artwork_id)?;
        let _ = self.artwork_summary(target_artwork_id)?;
        let source_manifest_path = self.artwork_manifest_path(source_artwork_id)?;
        {
            let conn = self.lock()?;
            validate_artwork_merge_scope_locked(
                &conn,
                collection_id,
                source_gallery_id,
                source_artwork_id,
                target_artwork_id,
            )?;
        }

        self.save_metadata_session_only(request.metadata)?;

        let mut gallery_ids_to_rewrite = BTreeSet::new();
        {
            let mut conn = self.lock()?;
            let tx = conn.transaction()?;
            validate_artwork_merge_scope_locked(
                &tx,
                collection_id,
                source_gallery_id,
                source_artwork_id,
                target_artwork_id,
            )?;

            let source_gallery_ids = {
                let mut statement = tx.prepare(
                    "SELECT gallery_id FROM gallery_artwork WHERE artwork_id = ?1 ORDER BY sort_order, gallery_id",
                )?;
                let rows = statement.query_map(params![source_artwork_id], |row| row.get(0))?;
                rows.collect::<std::result::Result<Vec<i64>, _>>()?
            };
            let mut target_gallery_ids = {
                let mut statement = tx.prepare(
                    "SELECT gallery_id FROM gallery_artwork WHERE artwork_id = ?1 ORDER BY sort_order, gallery_id",
                )?;
                let rows = statement.query_map(params![target_artwork_id], |row| row.get(0))?;
                rows.collect::<std::result::Result<BTreeSet<i64>, _>>()?
            };
            for gallery_id in &target_gallery_ids {
                gallery_ids_to_rewrite.insert(*gallery_id);
            }
            for gallery_id in source_gallery_ids {
                gallery_ids_to_rewrite.insert(gallery_id);
                if target_gallery_ids.insert(gallery_id) {
                    let sort_order: i64 = tx.query_row(
                        "SELECT COALESCE(MAX(sort_order) + 1, 0) FROM gallery_artwork WHERE gallery_id = ?1",
                        params![gallery_id],
                        |row| row.get(0),
                    )?;
                    tx.execute(
                        "INSERT INTO gallery_artwork (gallery_id, artwork_id, sort_order)
                         VALUES (?1, ?2, ?3)",
                        params![gallery_id, target_artwork_id, sort_order],
                    )?;
                }
            }

            let first_appended_file_order: i64 = tx.query_row(
                "SELECT COALESCE(MAX(display_order) + 1, 0) FROM file_asset WHERE artwork_id = ?1",
                params![target_artwork_id],
                |row| row.get(0),
            )?;
            let target_has_primary_file: bool = tx.query_row(
                "SELECT COUNT(*) FROM file_asset WHERE artwork_id = ?1 AND is_primary = 1",
                params![target_artwork_id],
                |row| row.get::<_, i64>(0),
            )? > 0;
            let source_file_asset_ids = {
                let mut statement = tx.prepare(
                    "SELECT id FROM file_asset WHERE artwork_id = ?1 ORDER BY display_order, is_primary DESC, id",
                )?;
                let rows = statement.query_map(params![source_artwork_id], |row| row.get(0))?;
                rows.collect::<std::result::Result<Vec<i64>, _>>()?
            };
            for (index, file_asset_id) in source_file_asset_ids.iter().enumerate() {
                let is_primary = !target_has_primary_file && index == 0;
                let display_order = first_appended_file_order + index as i64;
                tx.execute(
                    "UPDATE file_asset
                     SET artwork_id = ?1, display_order = ?2, is_primary = ?3
                     WHERE id = ?4",
                    params![
                        target_artwork_id,
                        display_order,
                        if is_primary { 1 } else { 0 },
                        file_asset_id
                    ],
                )?;
            }
            tx.execute(
                "UPDATE derived_asset SET artwork_id = ?1 WHERE artwork_id = ?2",
                params![target_artwork_id, source_artwork_id],
            )?;
            tx.execute(
                "UPDATE file_operation_log SET artwork_id = ?1 WHERE artwork_id = ?2",
                params![target_artwork_id, source_artwork_id],
            )?;
            tx.execute(
                "DELETE FROM gallery_artwork WHERE artwork_id = ?1",
                params![source_artwork_id],
            )?;
            tx.execute(
                "DELETE FROM oaa_extension_block
                 WHERE owner_id = ?1
                   AND owner_kind IN ('artwork', 'artwork_public_metadata', 'artwork_private_metadata')",
                params![source_artwork_id],
            )?;
            tx.execute(
                "DELETE FROM artwork WHERE id = ?1",
                params![source_artwork_id],
            )?;
            tx.commit()?;
        }

        self.set_setting("active_workspace_mode", "collection")?;
        self.set_setting("active_collection_id", &collection_id.to_string())?;
        self.set_setting("active_gallery_id", &source_gallery_id.to_string())?;
        self.ensure_artwork_manifest(target_artwork_id)?;
        for gallery_id in gallery_ids_to_rewrite {
            self.rewrite_gallery_manifest(gallery_id)?;
            self.rewrite_collections_for_gallery(gallery_id)?;
        }
        if let Some(source_manifest_path) = source_manifest_path {
            self.remove_manifest_file_or_managed_folder_if_unreferenced(&source_manifest_path)?;
        }
        self.artwork_detail(target_artwork_id)
    }

    pub fn link_artwork_to_gallery(&self, gallery_id: i64, artwork_id: i64) -> Result<()> {
        self.link_artwork_to_gallery_with_manifest_rewrite(gallery_id, artwork_id, true)
    }

    fn link_artwork_to_gallery_session_only(&self, gallery_id: i64, artwork_id: i64) -> Result<()> {
        let sort_order = self.gallery_artwork_count(gallery_id)?;
        let conn = self.lock()?;
        conn.execute(
            "INSERT OR IGNORE INTO gallery_artwork (gallery_id, artwork_id, sort_order)
             VALUES (?1, ?2, ?3)",
            params![gallery_id, artwork_id, sort_order],
        )?;
        Ok(())
    }

    fn link_artwork_to_gallery_with_manifest_rewrite(
        &self,
        gallery_id: i64,
        artwork_id: i64,
        rewrite_gallery_manifest: bool,
    ) -> Result<()> {
        let sort_order = self.gallery_artwork_count(gallery_id)?;
        {
            let conn = self.lock()?;
            conn.execute(
                "INSERT OR IGNORE INTO gallery_artwork (gallery_id, artwork_id, sort_order)
                 VALUES (?1, ?2, ?3)",
                params![gallery_id, artwork_id, sort_order],
            )?;
        }
        self.set_setting("active_gallery_id", &gallery_id.to_string())?;
        self.ensure_artwork_manifest(artwork_id)?;
        if rewrite_gallery_manifest {
            self.rewrite_gallery_manifest(gallery_id)?;
            self.rewrite_collections_for_gallery(gallery_id)
        } else {
            Ok(())
        }
    }

    pub fn delete_collection_preview(&self, collection_id: i64) -> Result<DeletePreview> {
        let mut preview = DeletePreview::default();
        let deleting_galleries = self.deleting_gallery_ids_for_collection(collection_id)?;
        self.add_delete_preview_for_gallery_set(&deleting_galleries, &mut preview)?;
        Ok(preview)
    }

    pub fn delete_gallery_preview(
        &self,
        gallery_id: i64,
        collection_id: Option<i64>,
    ) -> Result<DeletePreview> {
        let mut preview = DeletePreview::default();
        let deleting_galleries =
            self.deleting_gallery_ids_for_gallery(gallery_id, collection_id)?;
        self.add_delete_preview_for_gallery_set(&deleting_galleries, &mut preview)?;
        Ok(preview)
    }

    pub fn delete_artwork_preview(&self, artwork_id: i64) -> Result<DeletePreview> {
        let mut preview = DeletePreview::default();
        self.add_artwork_delete_preview(artwork_id, &mut preview)?;
        Ok(preview)
    }

    pub fn delete_artwork_from_gallery_preview(
        &self,
        artwork_id: i64,
        gallery_id: Option<i64>,
    ) -> Result<DeletePreview> {
        let mut preview = DeletePreview::default();
        if let Some(gallery_id) = gallery_id {
            let gallery_ids = self
                .galleries_for_artwork(artwork_id)?
                .into_iter()
                .map(|gallery| gallery.id)
                .collect::<Vec<_>>();
            if gallery_ids.iter().any(|id| *id != gallery_id) {
                return Ok(preview);
            }
        }
        self.add_artwork_delete_preview(artwork_id, &mut preview)?;
        Ok(preview)
    }

    pub fn delete_artwork_file_item_preview(
        &self,
        asset_kind: AssetKind,
        asset_id: i64,
    ) -> Result<DeletePreview> {
        let mut preview = DeletePreview::default();
        match asset_kind {
            AssetKind::File => {
                let asset = self.file_asset(asset_id)?;
                if let Some(candidate) = self.delete_candidate_for_file_asset(&asset)? {
                    preview.files_to_trash.push(candidate);
                }
            }
            AssetKind::Derived => {
                let asset = self.derived_asset(asset_id)?;
                if let Some(candidate) = delete_candidate_for_derived_asset(&asset) {
                    preview.files_to_trash.push(candidate);
                }
            }
        }
        Ok(preview)
    }

    fn deleting_gallery_ids_for_collection(&self, collection_id: i64) -> Result<BTreeSet<i64>> {
        let mut deleting_galleries = BTreeSet::new();
        for gallery in self.galleries_for_collection(collection_id)? {
            let collection_ids = self
                .collections_for_gallery(gallery.id)?
                .into_iter()
                .map(|collection| collection.id)
                .collect::<Vec<_>>();
            if collection_ids.iter().all(|id| *id == collection_id) {
                deleting_galleries.insert(gallery.id);
            }
        }
        Ok(deleting_galleries)
    }

    fn deleting_gallery_ids_for_gallery(
        &self,
        gallery_id: i64,
        collection_id: Option<i64>,
    ) -> Result<BTreeSet<i64>> {
        let mut deleting_galleries = BTreeSet::new();
        if let Some(collection_id) = collection_id {
            let collection_ids = self
                .collections_for_gallery(gallery_id)?
                .into_iter()
                .map(|collection| collection.id)
                .collect::<Vec<_>>();
            if collection_ids.iter().any(|id| *id != collection_id) {
                return Ok(deleting_galleries);
            }
        }
        deleting_galleries.insert(gallery_id);
        Ok(deleting_galleries)
    }

    fn add_delete_preview_for_gallery_set(
        &self,
        deleting_galleries: &BTreeSet<i64>,
        preview: &mut DeletePreview,
    ) -> Result<()> {
        let artwork_ids = self.deleting_artwork_ids_for_gallery_set(deleting_galleries)?;
        for artwork_id in artwork_ids {
            self.add_artwork_delete_preview(artwork_id, preview)?;
        }
        Ok(())
    }

    fn deleting_artwork_ids_for_gallery_set(
        &self,
        deleting_galleries: &BTreeSet<i64>,
    ) -> Result<BTreeSet<i64>> {
        let mut artwork_ids = BTreeSet::new();
        for gallery_id in deleting_galleries {
            for artwork in self.artworks_for_gallery(*gallery_id)? {
                let artwork_gallery_ids = self
                    .galleries_for_artwork(artwork.id)?
                    .into_iter()
                    .map(|gallery| gallery.id)
                    .collect::<Vec<_>>();
                if artwork_gallery_ids
                    .iter()
                    .all(|gallery_id| deleting_galleries.contains(gallery_id))
                {
                    artwork_ids.insert(artwork.id);
                }
            }
        }
        Ok(artwork_ids)
    }

    fn bulk_trash_root_for_collection(
        &self,
        manifest_path: &Path,
        collection_id: i64,
        deleting_galleries: &BTreeSet<i64>,
        deleting_artworks: &BTreeSet<i64>,
    ) -> Result<Option<PathBuf>> {
        let Some(root) = managed_container_folder_for_manifest(manifest_path) else {
            return Ok(None);
        };
        if self.catalog_has_surviving_path_under_deleted_collection_root(
            &root,
            collection_id,
            deleting_galleries,
            deleting_artworks,
        )? {
            return Ok(None);
        }
        Ok(Some(root))
    }

    fn catalog_has_surviving_path_under_deleted_collection_root(
        &self,
        root: &Path,
        collection_id: i64,
        deleting_galleries: &BTreeSet<i64>,
        deleting_artworks: &BTreeSet<i64>,
    ) -> Result<bool> {
        let conn = self.lock()?;

        let mut collections = conn.prepare("SELECT id, manifest_path FROM collection")?;
        let mut rows = collections.query([])?;
        while let Some(row) = rows.next()? {
            let id: i64 = row.get(0)?;
            let path: String = row.get(1)?;
            if path_is_at_or_under(root, Path::new(&path)) && id != collection_id {
                return Ok(true);
            }
        }

        let mut galleries = conn.prepare("SELECT id, manifest_path FROM gallery")?;
        let mut rows = galleries.query([])?;
        while let Some(row) = rows.next()? {
            let id: i64 = row.get(0)?;
            let path: String = row.get(1)?;
            if path_is_at_or_under(root, Path::new(&path)) && !deleting_galleries.contains(&id) {
                return Ok(true);
            }
        }

        let mut artworks = conn.prepare(
            "SELECT id, artwork_manifest_path FROM artwork WHERE artwork_manifest_path IS NOT NULL",
        )?;
        let mut rows = artworks.query([])?;
        while let Some(row) = rows.next()? {
            let id: i64 = row.get(0)?;
            let path: String = row.get(1)?;
            if path_is_at_or_under(root, Path::new(&path)) && !deleting_artworks.contains(&id) {
                return Ok(true);
            }
        }

        let mut file_assets = conn.prepare(
            "SELECT artwork_id, original_path, current_path, source_kind FROM file_asset",
        )?;
        let mut rows = file_assets.query([])?;
        while let Some(row) = rows.next()? {
            let artwork_id: i64 = row.get(0)?;
            let original_path: String = row.get(1)?;
            let current_path: String = row.get(2)?;
            let source_kind: String = row.get(3)?;
            let original_under_root = path_is_at_or_under(root, Path::new(&original_path));
            let current_under_root = path_is_at_or_under(root, Path::new(&current_path));
            if (original_under_root || current_under_root)
                && (!deleting_artworks.contains(&artwork_id) || source_kind == "linked")
            {
                return Ok(true);
            }
        }

        let mut derived_assets = conn.prepare("SELECT artwork_id, path FROM derived_asset")?;
        let mut rows = derived_assets.query([])?;
        while let Some(row) = rows.next()? {
            let artwork_id: i64 = row.get(0)?;
            let path: String = row.get(1)?;
            if path_is_at_or_under(root, Path::new(&path))
                && !deleting_artworks.contains(&artwork_id)
            {
                return Ok(true);
            }
        }

        Ok(false)
    }

    fn add_artwork_delete_preview(
        &self,
        artwork_id: i64,
        preview: &mut DeletePreview,
    ) -> Result<()> {
        let detail = self.artwork_detail(artwork_id)?;
        for asset in &detail.file_assets {
            if let Some(candidate) = self.delete_candidate_for_file_asset(asset)? {
                push_unique_delete_candidate(&mut preview.files_to_trash, candidate);
            }
        }
        for asset in &detail.derived_assets {
            if let Some(candidate) = delete_candidate_for_derived_asset(asset) {
                push_unique_delete_candidate(&mut preview.files_to_trash, candidate);
            }
        }
        Ok(())
    }

    pub fn delete_collection(&self, collection_id: i64) -> Result<DeleteResult> {
        self.delete_collection_with_trash(collection_id, move_path_to_trash)
    }

    pub fn delete_collection_with_trash<F>(
        &self,
        collection_id: i64,
        mut trash_file: F,
    ) -> Result<DeleteResult>
    where
        F: FnMut(&Path) -> std::result::Result<(), String>,
    {
        let mut result = DeleteResult::default();
        let pretrash_preview = self.delete_collection_pretrash_preview(collection_id)?;
        pretrash_managed_files_or_abort(&pretrash_preview, &mut trash_file, &mut result);
        if !result.trash_failures.is_empty() {
            return Ok(result);
        }
        self.delete_collection_internal(collection_id, &mut trash_file, &mut result)?;
        Ok(result)
    }

    fn delete_collection_pretrash_preview(&self, collection_id: i64) -> Result<DeletePreview> {
        let collection = self.collection_summary(collection_id)?;
        let deleting_galleries = self.deleting_gallery_ids_for_collection(collection_id)?;
        let deleting_artworks = self.deleting_artwork_ids_for_gallery_set(&deleting_galleries)?;
        if let Some(root) = self.bulk_trash_root_for_collection(
            &collection.manifest_path,
            collection_id,
            &deleting_galleries,
            &deleting_artworks,
        )? {
            return Ok(DeletePreview {
                files_to_trash: vec![DeleteFilePreview {
                    path: root,
                    label: "Collection folder".to_string(),
                    reason: "Self-contained OAC-managed collection folder".to_string(),
                }],
            });
        }
        self.delete_collection_preview(collection_id)
    }

    fn delete_collection_internal<F>(
        &self,
        collection_id: i64,
        trash_file: &mut F,
        result: &mut DeleteResult,
    ) -> Result<()>
    where
        F: FnMut(&Path) -> std::result::Result<(), String>,
    {
        let collection = self.collection_summary(collection_id)?;
        let deleting_galleries = self.deleting_gallery_ids_for_collection(collection_id)?;
        let deleting_artworks = self.deleting_artwork_ids_for_gallery_set(&deleting_galleries)?;
        let bulk_trash_root = self.bulk_trash_root_for_collection(
            &collection.manifest_path,
            collection_id,
            &deleting_galleries,
            &deleting_artworks,
        )?;
        let gallery_ids = self
            .galleries_for_collection(collection_id)?
            .into_iter()
            .map(|gallery| gallery.id)
            .collect::<Vec<_>>();
        for gallery_id in gallery_ids {
            self.delete_gallery_from_collection_internal(
                collection_id,
                gallery_id,
                trash_file,
                result,
                bulk_trash_root.as_deref(),
            )?;
        }
        {
            let conn = self.lock()?;
            conn.execute(
                "DELETE FROM collection WHERE id = ?1",
                params![collection_id],
            )?;
        }
        if self.setting("active_collection_id")? == Some(collection_id.to_string()) {
            self.set_setting("active_workspace_mode", "none")?;
            self.set_setting("active_collection_id", "")?;
            self.set_setting("active_gallery_id", "")?;
        }
        if let Some(root) = bulk_trash_root {
            trash_file_if_exists(&root, trash_file, result);
        } else {
            self.remove_manifest_file_or_managed_folder_if_unreferenced(&collection.manifest_path)?;
        }
        Ok(())
    }

    pub fn delete_gallery_from_collection(
        &self,
        collection_id: i64,
        gallery_id: i64,
    ) -> Result<DeleteResult> {
        self.delete_gallery_from_collection_with_trash(
            collection_id,
            gallery_id,
            move_path_to_trash,
        )
    }

    pub fn delete_gallery_from_collection_with_trash<F>(
        &self,
        collection_id: i64,
        gallery_id: i64,
        mut trash_file: F,
    ) -> Result<DeleteResult>
    where
        F: FnMut(&Path) -> std::result::Result<(), String>,
    {
        let mut result = DeleteResult::default();
        let pretrash_preview = self.delete_gallery_preview(gallery_id, Some(collection_id))?;
        pretrash_managed_files_or_abort(&pretrash_preview, &mut trash_file, &mut result);
        if !result.trash_failures.is_empty() {
            return Ok(result);
        }
        self.delete_gallery_from_collection_internal(
            collection_id,
            gallery_id,
            &mut trash_file,
            &mut result,
            None,
        )?;
        Ok(result)
    }

    fn delete_gallery_from_collection_internal<F>(
        &self,
        collection_id: i64,
        gallery_id: i64,
        trash_file: &mut F,
        result: &mut DeleteResult,
        bulk_trash_root: Option<&Path>,
    ) -> Result<()>
    where
        F: FnMut(&Path) -> std::result::Result<(), String>,
    {
        let _ = self.collection_summary(collection_id)?;
        let _ = self.gallery_summary(gallery_id)?;
        {
            let conn = self.lock()?;
            conn.execute(
                "DELETE FROM collection_gallery WHERE collection_id = ?1 AND gallery_id = ?2",
                params![collection_id, gallery_id],
            )?;
        }
        if self.collections_for_gallery(gallery_id)?.is_empty() {
            self.delete_gallery_internal(gallery_id, trash_file, result, bulk_trash_root)?;
        } else {
            self.rewrite_collection_manifest(collection_id)?;
        }
        if self.setting("active_gallery_id")? == Some(gallery_id.to_string())
            && self.collections_for_gallery(gallery_id)?.is_empty()
        {
            self.set_setting("active_gallery_id", "")?;
        }
        Ok(())
    }

    pub fn delete_gallery(&self, gallery_id: i64) -> Result<DeleteResult> {
        self.delete_gallery_with_trash(gallery_id, move_path_to_trash)
    }

    pub fn delete_gallery_with_trash<F>(
        &self,
        gallery_id: i64,
        mut trash_file: F,
    ) -> Result<DeleteResult>
    where
        F: FnMut(&Path) -> std::result::Result<(), String>,
    {
        let mut result = DeleteResult::default();
        let pretrash_preview = self.delete_gallery_preview(gallery_id, None)?;
        pretrash_managed_files_or_abort(&pretrash_preview, &mut trash_file, &mut result);
        if !result.trash_failures.is_empty() {
            return Ok(result);
        }
        self.delete_gallery_internal(gallery_id, &mut trash_file, &mut result, None)?;
        Ok(result)
    }

    fn delete_gallery_internal<F>(
        &self,
        gallery_id: i64,
        trash_file: &mut F,
        result: &mut DeleteResult,
        bulk_trash_root: Option<&Path>,
    ) -> Result<()>
    where
        F: FnMut(&Path) -> std::result::Result<(), String>,
    {
        let gallery = self.gallery_summary(gallery_id)?;
        let collections = self.collections_for_gallery(gallery_id)?;
        let artwork_ids = self
            .artworks_for_gallery(gallery_id)?
            .into_iter()
            .map(|artwork| artwork.id)
            .collect::<Vec<_>>();
        for artwork_id in artwork_ids {
            self.delete_artwork_from_gallery_internal(
                gallery_id,
                artwork_id,
                trash_file,
                result,
                false,
                bulk_trash_root,
            )?;
        }
        {
            let conn = self.lock()?;
            conn.execute("DELETE FROM gallery WHERE id = ?1", params![gallery_id])?;
        }
        for collection in collections {
            self.rewrite_collection_manifest(collection.id)?;
        }
        if self.setting("active_gallery_id")? == Some(gallery_id.to_string()) {
            self.set_setting("active_gallery_id", "")?;
        }
        self.remove_manifest_file_or_managed_folder_if_unreferenced(&gallery.manifest_path)?;
        Ok(())
    }

    pub fn delete_artwork_from_gallery(
        &self,
        gallery_id: i64,
        artwork_id: i64,
    ) -> Result<DeleteResult> {
        self.delete_artwork_from_gallery_with_trash(gallery_id, artwork_id, move_path_to_trash)
    }

    pub fn delete_artwork_from_gallery_with_trash<F>(
        &self,
        gallery_id: i64,
        artwork_id: i64,
        mut trash_file: F,
    ) -> Result<DeleteResult>
    where
        F: FnMut(&Path) -> std::result::Result<(), String>,
    {
        let mut result = DeleteResult::default();
        let pretrash_preview =
            self.delete_artwork_from_gallery_preview(artwork_id, Some(gallery_id))?;
        pretrash_managed_files_or_abort(&pretrash_preview, &mut trash_file, &mut result);
        if !result.trash_failures.is_empty() {
            return Ok(result);
        }
        self.delete_artwork_from_gallery_internal(
            gallery_id,
            artwork_id,
            &mut trash_file,
            &mut result,
            true,
            None,
        )?;
        Ok(result)
    }

    fn delete_artwork_from_gallery_internal<F>(
        &self,
        gallery_id: i64,
        artwork_id: i64,
        trash_file: &mut F,
        result: &mut DeleteResult,
        rewrite_gallery_manifest: bool,
        bulk_trash_root: Option<&Path>,
    ) -> Result<()>
    where
        F: FnMut(&Path) -> std::result::Result<(), String>,
    {
        let _ = self.gallery_summary(gallery_id)?;
        let _ = self.artwork_summary(artwork_id)?;
        {
            let conn = self.lock()?;
            conn.execute(
                "DELETE FROM gallery_artwork WHERE gallery_id = ?1 AND artwork_id = ?2",
                params![gallery_id, artwork_id],
            )?;
        }
        if rewrite_gallery_manifest {
            self.rewrite_gallery_manifest(gallery_id)?;
            self.rewrite_collections_for_gallery(gallery_id)?;
        }
        if self.galleries_for_artwork(artwork_id)?.is_empty() {
            self.delete_artwork_internal(artwork_id, trash_file, result, bulk_trash_root)?;
        }
        Ok(())
    }

    pub fn delete_artwork(&self, artwork_id: i64) -> Result<DeleteResult> {
        self.delete_artwork_with_trash(artwork_id, move_path_to_trash)
    }

    pub fn delete_artwork_with_trash<F>(
        &self,
        artwork_id: i64,
        mut trash_file: F,
    ) -> Result<DeleteResult>
    where
        F: FnMut(&Path) -> std::result::Result<(), String>,
    {
        let mut result = DeleteResult::default();
        let pretrash_preview = self.delete_artwork_preview(artwork_id)?;
        pretrash_managed_files_or_abort(&pretrash_preview, &mut trash_file, &mut result);
        if !result.trash_failures.is_empty() {
            return Ok(result);
        }
        self.delete_artwork_internal(artwork_id, &mut trash_file, &mut result, None)?;
        Ok(result)
    }

    fn delete_artwork_internal<F>(
        &self,
        artwork_id: i64,
        trash_file: &mut F,
        result: &mut DeleteResult,
        bulk_trash_root: Option<&Path>,
    ) -> Result<()>
    where
        F: FnMut(&Path) -> std::result::Result<(), String>,
    {
        let detail = self.artwork_detail(artwork_id)?;
        let manifest_path = self.artwork_manifest_path(artwork_id)?;
        let asset_folder = self.artwork_asset_folder(artwork_id).ok();
        let mut file_candidates = Vec::new();
        for asset in &detail.file_assets {
            if let Some(candidate) = self.delete_candidate_for_file_asset(asset)? {
                file_candidates.push(candidate);
            }
        }
        let export_candidates = detail
            .derived_assets
            .iter()
            .filter_map(delete_candidate_for_derived_asset)
            .collect::<Vec<_>>();
        let cache_paths = detail
            .derived_assets
            .iter()
            .filter(|asset| asset.derivative_type != "png_export")
            .map(|asset| asset.path.clone())
            .collect::<Vec<_>>();
        let galleries = self.galleries_for_artwork(artwork_id)?;
        {
            let conn = self.lock()?;
            conn.execute("DELETE FROM artwork WHERE id = ?1", params![artwork_id])?;
        }
        for gallery in galleries {
            self.rewrite_gallery_manifest(gallery.id)?;
            self.rewrite_collections_for_gallery(gallery.id)?;
        }
        for candidate in file_candidates.into_iter().chain(export_candidates) {
            if !path_is_covered_by_bulk_trash(bulk_trash_root, &candidate.path) {
                trash_file_if_exists(&candidate.path, trash_file, result);
            }
        }
        for path in cache_paths {
            if !path_is_covered_by_bulk_trash(bulk_trash_root, &path) {
                remove_file_if_exists(&path)?;
            }
        }
        if let Some(manifest_path) = manifest_path {
            if !path_is_covered_by_bulk_trash(bulk_trash_root, &manifest_path) {
                remove_file_if_exists(&manifest_path)?;
                if let Some(parent) = manifest_path.parent() {
                    remove_empty_dir_if_exists(parent)?;
                }
            }
        }
        if let Some(asset_folder) = &asset_folder {
            if !path_is_covered_by_bulk_trash(bulk_trash_root, asset_folder) {
                remove_empty_dir_if_exists(asset_folder)?;
            }
        }
        Ok(())
    }

    fn remove_manifest_file_or_managed_folder_if_unreferenced(
        &self,
        manifest_path: &Path,
    ) -> Result<()> {
        if let Some(folder) = managed_container_folder_for_manifest(manifest_path) {
            if !self.catalog_has_path_under(&folder)? {
                remove_file_if_exists(manifest_path)?;
                remove_empty_dir_tree_if_exists(&folder)?;
                return Ok(());
            }
        }
        remove_file_if_exists(manifest_path)
    }

    fn catalog_has_path_under(&self, root: &Path) -> Result<bool> {
        let conn = self.lock()?;
        for sql in [
            "SELECT manifest_path FROM collection",
            "SELECT manifest_path FROM gallery",
            "SELECT artwork_manifest_path FROM artwork WHERE artwork_manifest_path IS NOT NULL",
            "SELECT original_path FROM file_asset",
            "SELECT current_path FROM file_asset",
            "SELECT path FROM derived_asset",
        ] {
            let mut statement = conn.prepare(sql)?;
            let mut rows = statement.query([])?;
            while let Some(row) = rows.next()? {
                let path: String = row.get(0)?;
                if path_is_at_or_under(root, Path::new(&path)) {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }

    pub fn assign_artwork_manifest_path_for_gallery(
        &self,
        artwork_id: i64,
        gallery_id: i64,
    ) -> Result<()> {
        if self.artwork_manifest_path(artwork_id)?.is_some() {
            return Ok(());
        }
        let gallery = self.gallery_summary(gallery_id)?;
        let canonical_id = {
            let conn = self.lock()?;
            conn.query_row(
                "SELECT canonical_id FROM artwork WHERE id = ?1",
                params![artwork_id],
                |row| row.get::<_, String>(0),
            )?
        };
        let manifest_path = default_artwork_manifest_path(&gallery.manifest_path, &canonical_id);
        let path_string = manifest_path.to_string_lossy().to_string();
        let conn = self.lock()?;
        conn.execute(
            "UPDATE artwork SET artwork_stable_id = COALESCE(artwork_stable_id, canonical_id),
             artwork_manifest_path = ?1 WHERE id = ?2",
            params![path_string, artwork_id],
        )?;
        Ok(())
    }

    pub fn create_artwork_in_gallery(
        &self,
        gallery_id: i64,
        title: &str,
        manifest_path: Option<&Path>,
    ) -> Result<ArtworkSummary> {
        let gallery = self.gallery_summary(gallery_id)?;
        let title = normalized_name(title, "Untitled Artwork");
        let now = Utc::now().to_rfc3339();
        let id = {
            let conn = self.lock()?;
            cleanup_unlinked_artworks_locked(&conn)?;
            let canonical_id = next_canonical_id_locked(&conn)?;
            let path = manifest_path.map(Path::to_path_buf).unwrap_or_else(|| {
                default_artwork_manifest_path(&gallery.manifest_path, &canonical_id)
            });
            let path_string = path.to_string_lossy().to_string();
            conn.execute(
                "INSERT INTO artwork
                 (canonical_id, artwork_stable_id, title, source_folder, source_context, artwork_manifest_path, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    canonical_id,
                    canonical_id,
                    title,
                    path_string,
                    gallery.name,
                    path_string,
                    now,
                    now
                ],
            )?;
            conn.last_insert_rowid()
        };
        self.link_artwork_to_gallery(gallery_id, id)?;
        self.artwork_summary(id)
    }

    pub fn import_caf_artwork_in_gallery(
        &self,
        gallery_id: i64,
        imported: ImportedCafArtwork,
    ) -> Result<ArtworkSummary> {
        let gallery = self.gallery_summary(gallery_id)?;
        let piece_id = validate_caf_piece_id(&imported.piece_id)?;
        let title = normalized_name(&imported.title, "Untitled Artwork");
        let now = Utc::now().to_rfc3339();
        let artwork_id = {
            let conn = self.lock()?;
            if let Some(existing_id) = artwork_id_for_external_id_locked(&conn, "caf", &piece_id)? {
                conn.execute(
                    "UPDATE artwork SET title = ?1, updated_at = ?2 WHERE id = ?3",
                    params![title, now, existing_id],
                )?;
                existing_id
            } else if let Some(existing_id) = imported
                .primary_image_url
                .as_deref()
                .map(|url| artwork_id_for_external_id_locked(&conn, "caf_image", url))
                .transpose()?
                .flatten()
            {
                conn.execute(
                    "UPDATE artwork SET title = ?1, updated_at = ?2 WHERE id = ?3",
                    params![title, now, existing_id],
                )?;
                existing_id
            } else if let Some(existing_id) = conn
                .query_row(
                    "SELECT id FROM artwork WHERE canonical_id = ?1",
                    params![format!("CAF-{piece_id}")],
                    |row| row.get::<_, i64>(0),
                )
                .optional()?
            {
                conn.execute(
                    "UPDATE artwork SET
                       title = ?1,
                       updated_at = ?2
                     WHERE id = ?3",
                    params![title, now, existing_id],
                )?;
                existing_id
            } else {
                let canonical_id = next_canonical_id_locked(&conn)?;
                let manifest_path =
                    default_artwork_manifest_path(&gallery.manifest_path, &canonical_id);
                let path_string = manifest_path.to_string_lossy().to_string();
                conn.execute(
                    "INSERT INTO artwork
                     (canonical_id, artwork_stable_id, title, source_folder, source_context, artwork_manifest_path, created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                    params![
                        canonical_id,
                        canonical_id,
                        title,
                        path_string,
                        gallery.name,
                        path_string,
                        now,
                        now
                    ],
                )?;
                conn.last_insert_rowid()
            }
        };
        {
            let conn = self.lock()?;
            upsert_external_link_locked(
                &conn,
                artwork_id,
                "caf",
                Some(piece_id.as_str()),
                &imported.caf_url,
            )?;
            if let Some(primary_image_url) = imported.primary_image_url.as_deref() {
                upsert_external_link_locked(
                    &conn,
                    artwork_id,
                    "caf_image",
                    Some(primary_image_url),
                    primary_image_url,
                )?;
            }
        }
        self.link_artwork_to_gallery(gallery_id, artwork_id)?;
        let existing_private_metadata = {
            let conn = self.lock()?;
            conn.query_row(
                "SELECT purchase_price, estimated_value, purchase_date, provenance, personal_notes
                 FROM private_metadata WHERE artwork_id = ?1",
                params![artwork_id],
                |row| {
                    Ok((
                        row.get::<_, Option<String>>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, Option<String>>(2)?,
                        row.get::<_, Option<String>>(3)?,
                        row.get::<_, Option<String>>(4)?,
                    ))
                },
            )
            .optional()?
            .unwrap_or((None, None, None, None, None))
        };
        self.save_metadata(MetadataUpdate {
            artwork_id,
            title: imported.title,
            description: imported.description,
            for_sale_status: imported.for_sale_status,
            media_type_id: imported.media_type_id,
            art_type_id: imported.art_type_id,
            publication_status_id: Some("2".to_string()),
            active: true,
            illustration_exchange: false,
            ix_for_sale: false,
            artist_credits: imported.artist_credits,
            media: None,
            format: None,
            caf_url: Some(imported.caf_url),
            snikt_url: None,
            raremarq_url: None,
            generic_url: None,
            snikt_metadata: None,
            purchase_price: existing_private_metadata.0,
            estimated_value: existing_private_metadata.1,
            purchase_date: existing_private_metadata.2,
            provenance: existing_private_metadata.3,
            personal_notes: existing_private_metadata.4,
        })?;
        self.ensure_artwork_manifest(artwork_id)?;
        self.artwork_summary(artwork_id)
    }

    pub fn import_caf_image_artwork_in_gallery(
        &self,
        gallery_id: i64,
        imported: ImportedCafImageArtwork,
    ) -> Result<ArtworkSummary> {
        self.import_caf_image_artwork_in_gallery_with_manifest_rewrite(gallery_id, imported, true)
    }

    pub(crate) fn import_caf_image_artwork_in_gallery_deferred_manifest_with_allocator(
        &self,
        gallery_id: i64,
        imported: ImportedCafImageArtwork,
        allocator: &mut CanonicalIdAllocator,
    ) -> Result<ArtworkSummary> {
        self.import_caf_image_artwork_in_gallery_with_manifest_rewrite_and_allocator(
            gallery_id,
            imported,
            false,
            Some(allocator),
        )
    }

    fn import_caf_image_artwork_in_gallery_with_manifest_rewrite(
        &self,
        gallery_id: i64,
        imported: ImportedCafImageArtwork,
        rewrite_gallery_manifest: bool,
    ) -> Result<ArtworkSummary> {
        self.import_caf_image_artwork_in_gallery_with_manifest_rewrite_and_allocator(
            gallery_id,
            imported,
            rewrite_gallery_manifest,
            None,
        )
    }

    fn import_caf_image_artwork_in_gallery_with_manifest_rewrite_and_allocator(
        &self,
        gallery_id: i64,
        imported: ImportedCafImageArtwork,
        rewrite_gallery_manifest: bool,
        allocator: Option<&mut CanonicalIdAllocator>,
    ) -> Result<ArtworkSummary> {
        let gallery = self.gallery_summary(gallery_id)?;
        let title = normalized_name(&imported.title, "Untitled Artwork");
        let now = Utc::now().to_rfc3339();
        let artwork_id = {
            let conn = self.lock()?;
            let matched_id = caf_csv_auto_match_artwork_id_in_gallery_locked(
                &conn, gallery_id, &title, &imported,
            )?;
            if let Some(existing_id) = matched_id {
                conn.execute(
                    "UPDATE artwork SET title = ?1, updated_at = ?2 WHERE id = ?3",
                    params![title, now, existing_id],
                )?;
                existing_id
            } else {
                if artwork_title_exists_in_gallery_locked(&conn, gallery_id, &title)? {
                    return Err(AppError::Message(format!(
                        "CAF CSV row for \"{title}\" matches an existing Artwork title in this Gallery; reconciliation is required before importing it as new."
                    )));
                }
                if let Some(allocator) = allocator {
                    create_imported_artwork_with_allocator_locked(
                        &conn, &gallery, &title, &now, allocator,
                    )?
                } else {
                    create_imported_artwork_locked(&conn, &gallery, &title, &now)?
                }
            }
        };
        self.apply_caf_image_artwork_import(
            gallery_id,
            artwork_id,
            imported,
            rewrite_gallery_manifest,
        )
    }

    pub(crate) fn import_caf_image_artwork_as_new_in_gallery_deferred_manifest(
        &self,
        gallery_id: i64,
        imported: ImportedCafImageArtwork,
    ) -> Result<ArtworkSummary> {
        let gallery = self.gallery_summary(gallery_id)?;
        let title = normalized_name(&imported.title, "Untitled Artwork");
        let now = Utc::now().to_rfc3339();
        let artwork_id = {
            let conn = self.lock()?;
            create_imported_artwork_locked(&conn, &gallery, &title, &now)?
        };
        self.apply_caf_image_artwork_import(gallery_id, artwork_id, imported, false)
    }

    pub(crate) fn import_caf_image_artwork_as_new_in_gallery_deferred_manifest_with_allocator(
        &self,
        gallery_id: i64,
        imported: ImportedCafImageArtwork,
        allocator: &mut CanonicalIdAllocator,
    ) -> Result<ArtworkSummary> {
        let gallery = self.gallery_summary(gallery_id)?;
        let title = normalized_name(&imported.title, "Untitled Artwork");
        let now = Utc::now().to_rfc3339();
        let artwork_id = {
            let conn = self.lock()?;
            create_imported_artwork_with_allocator_locked(&conn, &gallery, &title, &now, allocator)?
        };
        self.apply_caf_image_artwork_import(gallery_id, artwork_id, imported, false)
    }

    pub(crate) fn caf_csv_auto_match_artwork_id_in_gallery(
        &self,
        gallery_id: i64,
        imported: &ImportedCafImageArtwork,
    ) -> Result<Option<i64>> {
        let title = normalized_name(&imported.title, "Untitled Artwork");
        let conn = self.lock()?;
        caf_csv_auto_match_artwork_id_in_gallery_locked(&conn, gallery_id, &title, imported)
    }

    pub(crate) fn import_caf_image_artwork_into_existing_deferred_manifest(
        &self,
        gallery_id: i64,
        artwork_id: i64,
        imported: ImportedCafImageArtwork,
    ) -> Result<ArtworkSummary> {
        let title = normalized_name(&imported.title, "Untitled Artwork");
        let now = Utc::now().to_rfc3339();
        {
            let conn = self.lock()?;
            conn.execute(
                "UPDATE artwork SET title = ?1, updated_at = ?2 WHERE id = ?3",
                params![title, now, artwork_id],
            )?;
        }
        self.apply_caf_image_artwork_import(gallery_id, artwork_id, imported, false)
    }

    fn apply_caf_image_artwork_import(
        &self,
        gallery_id: i64,
        artwork_id: i64,
        imported: ImportedCafImageArtwork,
        rewrite_gallery_manifest: bool,
    ) -> Result<ArtworkSummary> {
        {
            let conn = self.lock()?;
            upsert_external_link_locked(
                &conn,
                artwork_id,
                "caf_image",
                Some(imported.source_image_url.as_str()),
                &imported.source_image_url,
            )?;
            if let Some(source_thumbnail_url) = imported.source_thumbnail_url.as_deref() {
                upsert_external_link_locked(
                    &conn,
                    artwork_id,
                    "caf_image_thumbnail",
                    Some(source_thumbnail_url),
                    source_thumbnail_url,
                )?;
            }
        }
        if rewrite_gallery_manifest {
            self.link_artwork_to_gallery(gallery_id, artwork_id)?;
        } else {
            self.link_artwork_to_gallery_session_only(gallery_id, artwork_id)?;
        }
        let existing_detail = self.artwork_detail(artwork_id)?;
        let update = MetadataUpdate {
            artwork_id,
            title: imported.title,
            description: imported.description,
            for_sale_status: imported.for_sale_status,
            media_type_id: imported.media_type_id,
            art_type_id: imported.art_type_id,
            publication_status_id: existing_detail
                .publication_status_id
                .or_else(|| Some("2".to_string())),
            active: existing_detail.active,
            illustration_exchange: existing_detail.illustration_exchange,
            ix_for_sale: existing_detail.ix_for_sale,
            artist_credits: imported.artist_credits,
            media: None,
            format: None,
            caf_url: existing_detail.caf_url,
            snikt_url: existing_detail.snikt_url,
            raremarq_url: existing_detail.raremarq_url,
            generic_url: existing_detail.generic_url,
            snikt_metadata: None,
            purchase_price: existing_detail.purchase_price.or(imported.purchase_price),
            estimated_value: existing_detail.estimated_value.or(imported.estimated_value),
            purchase_date: existing_detail.purchase_date.or(imported.purchase_date),
            provenance: existing_detail.provenance.or(imported.provenance),
            personal_notes: existing_detail.personal_notes.or(imported.personal_notes),
        };
        if rewrite_gallery_manifest {
            self.save_metadata(update)?;
        } else {
            self.save_metadata_session_only(update)?;
        }
        self.update_caf_csv_tracking(
            artwork_id,
            imported.source_thumbnail_url.as_deref(),
            imported.added_to_caf.as_deref(),
        )?;
        self.ensure_artwork_manifest(artwork_id)?;
        self.artwork_summary(artwork_id)
    }

    pub(crate) fn artwork_title_candidates_in_gallery(
        &self,
        gallery_id: i64,
        title: &str,
    ) -> Result<Vec<ArtworkSummary>> {
        let candidate_ids = {
            let conn = self.lock()?;
            let mut statement = conn.prepare(
                "SELECT a.id
                 FROM artwork a
                 JOIN gallery_artwork ga ON ga.artwork_id = a.id
                 WHERE ga.gallery_id = ?1
                   AND a.title = ?2
                 ORDER BY a.id",
            )?;
            let rows = statement
                .query_map(params![gallery_id, title], |row| row.get::<_, i64>(0))?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            rows
        };
        candidate_ids
            .into_iter()
            .map(|artwork_id| self.artwork_summary(artwork_id))
            .collect()
    }

    pub fn import_snikt_artwork_in_gallery(
        &self,
        gallery_id: i64,
        imported: ImportedSniktArtwork,
    ) -> Result<ArtworkSummary> {
        let gallery = self.gallery_summary(gallery_id)?;
        let snikt_id = validate_external_text_id(Some(&imported.snikt_id), "SNIKT.com Artwork ID")?
            .ok_or_else(|| AppError::Message("SNIKT.com Artwork ID is required".to_string()))?;
        let title = normalized_name(&imported.title, "Untitled Artwork");
        let now = Utc::now().to_rfc3339();
        let artwork_id = {
            let conn = self.lock()?;
            if let Some(existing_id) = artwork_id_for_external_id_locked(&conn, "snikt", &snikt_id)?
            {
                conn.execute(
                    "UPDATE artwork SET title = ?1, updated_at = ?2 WHERE id = ?3",
                    params![title, now, existing_id],
                )?;
                existing_id
            } else {
                let canonical_id = next_canonical_id_locked(&conn)?;
                let manifest_path =
                    default_artwork_manifest_path(&gallery.manifest_path, &canonical_id);
                let path_string = manifest_path.to_string_lossy().to_string();
                conn.execute(
                    "INSERT INTO artwork
                     (canonical_id, artwork_stable_id, title, source_folder, source_context, artwork_manifest_path, created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                    params![
                        canonical_id,
                        canonical_id,
                        title,
                        path_string,
                        gallery.name,
                        path_string,
                        now,
                        now
                    ],
                )?;
                conn.last_insert_rowid()
            }
        };
        {
            let conn = self.lock()?;
            upsert_external_link_locked(
                &conn,
                artwork_id,
                "snikt",
                Some(snikt_id.as_str()),
                &imported.snikt_url,
            )?;
        }
        self.link_artwork_to_gallery(gallery_id, artwork_id)?;
        let existing_private_metadata = {
            let conn = self.lock()?;
            conn.query_row(
                "SELECT purchase_price, estimated_value, purchase_date, provenance, personal_notes
                 FROM private_metadata WHERE artwork_id = ?1",
                params![artwork_id],
                |row| {
                    Ok((
                        row.get::<_, Option<String>>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, Option<String>>(2)?,
                        row.get::<_, Option<String>>(3)?,
                        row.get::<_, Option<String>>(4)?,
                    ))
                },
            )
            .optional()?
            .unwrap_or((None, None, None, None, None))
        };
        self.save_metadata(MetadataUpdate {
            artwork_id,
            title: imported.title,
            description: imported.description,
            for_sale_status: Some("NFS".to_string()),
            media_type_id: None,
            art_type_id: None,
            publication_status_id: Some("2".to_string()),
            active: true,
            illustration_exchange: false,
            ix_for_sale: false,
            artist_credits: imported.artist_credits,
            media: None,
            format: None,
            caf_url: None,
            snikt_url: Some(imported.snikt_url),
            raremarq_url: None,
            generic_url: None,
            snikt_metadata: imported.snikt_metadata,
            purchase_price: existing_private_metadata.0,
            estimated_value: existing_private_metadata.1,
            purchase_date: existing_private_metadata.2,
            provenance: existing_private_metadata.3,
            personal_notes: existing_private_metadata.4,
        })?;
        self.ensure_artwork_manifest(artwork_id)?;
        self.artwork_summary(artwork_id)
    }

    pub fn import_snikt_csv_artwork_in_gallery(
        &self,
        gallery_id: i64,
        imported: ImportedSniktCsvArtwork,
    ) -> Result<ArtworkSummary> {
        let gallery = self.gallery_summary(gallery_id)?;
        let title = normalized_name(&imported.title, "Untitled Artwork");
        let now = Utc::now().to_rfc3339();
        let artwork_id = {
            let conn = self.lock()?;
            if let Some(existing_id) = artwork_id_for_snikt_csv_title_date_in_gallery_locked(
                &conn,
                gallery_id,
                &title,
                imported.created_date.as_deref(),
            )? {
                conn.execute(
                    "UPDATE artwork SET title = ?1, updated_at = ?2 WHERE id = ?3",
                    params![title, now, existing_id],
                )?;
                existing_id
            } else {
                let canonical_id = next_canonical_id_locked(&conn)?;
                let manifest_path =
                    default_artwork_manifest_path(&gallery.manifest_path, &canonical_id);
                let path_string = manifest_path.to_string_lossy().to_string();
                conn.execute(
                    "INSERT INTO artwork
                     (canonical_id, artwork_stable_id, title, source_folder, source_context, artwork_manifest_path, created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                    params![
                        canonical_id,
                        canonical_id,
                        title,
                        path_string,
                        gallery.name,
                        path_string,
                        now,
                        now
                    ],
                )?;
                conn.last_insert_rowid()
            }
        };
        self.apply_snikt_csv_artwork_import(gallery_id, artwork_id, imported, true)
    }

    pub(crate) fn import_snikt_csv_artwork_as_new_in_gallery(
        &self,
        gallery_id: i64,
        imported: ImportedSniktCsvArtwork,
    ) -> Result<ArtworkSummary> {
        let gallery = self.gallery_summary(gallery_id)?;
        let title = normalized_name(&imported.title, "Untitled Artwork");
        let now = Utc::now().to_rfc3339();
        let artwork_id = {
            let conn = self.lock()?;
            create_imported_artwork_locked(&conn, &gallery, &title, &now)?
        };
        self.apply_snikt_csv_artwork_import(gallery_id, artwork_id, imported, true)
    }

    pub(crate) fn import_snikt_csv_artwork_as_new_in_gallery_deferred_manifest_with_allocator(
        &self,
        gallery_id: i64,
        imported: ImportedSniktCsvArtwork,
        allocator: Option<&mut CanonicalIdAllocator>,
    ) -> Result<ArtworkSummary> {
        let gallery = self.gallery_summary(gallery_id)?;
        let title = normalized_name(&imported.title, "Untitled Artwork");
        let now = Utc::now().to_rfc3339();
        let artwork_id = {
            let conn = self.lock()?;
            if let Some(allocator) = allocator {
                create_imported_artwork_with_allocator_locked(
                    &conn, &gallery, &title, &now, allocator,
                )?
            } else {
                create_imported_artwork_locked(&conn, &gallery, &title, &now)?
            }
        };
        self.apply_snikt_csv_artwork_import(gallery_id, artwork_id, imported, false)
    }

    pub(crate) fn import_snikt_csv_artwork_into_existing_in_gallery(
        &self,
        gallery_id: i64,
        artwork_id: i64,
        imported: ImportedSniktCsvArtwork,
    ) -> Result<ArtworkSummary> {
        self.apply_snikt_csv_artwork_import(gallery_id, artwork_id, imported, true)
    }

    fn apply_snikt_csv_artwork_import(
        &self,
        gallery_id: i64,
        artwork_id: i64,
        imported: ImportedSniktCsvArtwork,
        rewrite_gallery_manifest: bool,
    ) -> Result<ArtworkSummary> {
        if rewrite_gallery_manifest {
            self.link_artwork_to_gallery(gallery_id, artwork_id)?;
        } else {
            self.link_artwork_to_gallery_session_only(gallery_id, artwork_id)?;
        }
        let existing_private_metadata = {
            let conn = self.lock()?;
            conn.query_row(
                "SELECT purchase_price, estimated_value, purchase_date, provenance, personal_notes
                 FROM private_metadata WHERE artwork_id = ?1",
                params![artwork_id],
                |row| {
                    Ok((
                        row.get::<_, Option<String>>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, Option<String>>(2)?,
                        row.get::<_, Option<String>>(3)?,
                        row.get::<_, Option<String>>(4)?,
                    ))
                },
            )
            .optional()?
            .unwrap_or((None, None, None, None, None))
        };
        let update = MetadataUpdate {
            artwork_id,
            title: imported.title,
            description: imported.description,
            for_sale_status: Some(
                if imported
                    .snikt_metadata
                    .as_ref()
                    .is_some_and(|metadata| metadata.is_for_sale)
                {
                    "For Sale".to_string()
                } else {
                    "NFS".to_string()
                },
            ),
            media_type_id: None,
            art_type_id: None,
            publication_status_id: Some("2".to_string()),
            active: imported.active,
            illustration_exchange: false,
            ix_for_sale: false,
            artist_credits: imported.artist_credits,
            media: None,
            format: None,
            caf_url: None,
            snikt_url: None,
            raremarq_url: None,
            generic_url: None,
            snikt_metadata: imported.snikt_metadata,
            purchase_price: existing_private_metadata.0,
            estimated_value: existing_private_metadata.1.or(imported.estimated_value),
            purchase_date: existing_private_metadata.2,
            provenance: existing_private_metadata.3,
            personal_notes: existing_private_metadata.4,
        };
        if rewrite_gallery_manifest {
            self.save_metadata(update)?;
        } else {
            self.save_metadata_session_only(update)?;
        }
        self.update_snikt_csv_tracking(artwork_id, imported.created_date.as_deref())?;
        self.ensure_artwork_manifest(artwork_id)?;
        self.artwork_summary(artwork_id)
    }

    pub(crate) fn snikt_csv_artwork_candidates_in_gallery(
        &self,
        gallery_id: i64,
        title: &str,
        created_date: Option<&str>,
    ) -> Result<Vec<ArtworkSummary>> {
        let title = normalized_name(title, "Untitled Artwork");
        let candidate_ids = {
            let conn = self.lock()?;
            let mut statement = if normalize_optional(created_date).is_some() {
                conn.prepare(
                    "SELECT a.id
                     FROM artwork a
                     JOIN gallery_artwork ga ON ga.artwork_id = a.id
                     WHERE ga.gallery_id = ?1
                       AND a.title = ?2
                       AND a.snikt_csv_created_date = ?3
                     ORDER BY a.id",
                )?
            } else {
                conn.prepare(
                    "SELECT a.id
                     FROM artwork a
                     JOIN gallery_artwork ga ON ga.artwork_id = a.id
                     WHERE ga.gallery_id = ?1
                       AND a.title = ?2
                     ORDER BY a.id",
                )?
            };
            let rows = if let Some(created_date) = normalize_optional(created_date) {
                statement
                    .query_map(params![gallery_id, title, created_date], |row| {
                        row.get::<_, i64>(0)
                    })?
                    .collect::<std::result::Result<Vec<_>, _>>()?
            } else {
                statement
                    .query_map(params![gallery_id, title], |row| row.get::<_, i64>(0))?
                    .collect::<std::result::Result<Vec<_>, _>>()?
            };
            rows
        };
        candidate_ids
            .into_iter()
            .map(|artwork_id| self.artwork_summary(artwork_id))
            .collect()
    }

    pub fn workspace_state(&self) -> Result<WorkspaceState> {
        self.workspace_state_with_search(None)
    }

    pub fn workspace_state_with_search(
        &self,
        search_query: Option<&str>,
    ) -> Result<WorkspaceState> {
        self.workspace_state_with_search_and_progress(search_query, |_| {})
    }

    pub fn workspace_state_with_search_and_progress<F>(
        &self,
        search_query: Option<&str>,
        mut progress: F,
    ) -> Result<WorkspaceState>
    where
        F: FnMut(WorkspaceLoadProgress),
    {
        let search_terms = search_query
            .map(parse_workspace_search_query)
            .unwrap_or_default();
        let mode = self
            .setting("active_workspace_mode")?
            .unwrap_or_else(|| "none".to_string());
        let active_collection_id = self
            .setting("active_collection_id")?
            .and_then(|value| value.parse::<i64>().ok());
        let selected_gallery_id = self
            .setting("active_gallery_id")?
            .and_then(|value| value.parse::<i64>().ok());

        if mode == "collection" {
            let collection = active_collection_id
                .and_then(|id| self.collection_summary(id).ok())
                .filter(|collection| collection.manifest_path.is_file());
            if collection.is_none() {
                self.set_setting("active_workspace_mode", "none")?;
                return Ok(empty_workspace_state());
            }
            let galleries = if let Some(collection) = &collection {
                self.galleries_for_collection(collection.id)?
            } else {
                Vec::new()
            };
            let selected_gallery_id =
                selected_gallery_id.or_else(|| galleries.first().map(|gallery| gallery.id));
            let artworks = if let Some(collection) = &collection {
                if search_terms.is_empty() {
                    self.artworks_for_collection_with_progress(collection.id, &mut progress)?
                } else {
                    self.artworks_for_collection_matching_with_progress(
                        collection.id,
                        &search_terms,
                        &mut progress,
                    )?
                }
            } else {
                Vec::new()
            };
            return Ok(WorkspaceState {
                mode,
                collection,
                galleries,
                selected_gallery_id,
                artworks,
            });
        }

        if mode == "loose" {
            let galleries = if let Some(gallery_id) = selected_gallery_id {
                let gallery = self.gallery_summary(gallery_id)?;
                if !gallery.manifest_path.is_file() {
                    self.set_setting("active_workspace_mode", "none")?;
                    return Ok(empty_workspace_state());
                }
                vec![gallery]
            } else {
                Vec::new()
            };
            let artworks = if let Some(gallery_id) = selected_gallery_id {
                if search_terms.is_empty() {
                    self.artworks_for_gallery_with_progress(gallery_id, &mut progress)?
                } else {
                    self.artworks_for_gallery_matching_with_progress(
                        gallery_id,
                        &search_terms,
                        &mut progress,
                    )?
                }
            } else {
                Vec::new()
            };
            return Ok(WorkspaceState {
                mode,
                collection: None,
                galleries,
                selected_gallery_id,
                artworks,
            });
        }

        Ok(empty_workspace_state())
    }

    pub fn close_collection(&self) -> Result<()> {
        self.clear_working_catalog()
    }

    pub fn select_gallery(&self, gallery_id: i64) -> Result<()> {
        let _ = self.gallery_summary(gallery_id)?;
        self.set_setting("active_gallery_id", &gallery_id.to_string())
    }

    pub fn galleries_for_artwork(&self, artwork_id: i64) -> Result<Vec<GallerySummary>> {
        let conn = self.lock()?;
        let mut statement = conn.prepare(
            "SELECT g.id, g.stable_id, g.name, g.manifest_path, g.caf_gallery_room_id, g.snikt_gallery_id, g.snikt_gallery_inherits_collection, g.raremarq_gallery_id
             FROM gallery g
             JOIN gallery_artwork ga ON ga.gallery_id = g.id
             WHERE ga.artwork_id = ?1
             ORDER BY g.name",
        )?;
        let rows = statement.query_map(params![artwork_id], gallery_from_row)?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(AppError::from)
    }

    pub fn collections_for_gallery(&self, gallery_id: i64) -> Result<Vec<CollectionSummary>> {
        let conn = self.lock()?;
        let mut statement = conn.prepare(
            "SELECT c.id, c.stable_id, c.name, c.manifest_path, c.caf_collection_id, c.snikt_collection_id, c.raremarq_collection_id
             FROM collection c
             JOIN collection_gallery cg ON cg.collection_id = c.id
             WHERE cg.gallery_id = ?1
             ORDER BY c.name",
        )?;
        let rows = statement.query_map(params![gallery_id], collection_from_row)?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(AppError::from)
    }

    pub(crate) fn open_gallery_without_activating(
        &self,
        manifest_path: &Path,
    ) -> Result<GallerySummary> {
        let manifest: GalleryManifest = read_json_manifest(manifest_path)?;
        let now = Utc::now().to_rfc3339();
        let path_string = manifest_path.to_string_lossy().to_string();
        let caf_gallery_room_id = provider_id(&manifest.external_links, "com.comicartfans");
        let snikt_gallery_id = provider_id(&manifest.external_links, "com.snikt");
        let raremarq_gallery_id = provider_id(&manifest.external_links, "com.raremarq");
        let snikt_gallery_inherits_collection = gallery_snikt_inherits_collection(&manifest);
        let id = {
            let conn = self.lock()?;
            conn.execute(
                "INSERT INTO gallery (stable_id, name, manifest_path, caf_gallery_room_id, snikt_gallery_id, snikt_gallery_inherits_collection, raremarq_gallery_id, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                 ON CONFLICT(manifest_path) DO UPDATE SET
                   stable_id = excluded.stable_id,
                   name = excluded.name,
                   caf_gallery_room_id = excluded.caf_gallery_room_id,
                   snikt_gallery_id = excluded.snikt_gallery_id,
                   snikt_gallery_inherits_collection = excluded.snikt_gallery_inherits_collection,
                   raremarq_gallery_id = excluded.raremarq_gallery_id,
                   updated_at = excluded.updated_at",
                params![
                    manifest.id,
                    manifest.name,
                    path_string,
                    caf_gallery_room_id,
                    snikt_gallery_id,
                    snikt_gallery_inherits_collection,
                    raremarq_gallery_id,
                    now,
                    now
                ],
            )?;
            conn.query_row(
                "SELECT id FROM gallery WHERE manifest_path = ?1",
                params![path_string],
                |row| row.get(0),
            )?
        };
        self.gallery_summary(id)
    }

    pub(crate) fn collection_summary(&self, collection_id: i64) -> Result<CollectionSummary> {
        let conn = self.lock()?;
        conn.query_row(
            "SELECT id, stable_id, name, manifest_path, caf_collection_id, snikt_collection_id, raremarq_collection_id FROM collection WHERE id = ?1",
            params![collection_id],
            collection_from_row,
        )
        .map_err(AppError::from)
    }

    pub(crate) fn gallery_summary(&self, gallery_id: i64) -> Result<GallerySummary> {
        let conn = self.lock()?;
        conn.query_row(
            "SELECT id, stable_id, name, manifest_path, caf_gallery_room_id, snikt_gallery_id, snikt_gallery_inherits_collection, raremarq_gallery_id FROM gallery WHERE id = ?1",
            params![gallery_id],
            gallery_from_row,
        )
        .map_err(AppError::from)
    }

    fn artwork_summary(&self, artwork_id: i64) -> Result<ArtworkSummary> {
        let conn = self.lock()?;
        let mut statement = conn.prepare(
            r#"
            SELECT
              a.id,
              a.canonical_id,
              a.artwork_stable_id,
              (
                SELECT el.external_id
                FROM external_link el
                WHERE el.artwork_id = a.id AND el.link_type = 'caf'
                LIMIT 1
              ) AS caf_artwork_id,
              (
                SELECT el.external_id
                FROM external_link el
                WHERE el.artwork_id = a.id AND el.link_type = 'snikt'
                LIMIT 1
              ) AS snikt_artwork_id,
              (
                SELECT el.external_id
                FROM external_link el
                WHERE el.artwork_id = a.id AND el.link_type = 'raremarq'
                LIMIT 1
              ) AS raremarq_artwork_id,
              a.title,
              a.media,
              a.format,
              a.source_folder,
              (
                SELECT da.path
                FROM derived_asset da
                LEFT JOIN file_asset fa ON fa.id = da.source_file_asset_id
                WHERE da.artwork_id = a.id AND da.derivative_type = 'thumbnail'
                ORDER BY COALESCE(fa.display_order, 999999), fa.is_primary DESC, da.id
                LIMIT 1
              ) AS thumbnail_path,
              (
                SELECT COUNT(*)
                FROM file_asset f
                WHERE f.artwork_id = a.id
              ) + (
                SELECT COUNT(*)
                FROM derived_asset export
                WHERE export.artwork_id = a.id AND export.derivative_type = 'png_export'
              ) AS file_count,
              a.artwork_manifest_path
            FROM artwork a
            WHERE a.id = ?1
            "#,
        )?;
        let mut summary = statement.query_row(params![artwork_id], artwork_from_row)?;
        summary.gallery_ids = self.gallery_ids_for_artwork_locked(&conn, artwork_id)?;
        summary.gallery_names = self.gallery_names_for_artwork_locked(&conn, artwork_id)?;
        summary.artist_credits = self.artist_credits_locked(&conn, artwork_id)?;
        let preference = artwork_id_label_preference_locked(&conn)?;
        apply_artwork_id_label_preference(&mut summary, preference);
        Ok(summary)
    }

    pub(crate) fn galleries_for_collection(
        &self,
        collection_id: i64,
    ) -> Result<Vec<GallerySummary>> {
        let conn = self.lock()?;
        let mut statement = conn.prepare(
            "SELECT g.id, g.stable_id, g.name, g.manifest_path, g.caf_gallery_room_id, g.snikt_gallery_id, g.snikt_gallery_inherits_collection, g.raremarq_gallery_id
             FROM gallery g
             JOIN collection_gallery cg ON cg.gallery_id = g.id
             WHERE cg.collection_id = ?1
             ORDER BY cg.sort_order, g.name",
        )?;
        let rows = statement.query_map(params![collection_id], gallery_from_row)?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(AppError::from)
    }

    pub(crate) fn file_assets_missing_thumbnail_for_collection(
        &self,
        collection_id: i64,
    ) -> Result<Vec<(i64, i64, PathBuf)>> {
        self.file_assets_missing_cache_derivative_for_collection(collection_id, "thumbnail")
    }

    pub(crate) fn file_assets_missing_preview_for_collection(
        &self,
        collection_id: i64,
    ) -> Result<Vec<(i64, i64, PathBuf)>> {
        self.file_assets_missing_cache_derivative_for_collection(collection_id, "preview")
    }

    fn file_assets_missing_cache_derivative_for_collection(
        &self,
        collection_id: i64,
        derivative_type: &str,
    ) -> Result<Vec<(i64, i64, PathBuf)>> {
        let conn = self.lock()?;
        let mut statement = conn.prepare(
            r#"
            SELECT DISTINCT fa.artwork_id, fa.id, fa.current_path
            FROM file_asset fa
            JOIN gallery_artwork ga ON ga.artwork_id = fa.artwork_id
            JOIN collection_gallery cg ON cg.gallery_id = ga.gallery_id
            WHERE cg.collection_id = ?1
              AND NOT EXISTS (
                SELECT 1
                FROM derived_asset da
                WHERE da.source_file_asset_id = fa.id
                  AND da.derivative_type = ?2
              )
            ORDER BY fa.artwork_id, fa.is_primary DESC, fa.display_order, fa.id
            "#,
        )?;
        let rows = statement.query_map(params![collection_id, derivative_type], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                PathBuf::from(row.get::<_, String>(2)?),
            ))
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(AppError::from)
    }

    pub(crate) fn artworks_for_collection(
        &self,
        collection_id: i64,
    ) -> Result<Vec<ArtworkSummary>> {
        self.artworks_for_collection_with_progress(collection_id, |_| {})
    }

    fn artworks_for_collection_with_progress<F>(
        &self,
        collection_id: i64,
        progress: F,
    ) -> Result<Vec<ArtworkSummary>>
    where
        F: FnMut(WorkspaceLoadProgress),
    {
        let conn = self.lock()?;
        let ids = collect_i64_column_with_param(
            &conn,
            "SELECT DISTINCT a.id
             FROM artwork a
             JOIN gallery_artwork ga ON ga.artwork_id = a.id
             JOIN collection_gallery cg ON cg.gallery_id = ga.gallery_id
             WHERE cg.collection_id = ?1
             ORDER BY a.canonical_id",
            collection_id,
        )?;
        drop(conn);
        self.artwork_summaries_with_progress(ids, progress)
    }

    fn artworks_for_collection_matching_with_progress<F>(
        &self,
        collection_id: i64,
        terms: &[WorkspaceSearchTerm],
        progress: F,
    ) -> Result<Vec<ArtworkSummary>>
    where
        F: FnMut(WorkspaceLoadProgress),
    {
        let conn = self.lock()?;
        let ids = matching_artwork_ids_for_scope(
            &conn,
            "JOIN gallery_artwork ga ON ga.artwork_id = a.id
             JOIN collection_gallery cg ON cg.gallery_id = ga.gallery_id
             WHERE cg.collection_id = ?1",
            "searched.canonical_id",
            collection_id,
            terms,
        )?;
        drop(conn);
        self.artwork_summaries_with_progress(ids, progress)
    }

    pub(crate) fn artworks_for_gallery(&self, gallery_id: i64) -> Result<Vec<ArtworkSummary>> {
        self.artworks_for_gallery_with_progress(gallery_id, |_| {})
    }

    fn artworks_for_gallery_with_progress<F>(
        &self,
        gallery_id: i64,
        progress: F,
    ) -> Result<Vec<ArtworkSummary>>
    where
        F: FnMut(WorkspaceLoadProgress),
    {
        let conn = self.lock()?;
        let ids = collect_i64_column_with_param(
            &conn,
            "SELECT a.id
             FROM artwork a
             JOIN gallery_artwork ga ON ga.artwork_id = a.id
             WHERE ga.gallery_id = ?1
             ORDER BY ga.sort_order, a.canonical_id",
            gallery_id,
        )?;
        drop(conn);
        self.artwork_summaries_with_progress(ids, progress)
    }

    fn artworks_for_gallery_matching_with_progress<F>(
        &self,
        gallery_id: i64,
        terms: &[WorkspaceSearchTerm],
        progress: F,
    ) -> Result<Vec<ArtworkSummary>>
    where
        F: FnMut(WorkspaceLoadProgress),
    {
        let conn = self.lock()?;
        let ids = matching_artwork_ids_for_scope(
            &conn,
            "JOIN gallery_artwork ga ON ga.artwork_id = a.id
             WHERE ga.gallery_id = ?1",
            "searched.sort_order, searched.canonical_id",
            gallery_id,
            terms,
        )?;
        drop(conn);
        self.artwork_summaries_with_progress(ids, progress)
    }

    fn artwork_summaries_with_progress<F>(
        &self,
        ids: Vec<i64>,
        mut progress: F,
    ) -> Result<Vec<ArtworkSummary>>
    where
        F: FnMut(WorkspaceLoadProgress),
    {
        let total = ids.len();
        if total > 0 {
            progress(WorkspaceLoadProgress {
                phase: "artworks".to_string(),
                message: format!("Loading artwork 0 of {total}"),
                artworks_total: total,
                artworks_loaded: 0,
                current_artwork: None,
                done: false,
            });
        }
        let mut summaries = Vec::with_capacity(total);
        let loaded_summaries = self.artwork_summaries_for_ids(&ids)?;
        for (index, summary) in loaded_summaries.into_iter().enumerate() {
            let loaded = index + 1;
            progress(WorkspaceLoadProgress {
                phase: "artworks".to_string(),
                message: format!("Loaded artwork {loaded} of {total}: {}", summary.title),
                artworks_total: total,
                artworks_loaded: loaded,
                current_artwork: Some(summary.title.clone()),
                done: loaded == total,
            });
            summaries.push(summary);
        }
        Ok(summaries)
    }

    fn artwork_summaries_for_ids(&self, ids: &[i64]) -> Result<Vec<ArtworkSummary>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let conn = self.lock()?;
        let preference = artwork_id_label_preference_locked(&conn)?;
        let mut summaries_by_id = BTreeMap::new();
        for chunk in ids.chunks(SUMMARY_QUERY_CHUNK_SIZE) {
            let placeholders = sql_placeholders(chunk.len());
            let mut statement = conn.prepare(&format!(
                r#"
                SELECT
                  a.id,
                  a.canonical_id,
                  a.artwork_stable_id,
                  (
                    SELECT el.external_id
                    FROM external_link el
                    WHERE el.artwork_id = a.id AND el.link_type = 'caf'
                    LIMIT 1
                  ) AS caf_artwork_id,
                  (
                    SELECT el.external_id
                    FROM external_link el
                    WHERE el.artwork_id = a.id AND el.link_type = 'snikt'
                    LIMIT 1
                  ) AS snikt_artwork_id,
                  (
                    SELECT el.external_id
                    FROM external_link el
                    WHERE el.artwork_id = a.id AND el.link_type = 'raremarq'
                    LIMIT 1
                  ) AS raremarq_artwork_id,
                  a.title,
                  a.media,
                  a.format,
                  a.source_folder,
                  (
                    SELECT da.path
                    FROM derived_asset da
                    LEFT JOIN file_asset fa ON fa.id = da.source_file_asset_id
                    WHERE da.artwork_id = a.id AND da.derivative_type = 'thumbnail'
                    ORDER BY COALESCE(fa.display_order, 999999), fa.is_primary DESC, da.id
                    LIMIT 1
                  ) AS thumbnail_path,
                  (
                    SELECT COUNT(*)
                    FROM file_asset f
                    WHERE f.artwork_id = a.id
                  ) + (
                    SELECT COUNT(*)
                    FROM derived_asset export
                    WHERE export.artwork_id = a.id AND export.derivative_type = 'png_export'
                  ) AS file_count,
                  a.artwork_manifest_path
                FROM artwork a
                WHERE a.id IN ({placeholders})
                "#
            ))?;
            let rows = statement.query_map(params_from_iter(chunk.iter()), artwork_from_row)?;
            for summary in rows {
                let summary = summary?;
                summaries_by_id.insert(summary.id, summary);
            }
        }

        let mut gallery_ids_by_artwork: BTreeMap<i64, Vec<i64>> = BTreeMap::new();
        let mut gallery_names_by_artwork: BTreeMap<i64, Vec<String>> = BTreeMap::new();
        for chunk in ids.chunks(SUMMARY_QUERY_CHUNK_SIZE) {
            let placeholders = sql_placeholders(chunk.len());
            let mut statement = conn.prepare(&format!(
                "SELECT ga.artwork_id, ga.gallery_id, g.name
                 FROM gallery_artwork ga
                 JOIN gallery g ON g.id = ga.gallery_id
                 WHERE ga.artwork_id IN ({placeholders})
                 ORDER BY ga.artwork_id, ga.sort_order, ga.gallery_id"
            ))?;
            let rows = statement.query_map(params_from_iter(chunk.iter()), |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })?;
            for row in rows {
                let (artwork_id, gallery_id, gallery_name) = row?;
                gallery_ids_by_artwork
                    .entry(artwork_id)
                    .or_default()
                    .push(gallery_id);
                gallery_names_by_artwork
                    .entry(artwork_id)
                    .or_default()
                    .push(gallery_name);
            }
        }

        let mut artist_credits_by_artwork: BTreeMap<i64, Vec<ArtistCredit>> = BTreeMap::new();
        for chunk in ids.chunks(SUMMARY_QUERY_CHUNK_SIZE) {
            let placeholders = sql_placeholders(chunk.len());
            let mut statement = conn.prepare(&format!(
                "SELECT aa.artwork_id, ar.name, aa.role, aa.first_name, aa.last_name, aa.role_id
                 FROM artwork_artist aa
                 JOIN artist ar ON ar.id = aa.artist_id
                 WHERE aa.artwork_id IN ({placeholders})
                 ORDER BY aa.artwork_id, aa.sort_order, ar.name"
            ))?;
            let rows = statement.query_map(params_from_iter(chunk.iter()), |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    ArtistCredit {
                        name: row.get(1)?,
                        role: row.get(2)?,
                        first_name: row.get(3)?,
                        last_name: row.get(4)?,
                        role_id: row.get(5)?,
                    },
                ))
            })?;
            for row in rows {
                let (artwork_id, credit) = row?;
                artist_credits_by_artwork
                    .entry(artwork_id)
                    .or_default()
                    .push(credit);
            }
        }

        ids.iter()
            .map(|id| {
                let mut summary = summaries_by_id.remove(id).ok_or_else(|| {
                    AppError::Message(format!("Artwork summary not found for id {id}"))
                })?;
                summary.gallery_ids = gallery_ids_by_artwork.remove(id).unwrap_or_default();
                summary.gallery_names = gallery_names_by_artwork.remove(id).unwrap_or_default();
                summary.artist_credits = artist_credits_by_artwork.remove(id).unwrap_or_default();
                apply_artwork_id_label_preference(&mut summary, preference);
                Ok(summary)
            })
            .collect()
    }

    pub(crate) fn artwork_id_for_external_id(
        &self,
        provider: &str,
        external_id: &str,
    ) -> Result<Option<i64>> {
        let conn = self.lock()?;
        artwork_id_for_external_id_locked(&conn, provider, external_id)
    }

    fn collection_gallery_count(&self, collection_id: i64) -> Result<i64> {
        let conn = self.lock()?;
        conn.query_row(
            "SELECT COUNT(*) FROM collection_gallery WHERE collection_id = ?1",
            params![collection_id],
            |row| row.get(0),
        )
        .map_err(AppError::from)
    }

    fn gallery_artwork_count(&self, gallery_id: i64) -> Result<i64> {
        let conn = self.lock()?;
        conn.query_row(
            "SELECT COUNT(*) FROM gallery_artwork WHERE gallery_id = ?1",
            params![gallery_id],
            |row| row.get(0),
        )
        .map_err(AppError::from)
    }

    fn rewrite_collection_manifest(&self, collection_id: i64) -> Result<()> {
        let collection = self.collection_summary(collection_id)?;
        let galleries = self.galleries_for_collection(collection_id)?;
        let mut artwork_by_id = BTreeMap::new();
        for gallery in &galleries {
            for artwork in self.artworks_for_gallery(gallery.id)? {
                artwork_by_id.entry(artwork.id).or_insert(artwork);
            }
        }
        let manifest = CollectionManifest {
            schema_version: SCHEMA_VERSION.to_string(),
            id: collection.stable_id.clone(),
            name: collection.name.clone(),
            external_links: collection_external_links(&collection),
            galleries: galleries
                .into_iter()
                .map(|gallery| ManifestReference {
                    id: gallery.stable_id,
                    name: gallery.name,
                    path: archive_relative_path(&collection.manifest_path, &gallery.manifest_path),
                    extensions: BTreeMap::new(),
                })
                .collect(),
            artworks: artwork_by_id
                .into_values()
                .filter_map(|artwork| {
                    artwork.manifest_path.map(|path| ArtworkManifestReference {
                        id: artwork.canonical_id,
                        title: None,
                        path: Some(archive_relative_path(&collection.manifest_path, &path)),
                        extensions: BTreeMap::new(),
                    })
                })
                .collect(),
            extensions: BTreeMap::new(),
        };
        write_json_manifest(&collection.manifest_path, &manifest)
    }

    pub(crate) fn rewrite_gallery_manifest(&self, gallery_id: i64) -> Result<()> {
        if let Ok(mut counts) = self.manifest_rewrite_debug_counts.lock() {
            counts.gallery += 1;
        }
        let gallery = self.gallery_summary(gallery_id)?;
        let artworks = self.artworks_for_gallery(gallery_id)?;
        let manifest = GalleryManifest {
            schema_version: SCHEMA_VERSION.to_string(),
            id: gallery.stable_id.clone(),
            name: gallery.name.clone(),
            external_links: gallery_external_links(&gallery),
            artworks: artworks
                .into_iter()
                .map(|artwork| ArtworkManifestReference {
                    id: artwork.canonical_id,
                    title: None,
                    path: None,
                    extensions: BTreeMap::new(),
                })
                .collect(),
            extensions: gallery_extensions(&gallery),
        };
        write_json_manifest(&gallery.manifest_path, &manifest)
    }

    pub(crate) fn rewrite_collections_for_gallery(&self, gallery_id: i64) -> Result<()> {
        for collection in self.collections_for_gallery(gallery_id)? {
            self.rewrite_collection_manifest(collection.id)?;
        }
        Ok(())
    }

    pub fn ensure_artwork_manifest(&self, artwork_id: i64) -> Result<()> {
        ManifestProjector::new(self).project_artwork(artwork_id)
    }

    pub fn reconcile_artwork_manifest_from_catalog(&self, artwork_id: i64) -> Result<()> {
        if self.artwork_manifest_path(artwork_id)?.is_none() {
            return Ok(());
        }
        ManifestProjector::new(self).reconcile_artwork(artwork_id)
    }

    pub fn artwork_asset_folder(&self, artwork_id: i64) -> Result<PathBuf> {
        let detail = self.artwork_detail(artwork_id)?;
        let manifest_path = self.artwork_manifest_path(artwork_id)?.ok_or_else(|| {
            AppError::Message(format!(
                "Artwork {} does not have an .oaartwork manifest path",
                detail.canonical_id
            ))
        })?;
        Ok(manifest_path
            .parent()
            .unwrap_or(&manifest_path)
            .to_path_buf())
    }

    fn artwork_manifest_path(&self, artwork_id: i64) -> Result<Option<PathBuf>> {
        let conn = self.lock()?;
        self.artwork_manifest_path_locked(&conn, artwork_id)
    }

    fn artwork_manifest_path_locked(
        &self,
        conn: &Connection,
        artwork_id: i64,
    ) -> Result<Option<PathBuf>> {
        let value: Option<String> = conn.query_row(
            "SELECT artwork_manifest_path FROM artwork WHERE id = ?1",
            params![artwork_id],
            |row| row.get(0),
        )?;
        Ok(value.map(PathBuf::from))
    }

    pub fn upsert_file_asset_with_paths(
        &self,
        artwork_id: i64,
        original_path: &Path,
        root: &Path,
        path: &Path,
        is_primary: bool,
        source_kind: &str,
    ) -> Result<i64> {
        let source_kind = normalize_file_source_kind(source_kind)?;
        let metadata = fs::metadata(path)?;
        let image_metadata = read_image_metadata(path)?;
        let relative_path = path
            .strip_prefix(root)
            .unwrap_or(path)
            .to_string_lossy()
            .to_string();
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_string();
        let extension = path
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        let path_string = path.to_string_lossy().to_string();
        let modified_at = metadata
            .modified()
            .ok()
            .map(|time| chrono::DateTime::<Utc>::from(time).to_rfc3339());
        let metadata_checked_at = Utc::now().to_rfc3339();
        let original_path_string = original_path.to_string_lossy().to_string();
        let conn = self.lock()?;
        let display_order: i64 = conn.query_row(
            "SELECT COUNT(*) FROM file_asset WHERE artwork_id = ?1",
            params![artwork_id],
            |row| row.get(0),
        )?;
        conn.execute(
            "INSERT INTO file_asset
             (artwork_id, original_path, current_path, relative_path, file_name, extension, size_bytes, width, height, dpi_x, dpi_y, metadata_checked_at, modified_at, source_kind, is_primary, display_order)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)
             ON CONFLICT(current_path) DO UPDATE SET
               artwork_id = excluded.artwork_id,
               relative_path = excluded.relative_path,
               file_name = excluded.file_name,
               extension = excluded.extension,
               size_bytes = excluded.size_bytes,
               width = excluded.width,
               height = excluded.height,
               dpi_x = excluded.dpi_x,
               dpi_y = excluded.dpi_y,
               metadata_checked_at = excluded.metadata_checked_at,
               modified_at = excluded.modified_at,
               source_kind = excluded.source_kind",
            params![
                artwork_id,
                original_path_string,
                path_string,
                relative_path,
                file_name,
                extension,
                metadata.len() as i64,
                image_metadata.width,
                image_metadata.height,
                image_metadata.dpi_x,
                image_metadata.dpi_y,
                metadata_checked_at,
                modified_at,
                source_kind,
                if is_primary { 1 } else { 0 },
                display_order
            ],
        )?;
        Ok(conn.query_row(
            "SELECT id FROM file_asset WHERE current_path = ?1",
            params![path_string],
            |row| row.get(0),
        )?)
    }

    pub(crate) fn upsert_file_asset_with_known_metadata(
        &self,
        artwork_id: i64,
        insert: FileAssetKnownMetadataInsert<'_>,
    ) -> Result<i64> {
        let source_kind = normalize_file_source_kind(insert.source_kind)?;
        let metadata = fs::metadata(insert.path)?;
        let relative_path = insert
            .path
            .strip_prefix(insert.root)
            .unwrap_or(insert.path)
            .to_string_lossy()
            .to_string();
        let file_name = insert
            .path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_string();
        let extension = insert
            .path
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        let path_string = insert.path.to_string_lossy().to_string();
        let modified_at = metadata
            .modified()
            .ok()
            .map(|time| chrono::DateTime::<Utc>::from(time).to_rfc3339());
        let metadata_checked_at = Utc::now().to_rfc3339();
        let original_path_string = insert.original_path.to_string_lossy().to_string();
        let conn = self.lock()?;
        let display_order: i64 = conn.query_row(
            "SELECT COUNT(*) FROM file_asset WHERE artwork_id = ?1",
            params![artwork_id],
            |row| row.get(0),
        )?;
        conn.execute(
            "INSERT INTO file_asset
             (artwork_id, original_path, current_path, relative_path, file_name, extension, size_bytes, width, height, dpi_x, dpi_y, metadata_checked_at, modified_at, source_kind, is_primary, display_order)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)
             ON CONFLICT(current_path) DO UPDATE SET
               artwork_id = excluded.artwork_id,
               original_path = excluded.original_path,
               relative_path = excluded.relative_path,
               file_name = excluded.file_name,
               extension = excluded.extension,
               size_bytes = excluded.size_bytes,
               width = excluded.width,
               height = excluded.height,
               dpi_x = excluded.dpi_x,
               dpi_y = excluded.dpi_y,
               metadata_checked_at = excluded.metadata_checked_at,
               modified_at = excluded.modified_at,
               source_kind = excluded.source_kind,
               is_primary = excluded.is_primary",
            params![
                artwork_id,
                original_path_string,
                path_string,
                relative_path,
                file_name,
                extension,
                metadata.len() as i64,
                insert.metadata.width,
                insert.metadata.height,
                insert.metadata.dpi_x,
                insert.metadata.dpi_y,
                metadata_checked_at,
                modified_at,
                source_kind,
                if insert.is_primary { 1 } else { 0 },
                display_order
            ],
        )?;
        Ok(conn.query_row(
            "SELECT id FROM file_asset WHERE current_path = ?1",
            params![path_string],
            |row| row.get(0),
        )?)
    }

    pub(crate) fn upsert_artwork_external_link(
        &self,
        artwork_id: i64,
        provider: &str,
        external_id: Option<&str>,
        url: &str,
        extensions: Option<&serde_json::Value>,
    ) -> Result<()> {
        let conn = self.lock()?;
        upsert_external_link_locked_with_extensions(
            &conn,
            artwork_id,
            provider,
            external_id,
            url,
            extensions,
        )
    }

    pub(crate) fn artwork_external_links(
        &self,
        artwork_id: i64,
    ) -> Result<Vec<ExternalLinkRecord>> {
        let conn = self.lock()?;
        let mut statement = conn.prepare(
            "SELECT link_type, external_id, url, extensions_json
             FROM external_link
             WHERE artwork_id = ?1
             ORDER BY id",
        )?;
        let rows = statement.query_map(params![artwork_id], |row| {
            let extensions_json: Option<String> = row.get(3)?;
            let extensions = extensions_json
                .as_deref()
                .map(serde_json::from_str)
                .transpose()
                .map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(
                        3,
                        rusqlite::types::Type::Text,
                        Box::new(error),
                    )
                })?;
            Ok(ExternalLinkRecord {
                provider: row.get(0)?,
                external_id: row.get(1)?,
                url: row.get(2)?,
                extensions,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(AppError::from)
    }

    pub(crate) fn upsert_file_asset_external_link(
        &self,
        file_asset_id: i64,
        provider: &str,
        external_id: &str,
        url: &str,
        extensions: Option<&serde_json::Value>,
    ) -> Result<()> {
        let extensions_json = extensions.map(serde_json::to_string).transpose()?;
        let conn = self.lock()?;
        conn.execute(
            "INSERT INTO file_asset_external_link
               (file_asset_id, provider, external_id, url, extensions_json)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(file_asset_id, provider, external_id) DO UPDATE SET
               url = excluded.url,
               extensions_json = excluded.extensions_json",
            params![
                file_asset_id,
                provider,
                external_id,
                url,
                extensions_json.as_deref()
            ],
        )?;
        Ok(())
    }

    pub(crate) fn file_asset_external_links(
        &self,
        file_asset_id: i64,
    ) -> Result<Vec<FileExternalLinkRecord>> {
        let conn = self.lock()?;
        let mut statement = conn.prepare(
            "SELECT provider, external_id, url, extensions_json
             FROM file_asset_external_link
             WHERE file_asset_id = ?1
             ORDER BY id",
        )?;
        let rows = statement.query_map(params![file_asset_id], |row| {
            let extensions_json: Option<String> = row.get(3)?;
            let extensions = extensions_json
                .as_deref()
                .map(serde_json::from_str)
                .transpose()
                .map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(
                        3,
                        rusqlite::types::Type::Text,
                        Box::new(error),
                    )
                })?;
            Ok(FileExternalLinkRecord {
                provider: row.get(0)?,
                external_id: row.get(1)?,
                url: row.get(2)?,
                extensions,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(AppError::from)
    }

    pub(crate) fn save_oaa_extension_block(
        &self,
        owner_kind: &str,
        owner_id: i64,
        provider: &str,
        value: &serde_json::Value,
    ) -> Result<()> {
        let json = serde_json::to_string(value)?;
        let conn = self.lock()?;
        conn.execute(
            "INSERT INTO oaa_extension_block (owner_kind, owner_id, provider, json)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(owner_kind, owner_id, provider) DO UPDATE SET
               json = excluded.json",
            params![owner_kind, owner_id, provider, json],
        )?;
        Ok(())
    }

    pub(crate) fn oaa_extension_blocks(
        &self,
        owner_kind: &str,
        owner_id: i64,
    ) -> Result<Vec<(String, serde_json::Value)>> {
        let conn = self.lock()?;
        let mut statement = conn.prepare(
            "SELECT provider, json
             FROM oaa_extension_block
             WHERE owner_kind = ?1 AND owner_id = ?2
             ORDER BY provider",
        )?;
        let rows = statement.query_map(params![owner_kind, owner_id], |row| {
            let json: String = row.get(1)?;
            let value = serde_json::from_str(&json).map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(
                    1,
                    rusqlite::types::Type::Text,
                    Box::new(error),
                )
            })?;
            Ok((row.get(0)?, value))
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(AppError::from)
    }

    pub fn add_derived_asset(
        &self,
        artwork_id: i64,
        insert: DerivedAssetInsert<'_>,
    ) -> Result<DerivedAsset> {
        self.add_derived_asset_with_manifest_rewrite(artwork_id, insert, true)
    }

    pub(crate) fn add_derived_asset_session_only(
        &self,
        artwork_id: i64,
        insert: DerivedAssetInsert<'_>,
    ) -> Result<DerivedAsset> {
        self.add_derived_asset_with_manifest_rewrite(artwork_id, insert, false)
    }

    fn add_derived_asset_with_manifest_rewrite(
        &self,
        artwork_id: i64,
        insert: DerivedAssetInsert<'_>,
        rewrite_artwork_manifest: bool,
    ) -> Result<DerivedAsset> {
        let now = Utc::now().to_rfc3339();
        let path_string = insert.path.to_string_lossy().to_string();
        let image_role = normalize_image_role(insert.image_role)?;
        let asset = {
            let conn = self.lock()?;
            conn.execute(
                "INSERT INTO derived_asset
                 (artwork_id, source_file_asset_id, derivative_type, format, path, width, height, image_role, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                 ON CONFLICT(path) DO UPDATE SET
                   artwork_id = excluded.artwork_id,
                   source_file_asset_id = excluded.source_file_asset_id,
                   derivative_type = excluded.derivative_type,
                   format = excluded.format,
                   width = excluded.width,
                   height = excluded.height,
                   image_role = excluded.image_role,
                   created_at = excluded.created_at",
                params![
                    artwork_id,
                    insert.source_file_asset_id,
                    insert.derivative_type,
                    insert.format,
                    path_string,
                    insert.width,
                    insert.height,
                    image_role.as_deref(),
                    now
                ],
            )?;
            let id = conn.query_row(
                "SELECT id FROM derived_asset WHERE path = ?1",
                params![path_string],
                |row| row.get(0),
            )?;
            DerivedAsset {
                id,
                artwork_id,
                source_file_asset_id: insert.source_file_asset_id,
                derivative_type: insert.derivative_type.to_string(),
                format: insert.format.to_string(),
                path: insert.path.to_path_buf(),
                width: insert.width,
                height: insert.height,
                image_role,
            }
        };
        if rewrite_artwork_manifest && self.artwork_manifest_path(artwork_id)?.is_some() {
            self.ensure_artwork_manifest(artwork_id)?;
        }
        Ok(asset)
    }

    pub fn update_image_role(
        &self,
        asset_kind: AssetKind,
        asset_id: i64,
        image_role: Option<&str>,
    ) -> Result<i64> {
        let image_role = normalize_image_role(image_role)?;
        let conn = self.lock()?;
        let artwork_id: i64 = match asset_kind {
            AssetKind::File => {
                let artwork_id = conn.query_row(
                    "SELECT artwork_id FROM file_asset WHERE id = ?1",
                    params![asset_id],
                    |row| row.get(0),
                )?;
                conn.execute(
                    "UPDATE file_asset SET image_role = ?1 WHERE id = ?2",
                    params![image_role.as_deref(), asset_id],
                )?;
                artwork_id
            }
            AssetKind::Derived => {
                let artwork_id = conn.query_row(
                    "SELECT artwork_id FROM derived_asset WHERE id = ?1",
                    params![asset_id],
                    |row| row.get(0),
                )?;
                conn.execute(
                    "UPDATE derived_asset SET image_role = ?1 WHERE id = ?2",
                    params![image_role.as_deref(), asset_id],
                )?;
                artwork_id
            }
        };
        drop(conn);
        if self.artwork_manifest_path(artwork_id)?.is_some() {
            self.ensure_artwork_manifest(artwork_id)?;
        }
        Ok(artwork_id)
    }

    fn update_file_asset_image_role_session_only(
        &self,
        file_asset_id: i64,
        image_role: Option<&str>,
    ) -> Result<()> {
        let image_role = normalize_image_role(image_role)?;
        let conn = self.lock()?;
        conn.execute(
            "UPDATE file_asset SET image_role = ?1 WHERE id = ?2",
            params![image_role.as_deref(), file_asset_id],
        )?;
        Ok(())
    }

    pub fn reorder_file_assets(
        &self,
        artwork_id: i64,
        file_asset_ids: &[i64],
    ) -> Result<ArtworkDetail> {
        if file_asset_ids.is_empty() {
            return Err(AppError::Message(
                "At least one file asset is required to reorder images".to_string(),
            ));
        }
        let requested_ids = file_asset_ids.iter().copied().collect::<BTreeSet<_>>();
        if requested_ids.len() != file_asset_ids.len() {
            return Err(AppError::Message(
                "Carousel order contains duplicate file assets".to_string(),
            ));
        }

        {
            let mut conn = self.lock()?;
            let existing_ids = {
                let mut statement = conn.prepare(
                    "SELECT id FROM file_asset WHERE artwork_id = ?1 ORDER BY display_order, is_primary DESC, id",
                )?;
                let rows = statement.query_map(params![artwork_id], |row| row.get::<_, i64>(0))?;
                rows.collect::<std::result::Result<Vec<_>, _>>()
                    .map_err(AppError::from)?
            };
            if existing_ids.is_empty() {
                return Err(AppError::Message(format!(
                    "Artwork {artwork_id} has no file assets to reorder"
                )));
            }
            let existing_set = existing_ids.iter().copied().collect::<BTreeSet<_>>();
            if requested_ids != existing_set {
                return Err(AppError::Message(
                    "Carousel order must include every file asset for the artwork exactly once"
                        .to_string(),
                ));
            }

            let transaction = conn.transaction()?;
            for (display_order, file_asset_id) in file_asset_ids.iter().enumerate() {
                transaction.execute(
                    "UPDATE file_asset
                     SET display_order = ?1, is_primary = ?2
                     WHERE artwork_id = ?3 AND id = ?4",
                    params![
                        display_order as i64,
                        if display_order == 0 { 1 } else { 0 },
                        artwork_id,
                        file_asset_id
                    ],
                )?;
            }
            transaction.commit()?;
        }

        if self.artwork_manifest_path(artwork_id)?.is_some() {
            self.ensure_artwork_manifest(artwork_id)?;
        }
        self.artwork_detail(artwork_id)
    }

    pub fn rename_collection(&self, collection_id: i64, name: &str) -> Result<CollectionSummary> {
        let name = validate_workspace_name(name, "Collection name")?;
        let now = Utc::now().to_rfc3339();
        {
            let conn = self.lock()?;
            let changed = conn.execute(
                "UPDATE collection SET name = ?1, updated_at = ?2 WHERE id = ?3",
                params![name, now, collection_id],
            )?;
            if changed == 0 {
                return Err(AppError::Message(format!(
                    "Collection {collection_id} was not found"
                )));
            }
        }
        self.rewrite_collection_manifest(collection_id)?;
        self.collection_summary(collection_id)
    }

    pub fn rename_gallery(&self, gallery_id: i64, name: &str) -> Result<GallerySummary> {
        let name = validate_workspace_name(name, "Gallery name")?;
        let now = Utc::now().to_rfc3339();
        {
            let conn = self.lock()?;
            let changed = conn.execute(
                "UPDATE gallery SET name = ?1, updated_at = ?2 WHERE id = ?3",
                params![name, now, gallery_id],
            )?;
            if changed == 0 {
                return Err(AppError::Message(format!(
                    "Gallery {gallery_id} was not found"
                )));
            }
        }
        self.rewrite_gallery_manifest(gallery_id)?;
        for collection in self.collections_for_gallery(gallery_id)? {
            self.rewrite_collection_manifest(collection.id)?;
        }
        self.gallery_summary(gallery_id)
    }

    pub fn rename_artwork_title(&self, artwork_id: i64, title: &str) -> Result<ArtworkDetail> {
        let title = validate_title(title)?;
        let now = Utc::now().to_rfc3339();
        {
            let conn = self.lock()?;
            let changed = conn.execute(
                "UPDATE artwork SET title = ?1, updated_at = ?2 WHERE id = ?3",
                params![title, now, artwork_id],
            )?;
            if changed == 0 {
                return Err(AppError::Message(format!(
                    "Artwork {artwork_id} was not found"
                )));
            }
        }
        if self.artwork_manifest_path(artwork_id)?.is_some() {
            self.ensure_artwork_manifest(artwork_id)?;
        }
        self.artwork_detail(artwork_id)
    }

    pub fn preview_rename_artwork_file_item(
        &self,
        asset_kind: AssetKind,
        asset_id: i64,
        name: &str,
    ) -> Result<FileRenamePlan> {
        let file_name = validate_file_name_component(name)?;
        let plan = match asset_kind {
            AssetKind::File => {
                let asset = self.file_asset(asset_id)?;
                let new_path = asset.current_path.with_file_name(&file_name);
                FileRenamePlan {
                    asset_kind,
                    asset_id,
                    artwork_id: asset.artwork_id,
                    current_path: asset.current_path,
                    new_path,
                    new_file_name: file_name,
                    physical_file_rename: true,
                }
            }
            AssetKind::Derived => {
                let asset = self.derived_asset(asset_id)?;
                let new_path = asset.path.with_file_name(&file_name);
                FileRenamePlan {
                    asset_kind,
                    asset_id,
                    artwork_id: asset.artwork_id,
                    current_path: asset.path,
                    new_path,
                    new_file_name: file_name,
                    physical_file_rename: true,
                }
            }
        };
        validate_rename_paths(&plan.current_path, &plan.new_path)?;
        Ok(plan)
    }

    pub fn execute_file_rename(&self, execution: FileRenameExecution) -> Result<FileRenameResult> {
        let plan = execution.plan;
        if plan.physical_file_rename && !execution.confirmed_physical_file_rename {
            return Err(AppError::Message(
                "Physical file rename requires explicit confirmation".to_string(),
            ));
        }
        self.revalidate_file_rename_plan(&plan)?;
        if plan.current_path == plan.new_path {
            let detail = self.artwork_detail(plan.artwork_id)?;
            return Ok(FileRenameResult {
                detail,
                plan,
                renamed: false,
                rolled_back: false,
            });
        }

        rename_existing_path(&plan.current_path, &plan.new_path)?;
        match self.apply_file_rename_plan(&plan) {
            Ok(detail) => Ok(FileRenameResult {
                detail,
                plan,
                renamed: true,
                rolled_back: false,
            }),
            Err(error) => {
                let rolled_back = rollback_renamed_path(&plan.current_path, &plan.new_path);
                let _ = self.restore_catalog_rename_path(&plan);
                let message = if rolled_back {
                    format!("Rename failed after file move and was rolled back: {error}")
                } else {
                    format!("Rename failed after file move; rollback failed or was not possible: {error}")
                };
                let _ = self.write_file_rename_log(&plan, "failure", Some(&message));
                Err(error)
            }
        }
    }

    fn revalidate_file_rename_plan(&self, plan: &FileRenamePlan) -> Result<()> {
        let current_asset_path = match plan.asset_kind {
            AssetKind::File => {
                let asset = self.file_asset(plan.asset_id)?;
                if asset.artwork_id != plan.artwork_id {
                    return Err(AppError::Message(
                        "Rename plan no longer matches the file asset artwork".to_string(),
                    ));
                }
                let expected_path = asset.current_path.with_file_name(&plan.new_file_name);
                if expected_path != plan.new_path {
                    return Err(AppError::Message(
                        "Rename plan destination no longer matches the current file asset"
                            .to_string(),
                    ));
                }
                asset.current_path
            }
            AssetKind::Derived => {
                let asset = self.derived_asset(plan.asset_id)?;
                if asset.artwork_id != plan.artwork_id {
                    return Err(AppError::Message(
                        "Rename plan no longer matches the derived asset artwork".to_string(),
                    ));
                }
                let expected_path = asset.path.with_file_name(&plan.new_file_name);
                if expected_path != plan.new_path {
                    return Err(AppError::Message(
                        "Rename plan destination no longer matches the current derived asset"
                            .to_string(),
                    ));
                }
                asset.path
            }
        };
        if current_asset_path != plan.current_path {
            return Err(AppError::Message(
                "Rename plan source no longer matches the catalog path".to_string(),
            ));
        }
        validate_rename_paths(&plan.current_path, &plan.new_path)
    }

    fn apply_file_rename_plan(&self, plan: &FileRenamePlan) -> Result<ArtworkDetail> {
        match plan.asset_kind {
            AssetKind::File => {
                self.update_file_asset_path(plan.asset_id, &plan.new_path)?;
            }
            AssetKind::Derived => {
                let asset = self.derived_asset(plan.asset_id)?;
                let format = plan
                    .new_path
                    .extension()
                    .and_then(|extension| extension.to_str())
                    .filter(|extension| !extension.is_empty())
                    .map(|extension| extension.to_ascii_lowercase())
                    .unwrap_or(asset.format);
                self.update_derived_asset_path(plan.asset_id, &plan.new_path, &format)?;
            }
        }
        if self.artwork_manifest_path(plan.artwork_id)?.is_some() {
            self.ensure_artwork_manifest(plan.artwork_id)?;
        }
        self.write_file_rename_log(plan, "success", Some("Renamed from Collection Explorer"))?;
        self.artwork_detail(plan.artwork_id)
    }

    fn restore_catalog_rename_path(&self, plan: &FileRenamePlan) -> Result<()> {
        match plan.asset_kind {
            AssetKind::File => self.update_file_asset_path(plan.asset_id, &plan.current_path),
            AssetKind::Derived => {
                let format = plan
                    .current_path
                    .extension()
                    .and_then(|extension| extension.to_str())
                    .filter(|extension| !extension.is_empty())
                    .map(|extension| extension.to_ascii_lowercase())
                    .unwrap_or_default();
                self.update_derived_asset_path(plan.asset_id, &plan.current_path, &format)
            }
        }
    }

    fn write_file_rename_log(
        &self,
        plan: &FileRenamePlan,
        result: &str,
        message: Option<&str>,
    ) -> Result<()> {
        self.write_operation_log(
            plan.artwork_id,
            (plan.asset_kind == AssetKind::File).then_some(plan.asset_id),
            &plan.current_path,
            &plan.new_path,
            result,
            message,
        )
    }

    pub fn delete_artwork_file_item(
        &self,
        asset_kind: AssetKind,
        asset_id: i64,
    ) -> Result<DeleteArtworkFileResult> {
        self.delete_artwork_file_item_with_trash(asset_kind, asset_id, move_path_to_trash)
    }

    pub fn delete_artwork_file_item_with_trash<F>(
        &self,
        asset_kind: AssetKind,
        asset_id: i64,
        mut trash_file: F,
    ) -> Result<DeleteArtworkFileResult>
    where
        F: FnMut(&Path) -> std::result::Result<(), String>,
    {
        let mut file_candidate = None;
        let mut derived_candidate = None;
        let mut result = DeleteResult::default();
        let artwork_id: i64 = match asset_kind {
            AssetKind::File => {
                let asset = self.file_asset(asset_id)?;
                let artwork_id = asset.artwork_id;
                file_candidate = self.delete_candidate_for_file_asset(&asset)?;
                if let Some(candidate) = file_candidate.as_ref() {
                    trash_delete_candidate_if_exists(candidate, &mut trash_file, &mut result);
                }
                if !result.trash_failures.is_empty() {
                    return Ok(DeleteArtworkFileResult {
                        detail: self.artwork_detail(artwork_id)?,
                        result,
                    });
                }
                let conn = self.lock()?;
                conn.execute("DELETE FROM file_asset WHERE id = ?1", params![asset_id])?;
                let has_primary = conn.query_row(
                    "SELECT EXISTS(SELECT 1 FROM file_asset WHERE artwork_id = ?1 AND is_primary = 1)",
                    params![artwork_id],
                    |row| row.get::<_, i64>(0),
                )? == 1;
                if !has_primary {
                    conn.execute(
                        "UPDATE file_asset SET is_primary = 1
                         WHERE id = (
                           SELECT id FROM file_asset
                           WHERE artwork_id = ?1
                           ORDER BY id
                           LIMIT 1
                         )",
                        params![artwork_id],
                    )?;
                }
                artwork_id
            }
            AssetKind::Derived => {
                let asset = self.derived_asset(asset_id)?;
                let artwork_id = asset.artwork_id;
                derived_candidate = delete_candidate_for_derived_asset(&asset);
                if let Some(candidate) = derived_candidate.as_ref() {
                    trash_delete_candidate_if_exists(candidate, &mut trash_file, &mut result);
                }
                if !result.trash_failures.is_empty() {
                    return Ok(DeleteArtworkFileResult {
                        detail: self.artwork_detail(artwork_id)?,
                        result,
                    });
                }
                let conn = self.lock()?;
                conn.execute("DELETE FROM derived_asset WHERE id = ?1", params![asset_id])?;
                artwork_id
            }
        };
        let _ = (file_candidate, derived_candidate);
        if self.artwork_manifest_path(artwork_id)?.is_some() {
            self.ensure_artwork_manifest(artwork_id)?;
        }
        Ok(DeleteArtworkFileResult {
            detail: self.artwork_detail(artwork_id)?,
            result,
        })
    }

    pub fn list_artworks(&self) -> Result<Vec<ArtworkSummary>> {
        let conn = self.lock()?;
        let mut statement = conn.prepare(
            r#"
            SELECT
              a.id,
              a.canonical_id,
              a.artwork_stable_id,
              (
                SELECT el.external_id
                FROM external_link el
                WHERE el.artwork_id = a.id AND el.link_type = 'caf'
                LIMIT 1
              ) AS caf_artwork_id,
              (
                SELECT el.external_id
                FROM external_link el
                WHERE el.artwork_id = a.id AND el.link_type = 'snikt'
                LIMIT 1
              ) AS snikt_artwork_id,
              (
                SELECT el.external_id
                FROM external_link el
                WHERE el.artwork_id = a.id AND el.link_type = 'raremarq'
                LIMIT 1
              ) AS raremarq_artwork_id,
              a.title,
              a.media,
              a.format,
              a.source_folder,
              (
                SELECT da.path
                FROM derived_asset da
                LEFT JOIN file_asset fa ON fa.id = da.source_file_asset_id
                WHERE da.artwork_id = a.id AND da.derivative_type = 'thumbnail'
                ORDER BY COALESCE(fa.display_order, 999999), fa.is_primary DESC, da.id
                LIMIT 1
              ) AS thumbnail_path,
              (
                SELECT COUNT(*)
                FROM file_asset f
                WHERE f.artwork_id = a.id
              ) + (
                SELECT COUNT(*)
                FROM derived_asset export
                WHERE export.artwork_id = a.id AND export.derivative_type = 'png_export'
              ) AS file_count,
              a.artwork_manifest_path
            FROM artwork a
            ORDER BY a.canonical_id
            "#,
        )?;
        let rows = statement.query_map([], artwork_from_row)?;
        let mut summaries = rows
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(AppError::from)?;
        for summary in &mut summaries {
            summary.gallery_ids = self.gallery_ids_for_artwork_locked(&conn, summary.id)?;
            summary.gallery_names = self.gallery_names_for_artwork_locked(&conn, summary.id)?;
            summary.artist_credits = self.artist_credits_locked(&conn, summary.id)?;
        }
        let preference = artwork_id_label_preference_locked(&conn)?;
        for summary in &mut summaries {
            apply_artwork_id_label_preference(summary, preference);
        }
        Ok(summaries)
    }

    pub fn artwork_detail(&self, artwork_id: i64) -> Result<ArtworkDetail> {
        let (detail, metadata_refreshed, manifest_path) = {
            let conn = self.lock()?;
            let mut detail = conn.query_row(
                r#"
                SELECT a.id, a.canonical_id, a.artwork_stable_id,
                       caf.external_id, snikt.external_id, raremarq.external_id, a.title,
                       a.description, a.for_sale_status,
                       a.media_type_id, a.media, a.art_type_id, a.format,
                       a.publication_status_id, a.active, a.illustration_exchange, a.ix_for_sale,
                       a.source_folder, a.caf_csv_image_link, a.caf_csv_added_to_caf,
                       a.snikt_csv_created_date,
                       caf.url, snikt.url, raremarq.url, generic.url,
                       pm.purchase_price, pm.estimated_value, pm.purchase_date, pm.provenance, pm.personal_notes
                FROM artwork a
                LEFT JOIN external_link caf ON caf.artwork_id = a.id AND caf.link_type = 'caf'
                LEFT JOIN external_link snikt ON snikt.artwork_id = a.id AND snikt.link_type = 'snikt'
                LEFT JOIN external_link raremarq ON raremarq.artwork_id = a.id AND raremarq.link_type = 'raremarq'
                LEFT JOIN external_link generic ON generic.artwork_id = a.id AND generic.link_type = 'generic'
                LEFT JOIN private_metadata pm ON pm.artwork_id = a.id
                WHERE a.id = ?1
                "#,
                params![artwork_id],
                |row| {
                    let canonical_id: String = row.get(1)?;
                    let stable_id: Option<String> = row.get(2)?;
                    let caf_artwork_id: Option<String> = row.get(3)?;
                    let snikt_artwork_id: Option<String> = row.get(4)?;
                    let raremarq_artwork_id: Option<String> = row.get(5)?;
                    let display_id = default_oac_display_id(&canonical_id, stable_id.as_deref());
                    Ok(ArtworkDetail {
                        id: row.get(0)?,
                        canonical_id,
                        display_id,
                        caf_artwork_id,
                        snikt_artwork_id,
                        raremarq_artwork_id,
                        title: row.get(6)?,
                        description: row.get(7)?,
                        for_sale_status: row.get(8)?,
                        media_type_id: row.get(9)?,
                        media: row.get(10)?,
                        art_type_id: row.get(11)?,
                        format: row.get(12)?,
                        publication_status_id: row.get(13)?,
                        active: row.get::<_, i64>(14)? == 1,
                        illustration_exchange: row.get::<_, i64>(15)? == 1,
                        ix_for_sale: row.get::<_, i64>(16)? == 1,
                        source_folder: PathBuf::from(row.get::<_, String>(17)?),
                        caf_csv_image_link: row.get(18)?,
                        caf_csv_added_to_caf: row.get(19)?,
                        snikt_csv_created_date: row.get(20)?,
                        caf_url: row.get(21)?,
                        snikt_url: row.get(22)?,
                        raremarq_url: row.get(23)?,
                        generic_url: row.get(24)?,
                        snikt_metadata: SniktMetadata::default(),
                        purchase_price: row.get(25)?,
                        estimated_value: row.get(26)?,
                        purchase_date: row.get(27)?,
                        provenance: row.get(28)?,
                        personal_notes: row.get(29)?,
                        artist_credits: Vec::new(),
                        file_assets: Vec::new(),
                        derived_assets: Vec::new(),
                        cache_warnings: Vec::new(),
                    })
                },
            )?;

            detail.artist_credits = self.artist_credits_locked(&conn, artwork_id)?;
            detail.snikt_metadata = self.snikt_metadata_locked(&conn, artwork_id)?;
            let (file_assets, metadata_refreshed) = self.file_assets_locked(&conn, artwork_id)?;
            detail.file_assets = file_assets;
            detail.derived_assets = self.derived_assets_locked(&conn, artwork_id)?;
            let preference = artwork_id_label_preference_locked(&conn)?;
            detail.display_id = display_id_for(
                &detail.display_id,
                detail.caf_artwork_id.as_deref(),
                detail.snikt_artwork_id.as_deref(),
                detail.raremarq_artwork_id.as_deref(),
                preference,
            );
            let manifest_path = self.artwork_manifest_path_locked(&conn, artwork_id)?;
            (detail, metadata_refreshed, manifest_path)
        };

        if metadata_refreshed {
            if let Some(manifest_path) = manifest_path {
                let asset_folder = manifest_path
                    .parent()
                    .unwrap_or(&manifest_path)
                    .to_path_buf();
                let manifest = artwork_manifest_from_detail(&detail, &asset_folder);
                write_json_manifest(&manifest_path, &manifest)?;
            }
        }
        Ok(detail)
    }

    pub fn snikt_upload_prefill_url(&self, artwork_id: i64) -> Result<String> {
        let detail = self.artwork_detail(artwork_id)?;
        Ok(snikt_upload_prefill_url(&detail))
    }

    pub(crate) fn update_caf_csv_tracking(
        &self,
        artwork_id: i64,
        image_link: Option<&str>,
        added_to_caf: Option<&str>,
    ) -> Result<()> {
        let image_link = normalize_optional(image_link);
        let added_to_caf = normalize_optional(added_to_caf);
        let conn = self.lock()?;
        conn.execute(
            "UPDATE artwork
             SET caf_csv_image_link = ?1, caf_csv_added_to_caf = ?2
             WHERE id = ?3",
            params![image_link.as_deref(), added_to_caf.as_deref(), artwork_id],
        )?;
        Ok(())
    }

    pub(crate) fn update_snikt_csv_tracking(
        &self,
        artwork_id: i64,
        created_date: Option<&str>,
    ) -> Result<()> {
        let created_date = normalize_optional(created_date);
        let conn = self.lock()?;
        conn.execute(
            "UPDATE artwork
             SET snikt_csv_created_date = ?1
             WHERE id = ?2",
            params![created_date.as_deref(), artwork_id],
        )?;
        Ok(())
    }

    pub fn file_asset(&self, file_asset_id: i64) -> Result<FileAsset> {
        let conn = self.lock()?;
        conn.query_row(
            "SELECT id, artwork_id, original_path, current_path, relative_path, file_name, extension, size_bytes, width, height, dpi_x, dpi_y, image_role, source_kind, is_primary
             FROM file_asset WHERE id = ?1",
            params![file_asset_id],
            |row| {
                Ok(FileAsset {
                    id: row.get(0)?,
                    artwork_id: row.get(1)?,
                    original_path: PathBuf::from(row.get::<_, String>(2)?),
                    current_path: PathBuf::from(row.get::<_, String>(3)?),
                    relative_path: row.get(4)?,
                    file_name: row.get(5)?,
                    extension: row.get(6)?,
                    size_bytes: row.get(7)?,
                    width: row.get(8)?,
                    height: row.get(9)?,
                    dpi_x: row.get(10)?,
                    dpi_y: row.get(11)?,
                    image_role: normalize_stored_image_role(row.get(12)?),
                    source_kind: row.get(13)?,
                    is_primary: row.get::<_, i64>(14)? == 1,
                })
            },
        )
        .map_err(AppError::from)
    }

    pub fn derived_asset(&self, derived_asset_id: i64) -> Result<DerivedAsset> {
        let conn = self.lock()?;
        conn.query_row(
            "SELECT id, artwork_id, source_file_asset_id, derivative_type, format, path, width, height, image_role
             FROM derived_asset WHERE id = ?1",
            params![derived_asset_id],
            |row| {
                Ok(DerivedAsset {
                    id: row.get(0)?,
                    artwork_id: row.get(1)?,
                    source_file_asset_id: row.get(2)?,
                    derivative_type: row.get(3)?,
                    format: row.get(4)?,
                    path: PathBuf::from(row.get::<_, String>(5)?),
                    width: row.get(6)?,
                    height: row.get(7)?,
                    image_role: normalize_stored_image_role(row.get(8)?),
                })
            },
        )
        .map_err(AppError::from)
    }

    fn delete_candidate_for_file_asset(
        &self,
        asset: &FileAsset,
    ) -> Result<Option<DeleteFilePreview>> {
        if asset.source_kind == "linked" {
            return Ok(None);
        }
        let asset_folder = self.artwork_asset_folder(asset.artwork_id)?;
        if !path_is_at_or_under(&asset_folder, &asset.current_path) {
            return Ok(None);
        }
        Ok(Some(DeleteFilePreview {
            path: asset.current_path.clone(),
            label: asset.file_name.clone(),
            reason: file_source_kind_delete_reason(&asset.source_kind).to_string(),
        }))
    }

    pub fn save_metadata(&self, update: MetadataUpdate) -> Result<()> {
        self.save_metadata_with_manifest_rewrite(update, true, false)
    }

    fn save_metadata_session_only(&self, update: MetadataUpdate) -> Result<()> {
        self.save_metadata_with_manifest_rewrite(update, false, false)
    }

    fn save_metadata_with_manifest_rewrite(
        &self,
        update: MetadataUpdate,
        rewrite_artwork_manifest: bool,
        rewrite_gallery_manifests: bool,
    ) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        let title = validate_title(&update.title)?;
        validate_description(update.description.as_deref())?;
        let purchase_date = validate_purchase_date(update.purchase_date.as_deref())?;
        let caf_url = normalize_optional(update.caf_url.as_deref());
        let caf_artwork_id = caf_url.as_deref().and_then(caf_piece_id_from_url);
        let snikt_url = normalize_optional(update.snikt_url.as_deref());
        let snikt_artwork_id = snikt_url.as_deref().and_then(snikt_image_id_from_url);
        let raremarq_url = normalize_optional(update.raremarq_url.as_deref());
        let raremarq_artwork_id = raremarq_url
            .as_deref()
            .and_then(raremarq_piece_slug_from_url);
        let generic_url = normalize_optional(update.generic_url.as_deref());
        let for_sale_status = normalize_optional(update.for_sale_status.as_deref())
            .unwrap_or_else(|| "NFS".to_string());
        let media_type_id = normalize_controlled_id(
            update.media_type_id.as_deref(),
            "7",
            MEDIA_TYPE_OPTIONS,
            "media type",
        )?;
        let media = controlled_label(MEDIA_TYPE_OPTIONS, &media_type_id)
            .expect("validated media id")
            .to_string();
        let art_type_id = normalize_controlled_id(
            update.art_type_id.as_deref(),
            "3",
            ART_TYPE_OPTIONS,
            "artwork type",
        )?;
        let format = controlled_label(ART_TYPE_OPTIONS, &art_type_id)
            .expect("validated artwork type id")
            .to_string();
        let publication_status_id = normalize_controlled_id(
            update.publication_status_id.as_deref(),
            "2",
            PUBLICATION_STATUS_OPTIONS,
            "publication status",
        )?;
        let illustration_exchange = update.illustration_exchange || update.ix_for_sale;
        let artist_credits = validated_artist_credits(&update.artist_credits)?;
        let conn = self.lock()?;
        conn.execute(
            "UPDATE artwork SET
               title = ?1,
               description = ?2,
               for_sale_status = ?3,
               media_type_id = ?4,
               media = ?5,
               art_type_id = ?6,
               format = ?7,
               publication_status_id = ?8,
               active = ?9,
               illustration_exchange = ?10,
               ix_for_sale = ?11,
               updated_at = ?12
             WHERE id = ?13",
            params![
                title,
                normalize_optional(update.description.as_deref()),
                for_sale_status,
                media_type_id,
                media,
                art_type_id,
                format,
                publication_status_id,
                update.active as i64,
                illustration_exchange as i64,
                update.ix_for_sale as i64,
                now,
                update.artwork_id
            ],
        )?;

        conn.execute(
            "INSERT OR IGNORE INTO term_media (value) VALUES (?1)",
            params![
                controlled_label(MEDIA_TYPE_OPTIONS, &media_type_id).expect("validated media id")
            ],
        )?;
        conn.execute(
            "INSERT OR IGNORE INTO term_format (value) VALUES (?1)",
            params![controlled_label(ART_TYPE_OPTIONS, &art_type_id)
                .expect("validated artwork type id")],
        )?;

        conn.execute(
            "DELETE FROM artwork_artist WHERE artwork_id = ?1",
            params![update.artwork_id],
        )?;
        for (sort_order, credit) in artist_credits.iter().enumerate() {
            conn.execute(
                "INSERT OR IGNORE INTO artist (name) VALUES (?1)",
                params![credit.name],
            )?;
            let artist_id: i64 = conn.query_row(
                "SELECT id FROM artist WHERE name = ?1",
                params![credit.name],
                |row| row.get(0),
            )?;
            conn.execute(
                "INSERT OR REPLACE INTO artwork_artist
                   (artwork_id, artist_id, role, sort_order, first_name, last_name, role_id)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    update.artwork_id,
                    artist_id,
                    credit.role.as_deref(),
                    sort_order as i64,
                    credit.first_name.as_deref(),
                    credit.last_name.as_deref(),
                    credit.role_id.as_deref()
                ],
            )?;
        }

        if let Some(url) = caf_url {
            upsert_external_link_locked(
                &conn,
                update.artwork_id,
                "caf",
                caf_artwork_id.as_deref(),
                &url,
            )?;
        } else {
            conn.execute(
                "DELETE FROM external_link WHERE artwork_id = ?1 AND link_type = 'caf'",
                params![update.artwork_id],
            )?;
        }
        if let Some(url) = snikt_url {
            upsert_external_link_locked(
                &conn,
                update.artwork_id,
                "snikt",
                snikt_artwork_id.as_deref(),
                &url,
            )?;
        } else {
            conn.execute(
                "DELETE FROM external_link WHERE artwork_id = ?1 AND link_type = 'snikt'",
                params![update.artwork_id],
            )?;
        }
        if let Some(url) = raremarq_url {
            upsert_external_link_locked(
                &conn,
                update.artwork_id,
                "raremarq",
                raremarq_artwork_id.as_deref(),
                &url,
            )?;
        } else {
            conn.execute(
                "DELETE FROM external_link WHERE artwork_id = ?1 AND link_type = 'raremarq'",
                params![update.artwork_id],
            )?;
        }
        if let Some(url) = generic_url {
            upsert_external_link_locked(&conn, update.artwork_id, "generic", None, &url)?;
        } else {
            conn.execute(
                "DELETE FROM external_link WHERE artwork_id = ?1 AND link_type = 'generic'",
                params![update.artwork_id],
            )?;
        }

        conn.execute(
            "INSERT INTO private_metadata
               (artwork_id, purchase_price, estimated_value, purchase_date, provenance, personal_notes)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(artwork_id) DO UPDATE SET
               purchase_price = excluded.purchase_price,
               estimated_value = excluded.estimated_value,
               purchase_date = excluded.purchase_date,
               provenance = excluded.provenance,
               personal_notes = excluded.personal_notes",
            params![
                update.artwork_id,
                normalize_optional(update.purchase_price.as_deref()),
                normalize_optional(update.estimated_value.as_deref()),
                purchase_date,
                normalize_optional(update.provenance.as_deref()),
                normalize_optional(update.personal_notes.as_deref())
            ],
        )?;
        if let Some(snikt_metadata) = update.snikt_metadata {
            conn.execute(
                "INSERT INTO snikt_metadata
                   (artwork_id, art_type, comic_publisher, series_title, issue_number,
                    series_page_number, year, character, subcategory, animation_studio,
                    episode_number, episode_title, published_date, strip_title,
                    is_sunday_strip, other, tags, is_nsfw, is_for_sale, price,
                    is_open_to_offers)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13,
                         ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21)
                 ON CONFLICT(artwork_id) DO UPDATE SET
                   art_type = excluded.art_type,
                   comic_publisher = excluded.comic_publisher,
                   series_title = excluded.series_title,
                   issue_number = excluded.issue_number,
                   series_page_number = excluded.series_page_number,
                   year = excluded.year,
                   character = excluded.character,
                   subcategory = excluded.subcategory,
                   animation_studio = excluded.animation_studio,
                   episode_number = excluded.episode_number,
                   episode_title = excluded.episode_title,
                   published_date = excluded.published_date,
                   strip_title = excluded.strip_title,
                   is_sunday_strip = excluded.is_sunday_strip,
                   other = excluded.other,
                   tags = excluded.tags,
                   is_nsfw = excluded.is_nsfw,
                   is_for_sale = excluded.is_for_sale,
                   price = excluded.price,
                   is_open_to_offers = excluded.is_open_to_offers",
                params![
                    update.artwork_id,
                    normalize_optional(snikt_metadata.art_type.as_deref()),
                    normalize_optional(snikt_metadata.comic_publisher.as_deref()),
                    normalize_optional(snikt_metadata.series_title.as_deref()),
                    normalize_optional(snikt_metadata.issue_number.as_deref()),
                    normalize_optional(snikt_metadata.series_page_number.as_deref()),
                    normalize_optional(snikt_metadata.year.as_deref()),
                    normalize_optional(snikt_metadata.character.as_deref()),
                    normalize_optional(snikt_metadata.subcategory.as_deref()),
                    normalize_optional(snikt_metadata.animation_studio.as_deref()),
                    normalize_optional(snikt_metadata.episode_number.as_deref()),
                    normalize_optional(snikt_metadata.episode_title.as_deref()),
                    normalize_optional(snikt_metadata.published_date.as_deref()),
                    normalize_optional(snikt_metadata.strip_title.as_deref()),
                    snikt_metadata.is_sunday_strip as i64,
                    normalize_optional(snikt_metadata.other.as_deref()),
                    normalize_optional(snikt_metadata.tags.as_deref()),
                    snikt_metadata.is_nsfw as i64,
                    snikt_metadata.is_for_sale as i64,
                    normalize_optional(snikt_metadata.price.as_deref()),
                    snikt_metadata.is_open_to_offers as i64
                ],
            )?;
        }
        let artwork_id = update.artwork_id;
        drop(conn);
        if rewrite_artwork_manifest && self.artwork_manifest_path(artwork_id)?.is_some() {
            self.ensure_artwork_manifest(artwork_id)?;
        }
        if rewrite_gallery_manifests {
            for gallery in self.galleries_for_artwork(artwork_id)? {
                self.rewrite_gallery_manifest(gallery.id)?;
            }
        }
        Ok(())
    }

    pub fn update_file_asset_path(&self, file_asset_id: i64, new_path: &Path) -> Result<()> {
        let asset = self.file_asset(file_asset_id)?;
        let artwork_folder = self.artwork_asset_folder(asset.artwork_id)?;
        let relative_path = if let Ok(relative) = new_path.strip_prefix(&artwork_folder) {
            relative.to_string_lossy().to_string()
        } else {
            new_path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default()
                .to_string()
        };
        let file_name = new_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_string();
        let extension = new_path
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        let path = new_path.to_string_lossy().to_string();
        let conn = self.lock()?;
        conn.execute(
            "UPDATE file_asset SET current_path = ?1, relative_path = ?2, file_name = ?3, extension = ?4 WHERE id = ?5",
            params![path, relative_path, file_name, extension, file_asset_id],
        )?;
        Ok(())
    }

    fn update_derived_asset_path(
        &self,
        derived_asset_id: i64,
        new_path: &Path,
        format: &str,
    ) -> Result<()> {
        let path = new_path.to_string_lossy().to_string();
        let conn = self.lock()?;
        conn.execute(
            "UPDATE derived_asset SET path = ?1, format = ?2 WHERE id = ?3",
            params![path, format, derived_asset_id],
        )?;
        Ok(())
    }

    pub fn write_operation_log(
        &self,
        artwork_id: i64,
        file_asset_id: Option<i64>,
        old_path: &Path,
        new_path: &Path,
        result: &str,
        message: Option<&str>,
    ) -> Result<()> {
        let conn = self.lock()?;
        conn.execute(
            "INSERT INTO file_operation_log
             (artwork_id, file_asset_id, old_path, new_path, result, message, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                artwork_id,
                file_asset_id,
                old_path.to_string_lossy().to_string(),
                new_path.to_string_lossy().to_string(),
                result,
                message,
                Utc::now().to_rfc3339()
            ],
        )?;
        Ok(())
    }

    pub fn operation_logs_for_artwork(&self, artwork_id: i64) -> Result<Vec<OperationLog>> {
        let conn = self.lock()?;
        let mut statement = conn.prepare(
            "SELECT id, artwork_id, file_asset_id, old_path, new_path, result, message, created_at
             FROM file_operation_log WHERE artwork_id = ?1 ORDER BY id",
        )?;
        let rows = statement.query_map(params![artwork_id], |row| {
            Ok(OperationLog {
                id: row.get(0)?,
                artwork_id: row.get(1)?,
                file_asset_id: row.get(2)?,
                old_path: PathBuf::from(row.get::<_, String>(3)?),
                new_path: PathBuf::from(row.get::<_, String>(4)?),
                result: row.get(5)?,
                message: row.get(6)?,
                created_at: row.get(7)?,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(AppError::from)
    }

    pub fn term_suggestions(&self) -> Result<(Vec<String>, Vec<String>)> {
        let conn = self.lock()?;
        let media = collect_string_column(&conn, "SELECT value FROM term_media ORDER BY value")?;
        let formats = collect_string_column(&conn, "SELECT value FROM term_format ORDER BY value")?;
        Ok((media, formats))
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, Connection>> {
        self.conn
            .lock()
            .map_err(|_| AppError::Message("Catalog database lock is poisoned".to_string()))
    }

    fn artist_credits_locked(
        &self,
        conn: &Connection,
        artwork_id: i64,
    ) -> Result<Vec<ArtistCredit>> {
        let mut statement = conn.prepare(
            "SELECT ar.name, aa.role, aa.first_name, aa.last_name, aa.role_id
             FROM artwork_artist aa
             JOIN artist ar ON ar.id = aa.artist_id
             WHERE aa.artwork_id = ?1
             ORDER BY aa.sort_order, ar.name",
        )?;
        let rows = statement.query_map(params![artwork_id], |row| {
            Ok(ArtistCredit {
                name: row.get(0)?,
                role: row.get(1)?,
                first_name: row.get(2)?,
                last_name: row.get(3)?,
                role_id: row.get(4)?,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(AppError::from)
    }

    fn gallery_names_for_artwork_locked(
        &self,
        conn: &Connection,
        artwork_id: i64,
    ) -> Result<Vec<String>> {
        collect_string_column_with_param(
            conn,
            "SELECT g.name
             FROM gallery_artwork ga
             JOIN gallery g ON g.id = ga.gallery_id
             WHERE ga.artwork_id = ?1
             ORDER BY ga.sort_order, g.id",
            artwork_id,
        )
    }

    fn gallery_ids_for_artwork_locked(
        &self,
        conn: &Connection,
        artwork_id: i64,
    ) -> Result<Vec<i64>> {
        collect_i64_column_with_param(
            conn,
            "SELECT ga.gallery_id
             FROM gallery_artwork ga
             WHERE ga.artwork_id = ?1
             ORDER BY ga.sort_order, ga.gallery_id",
            artwork_id,
        )
    }

    fn snikt_metadata_locked(&self, conn: &Connection, artwork_id: i64) -> Result<SniktMetadata> {
        conn.query_row(
            "SELECT art_type, comic_publisher, series_title, issue_number, series_page_number,
                    year, character, subcategory, animation_studio, episode_number, episode_title,
                    published_date, strip_title, is_sunday_strip, other, tags, is_nsfw,
                    is_for_sale, price, is_open_to_offers
             FROM snikt_metadata
             WHERE artwork_id = ?1",
            params![artwork_id],
            |row| {
                Ok(SniktMetadata {
                    art_type: row.get(0)?,
                    comic_publisher: row.get(1)?,
                    series_title: row.get(2)?,
                    issue_number: row.get(3)?,
                    series_page_number: row.get(4)?,
                    year: row.get(5)?,
                    character: row.get(6)?,
                    subcategory: row.get(7)?,
                    animation_studio: row.get(8)?,
                    episode_number: row.get(9)?,
                    episode_title: row.get(10)?,
                    published_date: row.get(11)?,
                    strip_title: row.get(12)?,
                    is_sunday_strip: row.get::<_, i64>(13)? == 1,
                    other: row.get(14)?,
                    tags: row.get(15)?,
                    is_nsfw: row.get::<_, i64>(16)? == 1,
                    is_for_sale: row.get::<_, i64>(17)? == 1,
                    price: row.get(18)?,
                    is_open_to_offers: row.get::<_, i64>(19)? == 1,
                })
            },
        )
        .optional()
        .map(|metadata| metadata.unwrap_or_default())
        .map_err(AppError::from)
    }

    fn file_assets_locked(
        &self,
        conn: &Connection,
        artwork_id: i64,
    ) -> Result<(Vec<FileAsset>, bool)> {
        let mut assets = {
            let mut statement = conn.prepare(
                "SELECT id, artwork_id, original_path, current_path, relative_path, file_name, extension, size_bytes, width, height, dpi_x, dpi_y, metadata_checked_at, image_role, source_kind, is_primary
                 FROM file_asset WHERE artwork_id = ?1 ORDER BY display_order, is_primary DESC, id",
            )?;
            let rows = statement.query_map(params![artwork_id], |row| {
                Ok((
                    FileAsset {
                        id: row.get(0)?,
                        artwork_id: row.get(1)?,
                        original_path: PathBuf::from(row.get::<_, String>(2)?),
                        current_path: PathBuf::from(row.get::<_, String>(3)?),
                        relative_path: row.get(4)?,
                        file_name: row.get(5)?,
                        extension: row.get(6)?,
                        size_bytes: row.get(7)?,
                        width: row.get(8)?,
                        height: row.get(9)?,
                        dpi_x: row.get(10)?,
                        dpi_y: row.get(11)?,
                        image_role: normalize_stored_image_role(row.get(13)?),
                        source_kind: row.get(14)?,
                        is_primary: row.get::<_, i64>(15)? == 1,
                    },
                    row.get::<_, Option<String>>(12)?,
                ))
            })?;
            rows.collect::<std::result::Result<Vec<_>, _>>()
                .map_err(AppError::from)?
        };

        let mut refreshed = false;
        for (asset, metadata_checked_at) in &mut assets {
            if metadata_needs_refresh(asset, metadata_checked_at) && asset.current_path.is_file() {
                let metadata = match read_image_metadata(&asset.current_path) {
                    Ok(metadata) => metadata,
                    Err(_) => continue,
                };
                let metadata_changed = apply_image_metadata(asset, metadata);
                let next_checked_at = Utc::now().to_rfc3339();
                conn.execute(
                    "UPDATE file_asset
                     SET width = ?1, height = ?2, dpi_x = ?3, dpi_y = ?4, metadata_checked_at = ?5
                     WHERE id = ?6",
                    params![
                        asset.width,
                        asset.height,
                        asset.dpi_x,
                        asset.dpi_y,
                        next_checked_at,
                        asset.id
                    ],
                )?;
                *metadata_checked_at = Some(next_checked_at);
                if metadata_changed {
                    refreshed = true;
                }
            }
        }

        Ok((
            assets
                .into_iter()
                .map(|(asset, _metadata_checked_at)| asset)
                .collect(),
            refreshed,
        ))
    }

    fn derived_assets_locked(
        &self,
        conn: &Connection,
        artwork_id: i64,
    ) -> Result<Vec<DerivedAsset>> {
        let mut statement = conn.prepare(
            "SELECT id, artwork_id, source_file_asset_id, derivative_type, format, path, width, height, image_role
             FROM derived_asset WHERE artwork_id = ?1 ORDER BY id",
        )?;
        let rows = statement.query_map(params![artwork_id], |row| {
            Ok(DerivedAsset {
                id: row.get(0)?,
                artwork_id: row.get(1)?,
                source_file_asset_id: row.get(2)?,
                derivative_type: row.get(3)?,
                format: row.get(4)?,
                path: PathBuf::from(row.get::<_, String>(5)?),
                width: row.get(6)?,
                height: row.get(7)?,
                image_role: normalize_stored_image_role(row.get(8)?),
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(AppError::from)
    }
}

fn validate_artwork_merge_scope_locked(
    conn: &Connection,
    collection_id: i64,
    source_gallery_id: i64,
    source_artwork_id: i64,
    target_artwork_id: i64,
) -> Result<()> {
    let source_gallery_in_collection: i64 = conn.query_row(
        "SELECT COUNT(*) FROM collection_gallery WHERE collection_id = ?1 AND gallery_id = ?2",
        params![collection_id, source_gallery_id],
        |row| row.get(0),
    )?;
    if source_gallery_in_collection == 0 {
        return Err(AppError::Message(
            "Source Gallery is not in the selected Collection".to_string(),
        ));
    }
    let source_artwork_in_source_gallery: i64 = conn.query_row(
        "SELECT COUNT(*) FROM gallery_artwork WHERE gallery_id = ?1 AND artwork_id = ?2",
        params![source_gallery_id, source_artwork_id],
        |row| row.get(0),
    )?;
    if source_artwork_in_source_gallery == 0 {
        return Err(AppError::Message(
            "Source Artwork is not in the selected Gallery".to_string(),
        ));
    }
    for (artwork_id, label) in [
        (source_artwork_id, "Source Artwork"),
        (target_artwork_id, "Target Artwork"),
    ] {
        let linked_count: i64 = conn.query_row(
            "SELECT COUNT(DISTINCT ga.gallery_id)
             FROM gallery_artwork ga
             JOIN collection_gallery cg ON cg.gallery_id = ga.gallery_id
             WHERE cg.collection_id = ?1 AND ga.artwork_id = ?2",
            params![collection_id, artwork_id],
            |row| row.get(0),
        )?;
        if linked_count == 0 {
            return Err(AppError::Message(format!(
                "{label} is not in the selected Collection"
            )));
        }
    }
    Ok(())
}

fn metadata_needs_refresh(asset: &FileAsset, metadata_checked_at: &Option<String>) -> bool {
    metadata_checked_at.is_none()
        && (asset.width.is_none()
            || asset.height.is_none()
            || asset.dpi_x.is_none()
            || asset.dpi_y.is_none())
}

fn apply_image_metadata(asset: &mut FileAsset, metadata: ImageMetadata) -> bool {
    let width = Some(metadata.width);
    let height = Some(metadata.height);
    if asset.width == width
        && asset.height == height
        && asset.dpi_x == metadata.dpi_x
        && asset.dpi_y == metadata.dpi_y
    {
        return false;
    }
    asset.width = width;
    asset.height = height;
    asset.dpi_x = metadata.dpi_x;
    asset.dpi_y = metadata.dpi_y;
    true
}

fn artwork_manifest_from_detail(detail: &ArtworkDetail, asset_folder: &Path) -> ArtworkManifest {
    let mut files: Vec<ArtworkFileManifest> = detail
        .file_assets
        .iter()
        .map(local_file_object_for_file_asset)
        .collect();
    files.extend(
        detail
            .derived_assets
            .iter()
            .filter_map(|asset| local_file_object_for_derived_asset(asset, asset_folder)),
    );

    ArtworkManifest {
        schema_version: SCHEMA_VERSION.to_string(),
        id: detail.canonical_id.clone(),
        title: detail.title.clone(),
        external_links: artwork_external_links(detail),
        public_metadata: Some(ArtworkPublicMetadata {
            description: detail.description.clone(),
            for_sale_status: detail.for_sale_status.clone(),
            media: detail.media.clone(),
            artwork_type: detail.format.clone(),
            publication_status: match detail.publication_status_id.as_deref() {
                Some("1") => Some("published_art".to_string()),
                Some("2") => Some("unpublished_art".to_string()),
                _ => None,
            },
            is_public: Some(detail.active),
            artist_credits: detail
                .artist_credits
                .iter()
                .map(|credit| {
                    let mut extensions = BTreeMap::new();
                    if let Some(role_id) = credit.role_id.clone() {
                        extensions.insert(
                            "com.comicartfans".to_string(),
                            serde_json::json!({ "artist_job_id": role_id }),
                        );
                    }
                    ArtworkArtistCredit {
                        first_name: credit.first_name.clone(),
                        last_name: credit.last_name.clone(),
                        role: credit.role.clone(),
                        extensions,
                    }
                })
                .collect(),
            extensions: public_metadata_extensions(detail),
        }),
        private_metadata: local_private_metadata(detail),
        files,
        extensions: serde_json_object_map([("app.oa-curator", artwork_app_extension(detail))]),
    }
}

fn artwork_app_extension(detail: &ArtworkDetail) -> serde_json::Value {
    let mut app = serde_json::Map::new();
    app.insert("artwork_id".to_string(), serde_json::Value::from(detail.id));
    app.insert(
        "display_id".to_string(),
        serde_json::Value::from(detail.display_id.clone()),
    );
    if let Some(generic_url) = detail.generic_url.clone() {
        app.insert(
            "generic_url".to_string(),
            serde_json::Value::from(generic_url),
        );
    }
    serde_json::Value::Object(app)
}

fn local_private_metadata(detail: &ArtworkDetail) -> Option<ArtworkPrivateMetadata> {
    if detail.purchase_price.is_none()
        && detail.estimated_value.is_none()
        && detail.purchase_date.is_none()
        && detail.provenance.is_none()
        && detail.personal_notes.is_none()
    {
        return None;
    }

    Some(ArtworkPrivateMetadata {
        purchase_price: detail.purchase_price.clone(),
        estimated_value: detail.estimated_value.clone(),
        purchase_date: detail.purchase_date.clone(),
        provenance: detail.provenance.clone(),
        personal_notes: detail.personal_notes.clone(),
        extensions: BTreeMap::new(),
    })
}

fn merge_manifest_only_entries(manifest: &mut ArtworkManifest, existing: ArtworkManifest) {
    for file in existing.files {
        if !manifest.files.iter().any(|candidate| {
            candidate.id == file.id || candidate.relative_path == file.relative_path
        }) {
            manifest.files.push(file);
        }
    }
}

fn collection_external_links(collection: &CollectionSummary) -> Vec<ExternalLinkManifest> {
    let mut links = Vec::new();
    if let Some(id) = collection.caf_collection_id.clone() {
        links.push(ExternalLinkManifest {
            provider: "com.comicartfans".to_string(),
            id: id.clone(),
            url: format!("https://www.comicartfans.com/GalleryDetail.asp?GCat={id}"),
            extensions: BTreeMap::new(),
        });
    }
    if let Some(id) = collection.snikt_collection_id.clone() {
        links.push(ExternalLinkManifest {
            provider: "com.snikt".to_string(),
            id: id.clone(),
            url: format!("https://www.snikt.com/user/{id}"),
            extensions: BTreeMap::new(),
        });
    }
    if let Some(id) = collection.raremarq_collection_id.clone() {
        links.push(ExternalLinkManifest {
            provider: "com.raremarq".to_string(),
            id: id.clone(),
            url: format!("https://www.raremarq.com/u/{id}"),
            extensions: BTreeMap::new(),
        });
    }
    links
}

fn gallery_external_links(gallery: &GallerySummary) -> Vec<ExternalLinkManifest> {
    let mut links = Vec::new();
    if let Some(id) = gallery.caf_gallery_room_id.clone() {
        links.push(ExternalLinkManifest {
            provider: "com.comicartfans".to_string(),
            id: id.clone(),
            url: format!("https://www.comicartfans.com/my/GalleryRoom.asp?GSub={id}"),
            extensions: BTreeMap::new(),
        });
    }
    if let Some(id) = gallery.snikt_gallery_id.clone() {
        links.push(ExternalLinkManifest {
            provider: "com.snikt".to_string(),
            id,
            url: String::new(),
            extensions: BTreeMap::new(),
        });
    }
    if let Some(id) = gallery.raremarq_gallery_id.clone() {
        links.push(ExternalLinkManifest {
            provider: "com.raremarq".to_string(),
            id,
            url: String::new(),
            extensions: BTreeMap::new(),
        });
    }
    links
}

fn gallery_extensions(gallery: &GallerySummary) -> BTreeMap<String, serde_json::Value> {
    if gallery.snikt_gallery_inherits_collection {
        return BTreeMap::new();
    }
    serde_json_object_map([(
        "app.oa-curator",
        serde_json::json!({ "snikt_gallery_inherits_collection": false }),
    )])
}

fn artwork_external_links(detail: &ArtworkDetail) -> Vec<ExternalLinkManifest> {
    let mut links = Vec::new();
    push_provider_link(
        &mut links,
        "com.comicartfans",
        detail.caf_artwork_id.clone(),
        detail.caf_url.clone(),
    );
    push_provider_link(
        &mut links,
        "com.snikt",
        detail.snikt_artwork_id.clone(),
        detail.snikt_url.clone(),
    );
    push_provider_link(
        &mut links,
        "com.raremarq",
        detail.raremarq_artwork_id.clone(),
        detail.raremarq_url.clone(),
    );
    links
}

fn push_provider_link(
    links: &mut Vec<ExternalLinkManifest>,
    provider: &str,
    id: Option<String>,
    url: Option<String>,
) {
    let Some(id) = id else {
        return;
    };
    links.push(ExternalLinkManifest {
        provider: provider.to_string(),
        id,
        url: url.unwrap_or_default(),
        extensions: BTreeMap::new(),
    });
}

fn public_metadata_extensions(detail: &ArtworkDetail) -> BTreeMap<String, serde_json::Value> {
    let mut extensions = BTreeMap::new();
    let caf = caf_public_extension(detail);
    if !caf.is_null() {
        extensions.insert("com.comicartfans".to_string(), caf);
    }
    let snikt = snikt_public_extension(detail);
    if !snikt.is_null() {
        extensions.insert("com.snikt".to_string(), snikt);
    }
    extensions
}

fn caf_public_extension(detail: &ArtworkDetail) -> serde_json::Value {
    let mut map = serde_json::Map::new();
    if let Some(value) = detail.media_type_id.clone() {
        map.insert("media_type_id".to_string(), serde_json::Value::from(value));
    }
    if let Some(value) = detail.art_type_id.clone() {
        map.insert("art_type_id".to_string(), serde_json::Value::from(value));
    }
    if let Some(value) = detail.publication_status_id.clone() {
        map.insert(
            "publication_status_id".to_string(),
            serde_json::Value::from(value.clone()),
        );
        if value == "3" {
            map.insert(
                "publication_status".to_string(),
                serde_json::Value::from("CAF Member Art"),
            );
        }
    }
    if let Some(value) = detail.caf_csv_image_link.clone() {
        map.insert("csv_image_link".to_string(), serde_json::Value::from(value));
    }
    if let Some(value) = detail.caf_csv_added_to_caf.clone() {
        map.insert(
            "csv_added_to_caf".to_string(),
            serde_json::Value::from(value),
        );
    }
    map.insert(
        "illustration_exchange".to_string(),
        serde_json::Value::from(detail.illustration_exchange),
    );
    map.insert(
        "ix_for_sale".to_string(),
        serde_json::Value::from(detail.ix_for_sale),
    );
    serde_json::Value::Object(map)
}

fn snikt_public_extension(detail: &ArtworkDetail) -> serde_json::Value {
    let metadata = &detail.snikt_metadata;
    let mut map = serde_json::Map::new();
    if let Some(value) = detail.snikt_csv_created_date.clone() {
        map.insert(
            "csv_created_date".to_string(),
            serde_json::Value::from(value),
        );
    }
    insert_optional_string(&mut map, "art_type", metadata.art_type.clone());
    insert_optional_string(
        &mut map,
        "comic_publisher",
        metadata.comic_publisher.clone(),
    );
    insert_optional_string(&mut map, "series_title", metadata.series_title.clone());
    insert_optional_string(&mut map, "issue_number", metadata.issue_number.clone());
    insert_optional_string(
        &mut map,
        "series_page_number",
        metadata.series_page_number.clone(),
    );
    insert_optional_string(&mut map, "year", metadata.year.clone());
    insert_optional_string(&mut map, "character", metadata.character.clone());
    insert_optional_string(&mut map, "subcategory", metadata.subcategory.clone());
    insert_optional_string(
        &mut map,
        "animation_studio",
        metadata.animation_studio.clone(),
    );
    insert_optional_string(&mut map, "episode_number", metadata.episode_number.clone());
    insert_optional_string(&mut map, "episode_title", metadata.episode_title.clone());
    insert_optional_string(&mut map, "published_date", metadata.published_date.clone());
    insert_optional_string(&mut map, "strip_title", metadata.strip_title.clone());
    insert_optional_string(&mut map, "other", metadata.other.clone());
    insert_optional_string(&mut map, "tags", metadata.tags.clone());
    map.insert(
        "is_sunday_strip".to_string(),
        serde_json::Value::from(metadata.is_sunday_strip),
    );
    map.insert(
        "is_nsfw".to_string(),
        serde_json::Value::from(metadata.is_nsfw),
    );
    map.insert(
        "is_for_sale".to_string(),
        serde_json::Value::from(metadata.is_for_sale),
    );
    insert_optional_string(&mut map, "price", metadata.price.clone());
    map.insert(
        "is_open_to_offers".to_string(),
        serde_json::Value::from(metadata.is_open_to_offers),
    );
    if map
        .values()
        .all(|value| matches!(value, serde_json::Value::Bool(false)))
    {
        serde_json::Value::Null
    } else {
        serde_json::json!({ "metadata": map })
    }
}

fn insert_optional_string(
    map: &mut serde_json::Map<String, serde_json::Value>,
    key: &str,
    value: Option<String>,
) {
    if let Some(value) = value.filter(|value| !value.is_empty()) {
        map.insert(key.to_string(), serde_json::Value::from(value));
    }
}

fn local_file_object_for_file_asset(asset: &FileAsset) -> ArtworkFileManifest {
    let mut extensions = BTreeMap::new();
    let mut app = serde_json::Map::new();
    app.insert(
        "file_asset_id".to_string(),
        serde_json::Value::from(asset.id),
    );
    app.insert(
        "source_kind".to_string(),
        serde_json::Value::from(asset.source_kind.clone()),
    );
    extensions.insert("app.oa-curator".to_string(), serde_json::Value::Object(app));

    let image_role = portable_image_role(asset.image_role.as_deref(), &mut extensions);

    ArtworkFileManifest {
        id: format!("file-{}", asset.id),
        relative_path: asset.relative_path.clone(),
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
        external_links: Vec::new(),
        extensions,
    }
}

fn local_file_object_for_derived_asset(
    asset: &DerivedAsset,
    asset_folder: &Path,
) -> Option<ArtworkFileManifest> {
    if asset.derivative_type == "thumbnail" || asset.derivative_type == "preview" {
        return None;
    }
    let relative_path = asset
        .path
        .strip_prefix(asset_folder)
        .ok()?
        .to_string_lossy()
        .replace('\\', "/");

    let mut extensions = BTreeMap::new();
    let mut app = serde_json::Map::new();
    app.insert(
        "derived_asset_id".to_string(),
        serde_json::Value::from(asset.id),
    );
    app.insert(
        "derivative_type".to_string(),
        serde_json::Value::from(asset.derivative_type.clone()),
    );
    if let Some(source_file_asset_id) = asset.source_file_asset_id {
        app.insert(
            "source_file_asset_id".to_string(),
            serde_json::Value::from(source_file_asset_id),
        );
    }
    extensions.insert("app.oa-curator".to_string(), serde_json::Value::Object(app));

    let file_name = asset
        .path
        .file_name()
        .and_then(|name| name.to_str())
        .map(str::to_string);
    Some(ArtworkFileManifest {
        id: format!("derived-{}", asset.id),
        relative_path,
        file_kind: "derivative".to_string(),
        file_name,
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
    extensions: &mut BTreeMap<String, serde_json::Value>,
) -> Option<String> {
    match image_role {
        Some("basic") | Some("caf_basic") => {
            extensions.insert(
                "com.comicartfans".to_string(),
                serde_json::json!({ "format_tier": "basic" }),
            );
            None
        }
        Some("premium") | Some("caf_premium") => {
            extensions.insert(
                "com.comicartfans".to_string(),
                serde_json::json!({ "format_tier": "premium" }),
            );
            None
        }
        other => other.map(str::to_string),
    }
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
        "gif" => Some("image/gif"),
        "webp" => Some("image/webp"),
        "pdf" => Some("application/pdf"),
        "txt" => Some("text/plain"),
        _ => None,
    }
}

fn serde_json_object_map<const N: usize>(
    entries: [(&str, serde_json::Value); N],
) -> BTreeMap<String, serde_json::Value> {
    entries
        .into_iter()
        .map(|(key, value)| (key.to_string(), value))
        .collect()
}

fn provider_id(links: &[ExternalLinkManifest], provider: &str) -> Option<String> {
    links
        .iter()
        .find(|link| link.provider == provider)
        .map(|link| link.id.clone())
}

fn set_setting_locked(conn: &Connection, key: &str, value: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO app_setting (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )?;
    Ok(())
}

fn setting_or_default_locked(conn: &Connection, key: &str, default_value: &str) -> Result<String> {
    let value: Option<String> = conn
        .query_row(
            "SELECT value FROM app_setting WHERE key = ?1",
            params![key],
            |row| row.get(0),
        )
        .optional()?;
    Ok(value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| default_value.to_string()))
}

fn validate_setting_choice(value: &str, allowed_values: &[&str], label: &str) -> Result<String> {
    let normalized = value.trim().to_ascii_lowercase();
    if allowed_values.contains(&normalized.as_str()) {
        return Ok(normalized);
    }
    Err(AppError::Message(format!(
        "{label} must be one of: {}",
        allowed_values.join(", ")
    )))
}

fn normalize_png_export_variant_setting(value: &str) -> Result<String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "basic" | "caf_basic" => Ok("basic".to_string()),
        "premium" | "caf_premium" => Ok("premium".to_string()),
        _ => Err(AppError::Message(
            "Default PNG export must be one of: basic, premium".to_string(),
        )),
    }
}

fn recent_collections_locked(conn: &Connection) -> Result<Vec<RecentCollection>> {
    let json: Option<String> = conn
        .query_row(
            "SELECT value FROM app_setting WHERE key = ?1",
            params![RECENT_COLLECTIONS_SETTING],
            |row| row.get(0),
        )
        .optional()?;
    match json {
        Some(value) if !value.trim().is_empty() => Ok(serde_json::from_str(&value)?),
        _ => Ok(Vec::new()),
    }
}

fn env_flag(name: &str) -> bool {
    env::var_os(name).is_some_and(|value| {
        let value = value.to_string_lossy();
        !value.is_empty() && value != "0" && !value.eq_ignore_ascii_case("false")
    })
}

fn elapsed_ms(started: Instant) -> u128 {
    started.elapsed().as_millis()
}

fn provider_url(links: &[ExternalLinkManifest], provider: &str) -> Option<String> {
    links
        .iter()
        .find(|link| link.provider == provider)
        .map(|link| link.url.clone())
}

fn app_extension_string(
    extensions: &BTreeMap<String, serde_json::Value>,
    key: &str,
) -> Option<String> {
    extensions
        .get("app.oa-curator")
        .and_then(|extension| extension.get(key))
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
}

fn app_extension_bool(extensions: &BTreeMap<String, serde_json::Value>, key: &str) -> Option<bool> {
    extensions
        .get("app.oa-curator")
        .and_then(|extension| extension.get(key))
        .and_then(serde_json::Value::as_bool)
}

fn gallery_snikt_inherits_collection(manifest: &GalleryManifest) -> bool {
    if provider_id(&manifest.external_links, "com.snikt").is_some() {
        return false;
    }
    app_extension_bool(&manifest.extensions, "snikt_gallery_inherits_collection").unwrap_or(true)
}

fn oac_link_type_for_provider(provider: &str) -> &str {
    match provider {
        "com.comicartfans" => "caf",
        "com.snikt" => "snikt",
        "com.raremarq" => "raremarq",
        other => other,
    }
}

fn normalize_optional(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn controlled_label(options: &[(&'static str, &'static str)], id: &str) -> Option<&'static str> {
    let id = id.trim();
    options
        .iter()
        .find_map(|(candidate_id, label)| (*candidate_id == id).then_some(*label))
}

fn controlled_id_for_label(
    options: &[(&'static str, &'static str)],
    label: &str,
) -> Option<&'static str> {
    let label = label.trim();
    options.iter().find_map(|(id, candidate_label)| {
        candidate_label.eq_ignore_ascii_case(label).then_some(*id)
    })
}

fn normalize_controlled_id(
    value: Option<&str>,
    default_id: &str,
    options: &[(&'static str, &'static str)],
    field_label: &str,
) -> Result<String> {
    let id = normalize_optional(value).unwrap_or_else(|| default_id.to_string());
    if controlled_label(options, &id).is_none() {
        return Err(AppError::Message(format!(
            "Unsupported {field_label} ID: {id}"
        )));
    }
    Ok(id)
}

fn validate_workspace_name(value: &str, label: &str) -> Result<String> {
    let name = value.trim();
    if name.is_empty() {
        return Err(AppError::Message(format!("{label} is required")));
    }
    if contains_html_like_tag(name) {
        return Err(AppError::Message(format!(
            "{label} must not contain HTML tags"
        )));
    }
    Ok(name.to_string())
}

fn validate_rename_paths(current_path: &Path, new_path: &Path) -> Result<()> {
    if current_path == new_path {
        return Ok(());
    }
    if !current_path.exists() {
        return Err(AppError::Message(format!(
            "Cannot rename missing file: {}",
            current_path.to_string_lossy()
        )));
    }
    if new_path.exists() {
        return Err(AppError::Message(format!(
            "Cannot rename file because the destination already exists: {}",
            new_path.to_string_lossy()
        )));
    }
    Ok(())
}

fn rename_existing_path(current_path: &Path, new_path: &Path) -> Result<()> {
    validate_rename_paths(current_path, new_path)?;
    if current_path == new_path {
        return Ok(());
    }
    fs::rename(current_path, new_path)?;
    Ok(())
}

fn rollback_renamed_path(current_path: &Path, new_path: &Path) -> bool {
    if !new_path.exists() || current_path.exists() {
        return false;
    }
    fs::rename(new_path, current_path).is_ok()
}

fn validate_title(value: &str) -> Result<String> {
    let title = value.trim();
    if title.is_empty() {
        return Err(AppError::Message("Title is required".to_string()));
    }
    if title.chars().count() > 150 {
        return Err(AppError::Message(
            "Title must be 150 characters or fewer".to_string(),
        ));
    }
    if contains_html_like_tag(title) {
        return Err(AppError::Message(
            "Title must not contain HTML tags".to_string(),
        ));
    }
    if contains_for_sale_phrase(title) {
        return Err(AppError::Message(
            "Title must not contain the phrase For Sale".to_string(),
        ));
    }
    Ok(title.to_string())
}

fn validate_positive_integer_id(value: Option<&str>, label: &str) -> Result<Option<String>> {
    let Some(value) = normalize_optional(value) else {
        return Ok(None);
    };
    if !value.chars().all(|character| character.is_ascii_digit()) {
        return Err(AppError::Message(format!(
            "{label} must be a positive integer"
        )));
    }
    let parsed = value
        .parse::<u64>()
        .map_err(|_| AppError::Message(format!("{label} must be a positive integer")))?;
    if parsed == 0 {
        return Err(AppError::Message(format!(
            "{label} must be a positive integer"
        )));
    }
    Ok(Some(value))
}

fn validate_caf_piece_id(value: &str) -> Result<String> {
    let value = value.trim();
    if value.is_empty()
        || !value.chars().all(|character| character.is_ascii_digit())
        || value.parse::<u64>().unwrap_or(0) == 0
    {
        return Err(AppError::Message(
            "CAF piece ID must be a positive integer".to_string(),
        ));
    }
    Ok(value.to_string())
}

fn validate_external_text_id(value: Option<&str>, label: &str) -> Result<Option<String>> {
    let Some(value) = normalize_optional(value) else {
        return Ok(None);
    };
    if value
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
    {
        Ok(Some(value))
    } else {
        Err(AppError::Message(format!(
            "{label} may only contain letters, numbers, hyphens, and underscores"
        )))
    }
}

fn normalize_caf_collection_id(value: Option<&str>, label: &str) -> Result<Option<String>> {
    let Some(value) = normalize_optional(value) else {
        return Ok(None);
    };
    let id = caf_query_id_from_url(&value, "GCat").unwrap_or(value);
    validate_positive_integer_id(Some(&id), label)
}

fn normalize_caf_gallery_room_id(value: Option<&str>, label: &str) -> Result<Option<String>> {
    let Some(value) = normalize_optional(value) else {
        return Ok(None);
    };
    let id = caf_query_id_from_url(&value, "GSub").unwrap_or(value);
    validate_positive_integer_id(Some(&id), label)
}

fn normalize_snikt_collection_id(value: Option<&str>, label: &str) -> Result<Option<String>> {
    let Some(value) = normalize_optional(value) else {
        return Ok(None);
    };
    let id = snikt_collection_id_from_url(&value).unwrap_or(value);
    validate_external_text_id(Some(&id), label)
}

fn normalize_raremarq_collection_id(value: Option<&str>, label: &str) -> Result<Option<String>> {
    let Some(value) = normalize_optional(value) else {
        return Ok(None);
    };
    let id = raremarq_collection_id_from_url(&value).unwrap_or(value);
    validate_external_text_id(Some(&id), label)
}

fn normalize_raremarq_gallery_id(value: Option<&str>, label: &str) -> Result<Option<String>> {
    let Some(value) = normalize_optional(value) else {
        return Ok(None);
    };
    let id = raremarq_gallery_id_from_url(&value).unwrap_or(value);
    validate_external_text_id(Some(&id), label)
}

fn caf_query_id_from_url(value: &str, query_key: &str) -> Option<String> {
    let parsed = url::Url::parse(value).ok()?;
    let host = parsed.host_str()?.to_ascii_lowercase();
    if host != "www.comicartfans.com" && host != "comicartfans.com" {
        return None;
    }
    parsed.query_pairs().find_map(|(key, candidate)| {
        let candidate = candidate.trim();
        (key.eq_ignore_ascii_case(query_key)
            && !candidate.is_empty()
            && candidate
                .chars()
                .all(|character| character.is_ascii_digit()))
        .then(|| candidate.to_string())
    })
}

fn snikt_collection_id_from_url(value: &str) -> Option<String> {
    let parsed = url::Url::parse(value).ok()?;
    let host = parsed.host_str()?.to_ascii_lowercase();
    if host != "www.snikt.com" && host != "snikt.com" {
        return None;
    }
    let mut segments = parsed.path_segments()?;
    if segments.next()? != "user" {
        return None;
    }
    let id = segments.next()?.trim();
    validate_external_text_id(Some(id), "SNIKT.com Collection ID")
        .ok()
        .flatten()
}

fn raremarq_collection_id_from_url(value: &str) -> Option<String> {
    let parsed = url::Url::parse(value).ok()?;
    let host = parsed.host_str()?.to_ascii_lowercase();
    if host != "www.raremarq.com" && host != "raremarq.com" {
        return None;
    }
    let mut segments = parsed.path_segments()?;
    if segments.next()? != "u" {
        return None;
    }
    let id = segments.next()?.trim();
    validate_external_text_id(Some(id), "Raremarq Collection ID")
        .ok()
        .flatten()
}

fn raremarq_gallery_id_from_url(value: &str) -> Option<String> {
    let parsed = url::Url::parse(value).ok()?;
    let host = parsed.host_str()?.to_ascii_lowercase();
    if host != "www.raremarq.com" && host != "raremarq.com" {
        return None;
    }
    let mut segments = parsed.path_segments()?;
    if segments.next()? != "u" {
        return None;
    }
    let _user_slug = segments.next()?;
    if segments.next()? != "galleries" {
        return None;
    }
    let id = segments.next()?.trim();
    validate_external_text_id(Some(id), "Raremarq Gallery ID")
        .ok()
        .flatten()
}

fn validate_description(value: Option<&str>) -> Result<()> {
    let Some(value) = normalize_optional(value) else {
        return Ok(());
    };
    let lowered = value.to_ascii_lowercase();
    if lowered.contains("<script")
        || lowered.contains("</script")
        || lowered.contains("<iframe")
        || lowered.contains("</iframe")
        || lowered.contains("javascript:")
        || lowered.contains(" onload=")
        || lowered.contains(" onclick=")
        || lowered.contains(" onerror=")
    {
        return Err(AppError::Message(
            "Description contains executable HTML".to_string(),
        ));
    }
    Ok(())
}

fn validate_purchase_date(value: Option<&str>) -> Result<Option<String>> {
    let Some(value) = normalize_optional(value) else {
        return Ok(None);
    };
    NaiveDate::parse_from_str(&value, "%Y-%m-%d")
        .map_err(|_| AppError::Message("Purchase date must use YYYY-MM-DD format".to_string()))?;
    Ok(Some(value))
}

fn validated_artist_credits(credits: &[ArtistCreditUpdate]) -> Result<Vec<ArtistCredit>> {
    let mut validated = Vec::new();
    for credit in credits {
        let first_name = normalize_optional(credit.first_name.as_deref());
        let last_name = normalize_optional(credit.last_name.as_deref());
        let role_id = normalize_optional(credit.role_id.as_deref());
        let row_is_used = first_name.is_some() || last_name.is_some() || role_id.is_some();
        if !row_is_used {
            continue;
        }
        let first = first_name.as_deref().unwrap_or("");
        let last = last_name.as_deref().unwrap_or("");
        if first.is_empty() && last.is_empty() {
            return Err(AppError::Message(
                "Artist credit requires a first name or last name".to_string(),
            ));
        }
        if contains_html_like_tag(first) || contains_html_like_tag(last) {
            return Err(AppError::Message(
                "Artist names must not contain HTML tags".to_string(),
            ));
        }
        let role_id =
            normalize_controlled_id(role_id.as_deref(), "1", ARTIST_ROLE_OPTIONS, "artist role")?;
        let role = controlled_label(ARTIST_ROLE_OPTIONS, &role_id)
            .expect("validated artist role id")
            .to_string();
        let name = [first, last]
            .into_iter()
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        validated.push(ArtistCredit {
            name,
            role: Some(role),
            first_name,
            last_name,
            role_id: Some(role_id),
        });
    }
    Ok(validated)
}

fn snikt_upload_prefill_url(detail: &ArtworkDetail) -> String {
    let mut url = url::Url::parse("https://www.snikt.com/upload")
        .expect("static SNIKT upload URL should parse");
    {
        let mut query = url.query_pairs_mut();
        query.append_pair("prefill", "true");
        query.append_pair("displayName", &detail.title);
        let art_type = normalize_optional(detail.snikt_metadata.art_type.as_deref())
            .unwrap_or_else(|| snikt_art_type(detail.art_type_id.as_deref()).to_string());
        query.append_pair("artType", &art_type);
        if let Some(artist) = snikt_primary_artist_name(detail) {
            query.append_pair("artist", &artist);
        }
        if let Some(pencils) = snikt_artist_name_for_role(detail, "1") {
            query.append_pair("pencils", &pencils);
        }
        if let Some(inker) = snikt_artist_name_for_role(detail, "5") {
            query.append_pair("inker", &inker);
        }
        if let Some(letterer) = snikt_artist_name_for_role(detail, "11") {
            query.append_pair("letterer", &letterer);
        }
        if let Some(description) = normalize_optional(detail.description.as_deref()) {
            query.append_pair("comments", &description);
        }
        append_snikt_query_optional(
            &mut query,
            "comicPublisher",
            detail.snikt_metadata.comic_publisher.as_deref(),
        );
        append_snikt_query_optional(
            &mut query,
            "seriesTitle",
            detail.snikt_metadata.series_title.as_deref(),
        );
        append_snikt_query_optional(
            &mut query,
            "issueNumber",
            detail.snikt_metadata.issue_number.as_deref(),
        );
        append_snikt_query_optional(
            &mut query,
            "seriesPageNumber",
            detail.snikt_metadata.series_page_number.as_deref(),
        );
        append_snikt_query_optional(&mut query, "year", detail.snikt_metadata.year.as_deref());
        append_snikt_query_optional(
            &mut query,
            "character",
            detail.snikt_metadata.character.as_deref(),
        );
        append_snikt_query_optional(
            &mut query,
            "subcategory",
            detail.snikt_metadata.subcategory.as_deref(),
        );
        append_snikt_query_optional(
            &mut query,
            "animationStudio",
            detail.snikt_metadata.animation_studio.as_deref(),
        );
        append_snikt_query_optional(
            &mut query,
            "episodeNumber",
            detail.snikt_metadata.episode_number.as_deref(),
        );
        append_snikt_query_optional(
            &mut query,
            "episodeTitle",
            detail.snikt_metadata.episode_title.as_deref(),
        );
        append_snikt_query_optional(
            &mut query,
            "publishedDate",
            detail.snikt_metadata.published_date.as_deref(),
        );
        append_snikt_query_optional(
            &mut query,
            "stripTitle",
            detail.snikt_metadata.strip_title.as_deref(),
        );
        query.append_pair(
            "isSundayStrip",
            bool_query_value(detail.snikt_metadata.is_sunday_strip),
        );
        append_snikt_query_optional(&mut query, "other", detail.snikt_metadata.other.as_deref());
        append_snikt_query_optional(&mut query, "tags", detail.snikt_metadata.tags.as_deref());
        query.append_pair("isPublic", if detail.active { "true" } else { "false" });
        query.append_pair("isNSFW", bool_query_value(detail.snikt_metadata.is_nsfw));
        query.append_pair(
            "isForSale",
            bool_query_value(detail.snikt_metadata.is_for_sale),
        );
        append_snikt_query_optional(&mut query, "price", detail.snikt_metadata.price.as_deref());
        query.append_pair(
            "isOpenToOffers",
            bool_query_value(detail.snikt_metadata.is_open_to_offers),
        );
        append_snikt_query_optional(
            &mut query,
            "estimatedValue",
            detail.estimated_value.as_deref(),
        );
    }
    url.to_string()
}

fn append_snikt_query_optional(
    query: &mut url::form_urlencoded::Serializer<'_, url::UrlQuery<'_>>,
    key: &str,
    value: Option<&str>,
) {
    if let Some(value) = normalize_optional(value) {
        query.append_pair(key, &value);
    }
}

fn bool_query_value(value: bool) -> &'static str {
    if value {
        "true"
    } else {
        "false"
    }
}

fn snikt_art_type(art_type_id: Option<&str>) -> &'static str {
    match art_type_id {
        Some("1") => "Cover",
        Some("3" | "4" | "12" | "13" | "17") => "Interior",
        Some("2") => "Commission",
        Some("10") => "Comic Strip",
        Some("18") => "Animation Cel",
        Some("19") => "Trading Card Art",
        _ => "Other Illustration",
    }
}

fn snikt_artist_name_for_role(detail: &ArtworkDetail, role_id: &str) -> Option<String> {
    detail
        .artist_credits
        .iter()
        .find(|credit| credit.role_id.as_deref() == Some(role_id))
        .and_then(snikt_artist_credit_name)
}

fn snikt_primary_artist_name(detail: &ArtworkDetail) -> Option<String> {
    snikt_artist_name_for_role(detail, "1")
        .or_else(|| snikt_artist_name_for_role(detail, "6"))
        .or_else(|| snikt_artist_name_for_role(detail, "7"))
        .or_else(|| {
            detail
                .artist_credits
                .iter()
                .find_map(snikt_artist_credit_name)
        })
}

fn snikt_artist_credit_name(credit: &ArtistCredit) -> Option<String> {
    normalize_optional(Some(&credit.name))
}

fn contains_html_like_tag(value: &str) -> bool {
    value
        .find('<')
        .and_then(|start| value[start..].find('>').map(|end| start + end))
        .is_some()
}

fn contains_for_sale_phrase(value: &str) -> bool {
    let words = value
        .split_whitespace()
        .map(|word| word.trim_matches(|character: char| !character.is_alphanumeric()))
        .collect::<Vec<_>>();
    words
        .windows(2)
        .any(|pair| pair[0].eq_ignore_ascii_case("for") && pair[1].eq_ignore_ascii_case("sale"))
}

fn normalize_image_role(value: Option<&str>) -> Result<Option<String>> {
    let Some(value) = normalize_optional(value) else {
        return Ok(None);
    };
    match value.to_ascii_lowercase().as_str() {
        "raw_scan" | "raw_photo" | "corrected_scan" | "detail" | "verso" | "reference"
        | "basic" | "premium" => Ok(Some(value.to_ascii_lowercase())),
        "caf_basic" => Ok(Some("basic".to_string())),
        "caf_premium" => Ok(Some("premium".to_string())),
        _ => Err(AppError::Message(format!(
            "Unsupported image role: {value}"
        ))),
    }
}

fn normalize_stored_image_role(value: Option<String>) -> Option<String> {
    value.map(|role| match role.as_str() {
        "caf_basic" => "basic".to_string(),
        "caf_premium" => "premium".to_string(),
        _ => role,
    })
}

fn normalize_file_source_kind(value: &str) -> Result<String> {
    let value = value.trim().to_ascii_lowercase();
    if matches!(value.as_str(), "linked" | "copied" | "imported") {
        return Ok(value);
    }
    Err(AppError::Message(format!(
        "Unsupported file source kind: {value}"
    )))
}

pub fn parse_workspace_search_query(query: &str) -> Vec<WorkspaceSearchTerm> {
    let mut raw_terms = Vec::new();
    let mut current = String::new();
    let mut in_quote = false;

    for character in query.chars() {
        match character {
            '"' => {
                if in_quote && !current.trim().is_empty() {
                    raw_terms.push(std::mem::take(&mut current));
                }
                in_quote = !in_quote;
            }
            character if character.is_whitespace() && !in_quote => {
                if !current.trim().is_empty() {
                    raw_terms.push(std::mem::take(&mut current));
                }
            }
            _ => current.push(character),
        }
    }

    if !current.trim().is_empty() {
        raw_terms.push(current);
    }

    let mut normalized_terms = raw_terms
        .into_iter()
        .filter_map(|term| {
            let normalized = normalize_search_term(&term);
            (!normalized.is_empty()).then_some(normalized)
        })
        .collect::<Vec<_>>();
    let last_index = normalized_terms.len().saturating_sub(1);

    normalized_terms
        .drain(..)
        .enumerate()
        .map(|(index, value)| WorkspaceSearchTerm {
            value,
            mode: if index == last_index {
                SearchMatchMode::Prefix
            } else {
                SearchMatchMode::Contains
            },
        })
        .collect()
}

fn normalize_search_term(term: &str) -> String {
    let mut normalized = String::new();
    let mut previous_was_separator = true;

    for character in term.chars().flat_map(char::to_lowercase) {
        if character.is_alphanumeric() {
            normalized.push(character);
            previous_was_separator = false;
        } else if !previous_was_separator {
            normalized.push(' ');
            previous_was_separator = true;
        }
    }

    normalized.trim().to_string()
}

fn matching_artwork_ids_for_scope(
    conn: &Connection,
    scope_join_sql: &str,
    order_by_sql: &str,
    scope_id: i64,
    terms: &[WorkspaceSearchTerm],
) -> Result<Vec<i64>> {
    let use_fts = terms
        .iter()
        .all(|term| term.mode == SearchMatchMode::Prefix);
    if use_fts && !terms.is_empty() {
        refresh_artwork_search_index_locked(conn)?;
    }
    let search_text_sql = artwork_search_text_sql();
    let mut sql = format!(
        "SELECT DISTINCT searched.id
         FROM (
           SELECT a.id, a.canonical_id, ga.sort_order AS sort_order, {search_text_sql} AS search_text
           FROM artwork a
           {scope_join_sql}
         ) searched"
    );
    let mut query_params = vec![SqlValue::Integer(scope_id)];

    if use_fts && !terms.is_empty() {
        sql.push_str(
            " JOIN artwork_search_fts ON artwork_search_fts.artwork_id = searched.id WHERE artwork_search_fts MATCH ?",
        );
        query_params.push(SqlValue::Text(fts_query_for_search_terms(terms)));
        sql.push_str(
            &terms
                .iter()
                .map(|term| {
                    query_params.push(SqlValue::Text(like_pattern_for_search_term(term)));
                    " AND searched.search_text LIKE ? ESCAPE '\\'".to_string()
                })
                .collect::<Vec<_>>()
                .join(""),
        );
    } else if !terms.is_empty() {
        sql.push_str(" WHERE ");
        sql.push_str(
            &terms
                .iter()
                .map(|term| {
                    query_params.push(SqlValue::Text(like_pattern_for_search_term(term)));
                    "searched.search_text LIKE ? ESCAPE '\\'".to_string()
                })
                .collect::<Vec<_>>()
                .join(" AND "),
        );
    }

    sql.push_str(" ORDER BY ");
    sql.push_str(order_by_sql);

    let mut statement = conn.prepare(&sql)?;
    let rows = statement.query_map(params_from_iter(query_params.iter()), |row| row.get(0))?;
    rows.collect::<std::result::Result<Vec<_>, _>>()
        .map_err(AppError::from)
}

fn refresh_artwork_search_index_locked(conn: &Connection) -> Result<()> {
    conn.execute("DELETE FROM artwork_search_fts", [])?;
    let search_text_sql = artwork_search_text_sql();
    conn.execute(
        &format!(
            "INSERT INTO artwork_search_fts(rowid, artwork_id, search_text)
             SELECT a.id, a.id, {search_text_sql}
             FROM artwork a"
        ),
        [],
    )?;
    Ok(())
}

fn fts_query_for_search_terms(terms: &[WorkspaceSearchTerm]) -> String {
    terms
        .iter()
        .flat_map(|term| term.value.split_whitespace())
        .map(escape_fts_term)
        .filter(|term| !term.is_empty())
        .map(|term| format!("{term}*"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn escape_fts_term(term: &str) -> String {
    term.chars()
        .filter(|character| character.is_alphanumeric())
        .collect::<String>()
}

fn like_pattern_for_search_term(term: &WorkspaceSearchTerm) -> String {
    let escaped = escape_like_pattern(&term.value);
    match term.mode {
        SearchMatchMode::Contains => format!("%{escaped}%"),
        SearchMatchMode::Prefix => format!("% {escaped}%"),
    }
}

fn escape_like_pattern(value: &str) -> String {
    let mut escaped = String::new();
    for character in value.chars() {
        if matches!(character, '%' | '_' | '\\') {
            escaped.push('\\');
        }
        escaped.push(character);
    }
    escaped
}

fn artwork_search_text_sql() -> String {
    normalized_search_sql(
        r#"' ' ||
           COALESCE(a.canonical_id, '') || ' ' ||
           COALESCE(a.artwork_stable_id, '') || ' ' ||
           COALESCE(a.title, '') || ' ' ||
           COALESCE(a.description, '') || ' ' ||
           COALESCE(a.for_sale_status, '') || ' ' ||
           COALESCE(a.media_type_id, '') || ' ' ||
           COALESCE(a.media, '') || ' ' ||
           COALESCE(a.art_type_id, '') || ' ' ||
           COALESCE(a.format, '') || ' ' ||
           COALESCE(a.publication_status_id, '') || ' ' ||
           COALESCE(a.source_folder, '') || ' ' ||
           COALESCE((SELECT GROUP_CONCAT(
               g2.name || ' ' ||
               COALESCE(g2.caf_gallery_room_id, '') || ' ' ||
               COALESCE(g2.snikt_gallery_id, '') || ' ' ||
               COALESCE(g2.raremarq_gallery_id, ''),
               ' ')
             FROM gallery g2
             JOIN gallery_artwork ga2 ON ga2.gallery_id = g2.id
             WHERE ga2.artwork_id = a.id), '') || ' ' ||
           COALESCE((SELECT GROUP_CONCAT(
               c2.name || ' ' ||
               COALESCE(c2.caf_collection_id, '') || ' ' ||
               COALESCE(c2.snikt_collection_id, '') || ' ' ||
               COALESCE(c2.raremarq_collection_id, ''),
               ' ')
             FROM collection c2
             JOIN collection_gallery cg2 ON cg2.collection_id = c2.id
             JOIN gallery_artwork ga3 ON ga3.gallery_id = cg2.gallery_id
             WHERE ga3.artwork_id = a.id), '') || ' ' ||
           COALESCE((SELECT GROUP_CONCAT(
               COALESCE(el.link_type, '') || ' ' ||
               COALESCE(el.external_id, '') || ' ' ||
               COALESCE(el.url, ''),
               ' ')
             FROM external_link el
             WHERE el.artwork_id = a.id), '') || ' ' ||
           COALESCE((SELECT GROUP_CONCAT(
               COALESCE(ar.name, '') || ' ' ||
               COALESCE(aa.first_name, '') || ' ' ||
               COALESCE(aa.last_name, '') || ' ' ||
               COALESCE(aa.role, '') || ' ' ||
               COALESCE(aa.role_id, ''),
               ' ')
             FROM artwork_artist aa
             JOIN artist ar ON ar.id = aa.artist_id
             WHERE aa.artwork_id = a.id), '') || ' ' ||
           COALESCE((SELECT GROUP_CONCAT(
               COALESCE(f.file_name, '') || ' ' ||
               COALESCE(f.relative_path, '') || ' ' ||
               COALESCE(f.current_path, '') || ' ' ||
               COALESCE(f.extension, '') || ' ' ||
               COALESCE(f.source_kind, '') || ' ' ||
               COALESCE(f.image_role, ''),
               ' ')
             FROM file_asset f
             WHERE f.artwork_id = a.id), '') || ' ' ||
           COALESCE((SELECT GROUP_CONCAT(
               COALESCE(da.path, '') || ' ' ||
               COALESCE(da.format, '') || ' ' ||
               COALESCE(da.derivative_type, '') || ' ' ||
               COALESCE(da.image_role, ''),
               ' ')
             FROM derived_asset da
             WHERE da.artwork_id = a.id), '') || ' ' ||
           COALESCE((SELECT GROUP_CONCAT(
               COALESCE(pm.purchase_price, '') || ' ' ||
               COALESCE(pm.estimated_value, '') || ' ' ||
               COALESCE(pm.purchase_date, '') || ' ' ||
               COALESCE(pm.provenance, '') || ' ' ||
               COALESCE(pm.personal_notes, ''),
               ' ')
             FROM private_metadata pm
             WHERE pm.artwork_id = a.id), '') || ' '"#,
    )
}

fn normalized_search_sql(expression: &str) -> String {
    let mut sql = format!("LOWER({expression})");
    for separator in [
        "char(9)",
        "char(10)",
        "char(13)",
        "char(34)",
        "char(35)",
        "char(39)",
        "char(40)",
        "char(41)",
        "char(44)",
        "char(45)",
        "char(46)",
        "char(47)",
        "char(58)",
        "char(59)",
        "char(91)",
        "char(92)",
        "char(93)",
        "char(95)",
        "char(123)",
        "char(125)",
    ] {
        sql = format!("REPLACE({sql}, {separator}, ' ')");
    }
    sql
}

fn empty_workspace_state() -> WorkspaceState {
    WorkspaceState {
        mode: "none".to_string(),
        collection: None,
        galleries: Vec::new(),
        selected_gallery_id: None,
        artworks: Vec::new(),
    }
}

fn normalized_name(value: &str, fallback: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        fallback.to_string()
    } else {
        trimmed.to_string()
    }
}

fn stable_id(prefix: &str) -> String {
    let nanos = Utc::now().timestamp_nanos_opt().unwrap_or_default();
    format!("{prefix}-{nanos}")
}

fn manifest_path_from_input(
    input: &Path,
    name: &str,
    extension: &str,
    fallback_name: &str,
) -> PathBuf {
    if is_dot_manifest_path(input, extension) {
        return input.to_path_buf();
    }
    match input.extension().and_then(|value| value.to_str()) {
        Some(existing_extension) if existing_extension.eq_ignore_ascii_case(extension) => {
            input.to_path_buf()
        }
        Some(_) => input.with_extension(extension),
        None => input.join(
            dot_manifest_file_name(extension)
                .unwrap_or_else(|| manifest_file_name(name, fallback_name, extension)),
        ),
    }
}

pub(crate) fn default_collection_manifest_path(collection_folder: &Path) -> PathBuf {
    collection_folder.join(".oacollection")
}

pub(crate) fn default_gallery_manifest_path(gallery_folder: &Path) -> PathBuf {
    gallery_folder.join(".oagallery")
}

fn is_dot_manifest_path(path: &Path, extension: &str) -> bool {
    path.file_name()
        .and_then(|value| value.to_str())
        .is_some_and(|file_name| dot_manifest_file_name(extension).as_deref() == Some(file_name))
}

fn dot_manifest_file_name(extension: &str) -> Option<String> {
    match extension {
        "oacollection" | "oagallery" | "oaartwork" => Some(format!(".{extension}")),
        _ => None,
    }
}

fn manifest_file_name(name: &str, fallback_name: &str, extension: &str) -> String {
    let stem = normalized_name(name, fallback_name)
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
    let stem = stem.split_whitespace().collect::<Vec<_>>().join(" ");
    let stem = stem.trim_matches([' ', '.']).trim();
    let stem = if stem.is_empty() { fallback_name } else { stem };
    format!("{stem}.{extension}")
}

fn default_artwork_manifest_path(collection_path: &Path, canonical_id: &str) -> PathBuf {
    let collection_root = collection_root_for_gallery_manifest(collection_path);
    collection_root
        .join("artworks")
        .join(canonical_id)
        .join(".oaartwork")
}

fn collection_root_for_gallery_manifest(gallery_manifest_path: &Path) -> PathBuf {
    let gallery_folder = gallery_manifest_path
        .parent()
        .unwrap_or_else(|| Path::new("."));
    if gallery_folder
        .parent()
        .and_then(Path::file_name)
        .and_then(|value| value.to_str())
        .is_some_and(|name| path_component_eq(name, "galleries"))
    {
        return gallery_folder
            .parent()
            .and_then(Path::parent)
            .unwrap_or(gallery_folder)
            .to_path_buf();
    }
    gallery_folder.to_path_buf()
}

fn archive_relative_path(base_manifest_path: &Path, target_path: &Path) -> String {
    let base = base_manifest_path
        .parent()
        .unwrap_or_else(|| Path::new("."));
    let relative = target_path.strip_prefix(base).unwrap_or(target_path);
    relative
        .components()
        .map(|component| component.as_os_str().to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .join("/")
}

fn resolve_manifest_reference_path(base_manifest_path: &Path, reference_path: &str) -> PathBuf {
    let candidate = PathBuf::from(reference_path.replace('/', std::path::MAIN_SEPARATOR_STR));
    if candidate.is_absolute() {
        return candidate;
    }
    base_manifest_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(candidate)
}

fn managed_container_folder_for_manifest(manifest_path: &Path) -> Option<PathBuf> {
    let parent = manifest_path.parent()?;
    if is_dot_manifest_path(manifest_path, "oacollection")
        || is_dot_manifest_path(manifest_path, "oagallery")
        || is_dot_manifest_path(manifest_path, "oaartwork")
    {
        return Some(parent.to_path_buf());
    }
    let parent_name = parent.file_name()?.to_str()?;
    let stem = manifest_path.file_stem()?.to_str()?;
    path_component_eq(parent_name, stem).then(|| parent.to_path_buf())
}

fn path_component_eq(left: &str, right: &str) -> bool {
    if cfg!(windows) {
        left.eq_ignore_ascii_case(right)
    } else {
        left == right
    }
}

fn path_is_at_or_under(root: &Path, path: &Path) -> bool {
    let root = normalized_path_prefix(root);
    let path = normalized_path_prefix(path);
    if root.is_empty() || path.is_empty() {
        return false;
    }
    path == root || path.starts_with(&format!("{root}/"))
}

fn path_is_covered_by_bulk_trash(bulk_trash_root: Option<&Path>, path: &Path) -> bool {
    bulk_trash_root.is_some_and(|root| path_is_at_or_under(root, path))
}

fn normalized_path_prefix(path: &Path) -> String {
    let mut value = path.to_string_lossy().replace('\\', "/");
    while value.ends_with('/') {
        value.pop();
    }
    if cfg!(windows) {
        value.make_ascii_lowercase();
    }
    value
}

fn remove_file_if_exists(path: &Path) -> Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(AppError::from(error)),
    }
}

fn remove_empty_dir_tree_if_exists(path: &Path) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }
    if !path.is_dir() {
        return Ok(());
    }
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let child = entry.path();
        if child.is_dir() {
            remove_empty_dir_tree_if_exists(&child)?;
        }
    }
    remove_empty_dir_if_exists(path)
}

fn remove_empty_dir_if_exists(path: &Path) -> Result<()> {
    match fs::remove_dir(path) {
        Ok(()) => Ok(()),
        Err(error)
            if matches!(
                error.kind(),
                ErrorKind::NotFound | ErrorKind::DirectoryNotEmpty
            ) =>
        {
            Ok(())
        }
        Err(error) => Err(AppError::from(error)),
    }
}

fn cleanup_unlinked_artworks_locked(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute(
        "DELETE FROM artwork
         WHERE id NOT IN (SELECT artwork_id FROM gallery_artwork)",
        [],
    )?;
    Ok(())
}

fn add_column_if_missing(
    conn: &Connection,
    table_name: &str,
    column_name: &str,
    column_definition: &str,
) -> Result<()> {
    let mut statement = conn.prepare(&format!("PRAGMA table_info({table_name})"))?;
    let columns = statement.query_map([], |row| row.get::<_, String>(1))?;
    for column in columns {
        if column? == column_name {
            return Ok(());
        }
    }
    conn.execute_batch(&format!(
        "ALTER TABLE {table_name} ADD COLUMN {column_name} {column_definition}"
    ))?;
    Ok(())
}

fn next_canonical_id_locked(conn: &Connection) -> rusqlite::Result<String> {
    let mut allocator = CanonicalIdAllocator::new(conn)?;
    Ok(allocator.next_id())
}

fn caf_piece_id_from_url(url: &str) -> Option<String> {
    if !url.to_ascii_lowercase().contains("gallerypiece.asp") {
        return None;
    }
    let query = url.split_once('?')?.1;
    for pair in query.split('&') {
        let Some((key, value)) = pair.split_once('=') else {
            continue;
        };
        let piece_id = value.trim();
        if key.eq_ignore_ascii_case("piece")
            && !piece_id.is_empty()
            && piece_id.chars().all(|character| character.is_ascii_digit())
        {
            return Some(piece_id.to_string());
        }
    }
    None
}

fn snikt_image_id_from_url(url: &str) -> Option<String> {
    let parsed = url::Url::parse(url).ok()?;
    let host = parsed.host_str()?.to_ascii_lowercase();
    if host != "www.snikt.com" && host != "snikt.com" {
        return None;
    }
    let mut segments = parsed.path_segments()?;
    if segments.next()? != "image" {
        return None;
    }
    let id = segments.next()?.trim();
    validate_external_text_id(Some(id), "SNIKT.com Artwork ID")
        .ok()
        .flatten()
}

fn raremarq_piece_slug_from_url(url: &str) -> Option<String> {
    let parsed = url::Url::parse(url).ok()?;
    let host = parsed.host_str()?.to_ascii_lowercase();
    if host != "www.raremarq.com" && host != "raremarq.com" {
        return None;
    }
    let mut segments = parsed.path_segments()?;
    if segments.next()? != "u" {
        return None;
    }
    let _user_slug = segments.next()?;
    if segments.next()? != "pieces" {
        return None;
    }
    let slug = segments.next()?.trim();
    validate_external_text_id(Some(slug), "Raremarq Artwork ID")
        .ok()
        .flatten()
}

fn artwork_id_for_external_id_locked(
    conn: &Connection,
    provider: &str,
    external_id: &str,
) -> Result<Option<i64>> {
    Ok(conn
        .query_row(
            "SELECT artwork_id FROM external_link WHERE link_type = ?1 AND external_id = ?2",
            params![provider, external_id],
            |row| row.get(0),
        )
        .optional()?)
}

fn create_imported_artwork_locked(
    conn: &Connection,
    gallery: &GallerySummary,
    title: &str,
    now: &str,
) -> Result<i64> {
    let mut allocator = CanonicalIdAllocator::new(conn)?;
    create_imported_artwork_with_allocator_locked(conn, gallery, title, now, &mut allocator)
}

fn create_imported_artwork_with_allocator_locked(
    conn: &Connection,
    gallery: &GallerySummary,
    title: &str,
    now: &str,
    allocator: &mut CanonicalIdAllocator,
) -> Result<i64> {
    let canonical_id = allocator.next_id();
    let manifest_path = default_artwork_manifest_path(&gallery.manifest_path, &canonical_id);
    let path_string = manifest_path.to_string_lossy().to_string();
    conn.execute(
        "INSERT INTO artwork
         (canonical_id, artwork_stable_id, title, source_folder, source_context, artwork_manifest_path, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            canonical_id,
            canonical_id,
            title,
            path_string,
            gallery.name,
            path_string,
            now,
            now
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

fn artwork_id_for_caf_csv_image_link_and_added_in_gallery_locked(
    conn: &Connection,
    gallery_id: i64,
    image_link: Option<&str>,
    added_to_caf: Option<&str>,
) -> Result<Option<i64>> {
    let Some(image_link) = normalize_optional(image_link) else {
        return Ok(None);
    };
    let Some(added_to_caf) = normalize_optional(added_to_caf)
        .and_then(|value| normalize_caf_added_to_caf_minute(&value))
    else {
        return Ok(None);
    };
    let mut statement = conn.prepare(
        "SELECT a.id, a.caf_csv_added_to_caf
         FROM artwork a
         JOIN gallery_artwork ga ON ga.artwork_id = a.id
         WHERE ga.gallery_id = ?1
           AND a.caf_csv_image_link = ?2
           AND a.caf_csv_added_to_caf IS NOT NULL
         ORDER BY a.id",
    )?;
    let rows = statement
        .query_map(params![gallery_id, image_link], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, Option<String>>(1)?))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?
        .into_iter()
        .filter_map(|(id, stored)| {
            let stored = stored?;
            (normalize_caf_added_to_caf_minute(&stored).as_deref() == Some(added_to_caf.as_str()))
                .then_some(id)
        })
        .take(2)
        .collect::<Vec<_>>();
    match rows.as_slice() {
        [] => Ok(None),
        [id] => Ok(Some(*id)),
        _ => Err(AppError::Message(
            "CAF CSV row has multiple existing Artwork candidates with the same CAF CSV image link and added-to-CAF timestamp; reconciliation is required."
                .to_string(),
        )),
    }
}

fn caf_csv_auto_match_artwork_id_in_gallery_locked(
    conn: &Connection,
    gallery_id: i64,
    title: &str,
    imported: &ImportedCafImageArtwork,
) -> Result<Option<i64>> {
    let mut matched_id = artwork_id_for_caf_csv_image_link_and_added_in_gallery_locked(
        conn,
        gallery_id,
        imported.source_thumbnail_url.as_deref(),
        imported.added_to_caf.as_deref(),
    )?;
    if matched_id.is_none() {
        matched_id = artwork_id_for_caf_csv_image_link_in_gallery_locked(
            conn,
            gallery_id,
            imported.source_thumbnail_url.as_deref(),
        )?;
    }
    if matched_id.is_none() {
        matched_id = imported
            .source_thumbnail_url
            .as_deref()
            .map(|url| artwork_id_for_external_id_locked(conn, "caf_image_thumbnail", url))
            .transpose()?
            .flatten();
    }
    if matched_id.is_none() {
        matched_id =
            artwork_id_for_external_id_locked(conn, "caf_image", &imported.source_image_url)?;
    }
    if matched_id.is_none() {
        matched_id = artwork_id_for_caf_csv_added_title_in_gallery_locked(
            conn,
            gallery_id,
            title,
            imported.added_to_caf.as_deref(),
        )?;
    }
    Ok(matched_id)
}

fn artwork_id_for_caf_csv_image_link_in_gallery_locked(
    conn: &Connection,
    gallery_id: i64,
    image_link: Option<&str>,
) -> Result<Option<i64>> {
    let Some(image_link) = normalize_optional(image_link) else {
        return Ok(None);
    };
    let mut statement = conn.prepare(
        "SELECT a.id
         FROM artwork a
         JOIN gallery_artwork ga ON ga.artwork_id = a.id
         WHERE ga.gallery_id = ?1
           AND a.caf_csv_image_link = ?2
         ORDER BY a.id
         LIMIT 2",
    )?;
    let rows = statement
        .query_map(params![gallery_id, image_link], |row| row.get::<_, i64>(0))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    match rows.as_slice() {
        [] => Ok(None),
        [id] => Ok(Some(*id)),
        _ => Err(AppError::Message(
            "CAF CSV row has multiple existing Artwork candidates with the same CAF CSV image link; reconciliation is required."
                .to_string(),
        )),
    }
}

fn artwork_id_for_caf_csv_added_title_in_gallery_locked(
    conn: &Connection,
    gallery_id: i64,
    title: &str,
    added_to_caf: Option<&str>,
) -> Result<Option<i64>> {
    let Some(added_to_caf) = normalize_optional(added_to_caf)
        .and_then(|value| normalize_caf_added_to_caf_minute(&value))
    else {
        return Ok(None);
    };
    let mut statement = conn.prepare(
        "SELECT a.id, a.caf_csv_added_to_caf
         FROM artwork a
         JOIN gallery_artwork ga ON ga.artwork_id = a.id
         WHERE ga.gallery_id = ?1
           AND a.title = ?2
           AND a.caf_csv_added_to_caf IS NOT NULL
         ORDER BY a.id",
    )?;
    let rows = statement
        .query_map(params![gallery_id, title], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, Option<String>>(1)?))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?
        .into_iter()
        .filter_map(|(id, stored)| {
            let stored = stored?;
            (normalize_caf_added_to_caf_minute(&stored).as_deref() == Some(added_to_caf.as_str()))
                .then_some(id)
        })
        .take(2)
        .collect::<Vec<_>>();
    match rows.as_slice() {
        [] => Ok(None),
        [id] => Ok(Some(*id)),
        _ => Err(AppError::Message(format!(
            "CAF CSV row for \"{title}\" has multiple existing Artwork candidates with the same added-to-CAF timestamp; reconciliation is required."
        ))),
    }
}

fn normalize_caf_added_to_caf_minute(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    for format in [
        "%Y-%m-%dT%H:%M",
        "%Y-%m-%dT%H:%M:%S",
        "%Y-%m-%d %H:%M",
        "%Y-%m-%d %H:%M:%S",
        "%m/%d/%Y %I:%M:%S %p",
        "%m/%d/%Y %I:%M %p",
        "%-m/%-d/%Y %I:%M:%S %p",
        "%-m/%-d/%Y %I:%M %p",
    ] {
        if let Ok(parsed) = NaiveDateTime::parse_from_str(value, format) {
            return Some(parsed.format("%Y-%m-%dT%H:%M").to_string());
        }
    }
    if let Ok(parsed) = NaiveDate::parse_from_str(value, "%Y-%m-%d") {
        return Some(parsed.format("%Y-%m-%d").to_string());
    }
    let parts = value.split('/').collect::<Vec<_>>();
    if parts.len() == 3 {
        let parsed = parts[0]
            .parse::<u32>()
            .ok()
            .zip(parts[1].parse::<u32>().ok())
            .zip(parts[2].parse::<i32>().ok())
            .and_then(|((month, day), year)| NaiveDate::from_ymd_opt(year, month, day));
        if let Some(date) = parsed {
            return Some(date.format("%Y-%m-%d").to_string());
        }
    }
    None
}

fn artwork_title_exists_in_gallery_locked(
    conn: &Connection,
    gallery_id: i64,
    title: &str,
) -> Result<bool> {
    Ok(conn
        .query_row(
            "SELECT 1
             FROM artwork a
             JOIN gallery_artwork ga ON ga.artwork_id = a.id
             WHERE ga.gallery_id = ?1
               AND a.title = ?2
             LIMIT 1",
            params![gallery_id, title],
            |_| Ok(()),
        )
        .optional()?
        .is_some())
}

fn artwork_id_for_snikt_csv_title_date_in_gallery_locked(
    conn: &Connection,
    gallery_id: i64,
    title: &str,
    created_date: Option<&str>,
) -> Result<Option<i64>> {
    let Some(created_date) = normalize_optional(created_date) else {
        return Ok(None);
    };
    let mut statement = conn.prepare(
        "SELECT a.id
         FROM artwork a
         JOIN gallery_artwork ga ON ga.artwork_id = a.id
         WHERE ga.gallery_id = ?1
           AND a.title = ?2
           AND a.snikt_csv_created_date = ?3
         ORDER BY a.id
         LIMIT 2",
    )?;
    let rows = statement
        .query_map(params![gallery_id, title, created_date], |row| {
            row.get::<_, i64>(0)
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    match rows.as_slice() {
        [] => Ok(None),
        [id] => Ok(Some(*id)),
        _ => Err(AppError::Message(format!(
            "SNIKT.com CSV row for \"{title}\" has multiple existing Artwork candidates with the same created date; reconciliation is required."
        ))),
    }
}

fn upsert_external_link_locked(
    conn: &Connection,
    artwork_id: i64,
    provider: &str,
    external_id: Option<&str>,
    url: &str,
) -> Result<()> {
    upsert_external_link_locked_with_extensions(conn, artwork_id, provider, external_id, url, None)
}

fn upsert_external_link_locked_with_extensions(
    conn: &Connection,
    artwork_id: i64,
    provider: &str,
    external_id: Option<&str>,
    url: &str,
    extensions: Option<&serde_json::Value>,
) -> Result<()> {
    if let Some(external_id) = external_id {
        let conflict_id = artwork_id_for_external_id_locked(conn, provider, external_id)?;
        if conflict_id.is_some_and(|id| id != artwork_id) {
            return Err(AppError::Message(format!(
                "{provider} artwork ID already exists in this catalog: {external_id}"
            )));
        }
    }
    let extensions_json = extensions.map(serde_json::to_string).transpose()?;
    conn.execute(
        "INSERT INTO external_link (artwork_id, link_type, external_id, url, extensions_json)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(artwork_id, link_type) DO UPDATE SET
           external_id = excluded.external_id,
           url = excluded.url,
           extensions_json = excluded.extensions_json",
        params![artwork_id, provider, external_id, url, extensions_json],
    )?;
    Ok(())
}

fn artwork_id_label_preference_locked(conn: &Connection) -> Result<ArtworkIdLabelPreference> {
    let value: Option<String> = conn
        .query_row(
            "SELECT value FROM app_setting WHERE key = ?1",
            params![ARTWORK_ID_LABEL_PREFERENCE_SETTING],
            |row| row.get(0),
        )
        .optional()?;
    value
        .as_deref()
        .map(ArtworkIdLabelPreference::from_setting_value)
        .unwrap_or(Ok(ArtworkIdLabelPreference::Oac))
}

fn apply_artwork_id_label_preference(
    summary: &mut ArtworkSummary,
    preference: ArtworkIdLabelPreference,
) {
    summary.display_id = display_id_for(
        &summary.display_id,
        summary.caf_artwork_id.as_deref(),
        summary.snikt_artwork_id.as_deref(),
        summary.raremarq_artwork_id.as_deref(),
        preference,
    );
}

fn display_id_for(
    local_oac_id: &str,
    caf_artwork_id: Option<&str>,
    snikt_artwork_id: Option<&str>,
    raremarq_artwork_id: Option<&str>,
    preference: ArtworkIdLabelPreference,
) -> String {
    match preference {
        ArtworkIdLabelPreference::PreferCaf => caf_artwork_id
            .map(|id| format!("CAF-{id}"))
            .unwrap_or_else(|| local_oac_id.to_string()),
        ArtworkIdLabelPreference::PreferSnikt => snikt_artwork_id
            .map(|id| format!("SNIKT-{id}"))
            .unwrap_or_else(|| local_oac_id.to_string()),
        ArtworkIdLabelPreference::PreferRaremarq => raremarq_artwork_id
            .map(|id| format!("RAREMARQ-{id}"))
            .unwrap_or_else(|| local_oac_id.to_string()),
        ArtworkIdLabelPreference::Oac => local_oac_id.to_string(),
    }
}

fn default_oac_display_id(canonical_id: &str, stable_id: Option<&str>) -> String {
    stable_id
        .filter(|value| oac_number(value).is_some())
        .or_else(|| (oac_number(canonical_id).is_some()).then_some(canonical_id))
        .unwrap_or(canonical_id)
        .to_string()
}

fn oac_number(value: &str) -> Option<i64> {
    value.strip_prefix("OAC-")?.parse::<i64>().ok()
}

fn collection_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<CollectionSummary> {
    Ok(CollectionSummary {
        id: row.get(0)?,
        stable_id: row.get(1)?,
        name: row.get(2)?,
        manifest_path: PathBuf::from(row.get::<_, String>(3)?),
        caf_collection_id: row.get(4)?,
        snikt_collection_id: row.get(5)?,
        raremarq_collection_id: row.get(6)?,
    })
}

fn gallery_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<GallerySummary> {
    Ok(GallerySummary {
        id: row.get(0)?,
        stable_id: row.get(1)?,
        name: row.get(2)?,
        manifest_path: PathBuf::from(row.get::<_, String>(3)?),
        caf_gallery_room_id: row.get(4)?,
        snikt_gallery_id: row.get(5)?,
        snikt_gallery_inherits_collection: row.get(6)?,
        raremarq_gallery_id: row.get(7)?,
    })
}

fn artwork_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ArtworkSummary> {
    let canonical_id: String = row.get(1)?;
    let stable_id: Option<String> = row.get(2)?;
    let caf_artwork_id: Option<String> = row.get(3)?;
    let snikt_artwork_id: Option<String> = row.get(4)?;
    let raremarq_artwork_id: Option<String> = row.get(5)?;
    let thumbnail: Option<String> = row.get(10)?;
    let manifest_path: Option<String> = row.get(12)?;
    Ok(ArtworkSummary {
        id: row.get(0)?,
        display_id: default_oac_display_id(&canonical_id, stable_id.as_deref()),
        canonical_id,
        caf_artwork_id,
        snikt_artwork_id,
        raremarq_artwork_id,
        title: row.get(6)?,
        media: row.get(7)?,
        format: row.get(8)?,
        source_folder: PathBuf::from(row.get::<_, String>(9)?),
        thumbnail_path: thumbnail.map(PathBuf::from),
        file_count: row.get(11)?,
        manifest_path: manifest_path.map(PathBuf::from),
        gallery_ids: Vec::new(),
        gallery_names: Vec::new(),
        artist_credits: Vec::new(),
    })
}

fn collect_string_column(conn: &Connection, sql: &str) -> Result<Vec<String>> {
    let mut statement = conn.prepare(sql)?;
    let rows = statement.query_map([], |row| row.get(0))?;
    rows.collect::<std::result::Result<Vec<_>, _>>()
        .map_err(AppError::from)
}

fn collect_i64_column_with_param(conn: &Connection, sql: &str, id: i64) -> Result<Vec<i64>> {
    let mut statement = conn.prepare(sql)?;
    let rows = statement.query_map(params![id], |row| row.get(0))?;
    rows.collect::<std::result::Result<Vec<_>, _>>()
        .map_err(AppError::from)
}

fn collect_string_column_with_param(conn: &Connection, sql: &str, id: i64) -> Result<Vec<String>> {
    let mut statement = conn.prepare(sql)?;
    let rows = statement.query_map(params![id], |row| row.get(0))?;
    rows.collect::<std::result::Result<Vec<_>, _>>()
        .map_err(AppError::from)
}

fn sql_placeholders(count: usize) -> String {
    std::iter::repeat_n("?", count)
        .collect::<Vec<_>>()
        .join(", ")
}
