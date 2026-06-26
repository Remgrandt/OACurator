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

CREATE TABLE IF NOT EXISTS derived_asset_render (
  derived_asset_id INTEGER PRIMARY KEY REFERENCES derived_asset(id) ON DELETE CASCADE,
  purpose TEXT NOT NULL,
  recipe_key TEXT NOT NULL,
  recipe_json TEXT NOT NULL,
  source_path TEXT NOT NULL,
  source_size_bytes INTEGER NOT NULL,
  source_modified_at TEXT,
  source_width INTEGER NOT NULL,
  source_height INTEGER NOT NULL,
  output_width INTEGER NOT NULL,
  output_height INTEGER NOT NULL,
  output_size_bytes INTEGER NOT NULL,
  renderer TEXT NOT NULL,
  renderer_version TEXT NOT NULL,
  renderer_options_json TEXT NOT NULL,
  created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS file_asset_image_probe (
  file_asset_id INTEGER PRIMARY KEY REFERENCES file_asset(id) ON DELETE CASCADE,
  probe_status TEXT NOT NULL,
  render_status TEXT NOT NULL,
  width INTEGER,
  height INTEGER,
  dpi_x REAL,
  dpi_y REAL,
  container_format TEXT,
  detected_mime TEXT,
  compression TEXT,
  photometric TEXT,
  bits_per_sample INTEGER,
  samples_per_pixel INTEGER,
  has_alpha INTEGER,
  preferred_renderer TEXT,
  renderer_version TEXT,
  error_code TEXT,
  error_message TEXT,
  probed_at TEXT NOT NULL
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
CREATE INDEX IF NOT EXISTS idx_derived_asset_render_recipe ON derived_asset_render(recipe_key);
CREATE INDEX IF NOT EXISTS idx_derived_asset_render_source ON derived_asset_render(source_path, source_size_bytes, source_modified_at);
CREATE INDEX IF NOT EXISTS idx_file_asset_external_link_asset ON file_asset_external_link(file_asset_id);
CREATE INDEX IF NOT EXISTS idx_oaa_extension_block_owner ON oaa_extension_block(owner_kind, owner_id);
CREATE INDEX IF NOT EXISTS idx_manifest_projection_state_updated_at ON manifest_projection_state(updated_at);
CREATE UNIQUE INDEX IF NOT EXISTS idx_external_link_provider_id
  ON external_link(link_type, external_id)
  WHERE external_id IS NOT NULL;
