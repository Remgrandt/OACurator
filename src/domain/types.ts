// Copyright (c) 2026 Remgrandt Works. All rights reserved.

export type ArtistCredit = {
  name: string;
  role?: string | null;
  first_name?: string | null;
  last_name?: string | null;
  role_id?: string | null;
};

export type CollectionSummary = {
  id: number;
  stable_id: string;
  name: string;
  manifest_path: string;
  caf_collection_id?: string | null;
  snikt_collection_id?: string | null;
  raremarq_collection_id?: string | null;
};

export type RecentCollection = {
  name: string;
  path: string;
  last_opened_at: string;
};

export type GallerySummary = {
  id: number;
  stable_id: string;
  name: string;
  manifest_path: string;
  caf_gallery_room_id?: string | null;
  snikt_gallery_id?: string | null;
  snikt_gallery_inherits_collection: boolean;
  raremarq_gallery_id?: string | null;
};

export type ImageRole =
  | "raw_scan"
  | "raw_photo"
  | "corrected_scan"
  | "detail"
  | "verso"
  | "reference"
  | "basic"
  | "premium";
export type FileSourceKind = "linked" | "copied" | "imported";
export type PngExportVariant = "basic" | "premium";
export type AssetKind = "file" | "derived";

export type ArtistCreditRequest = {
  first_name?: string | null;
  last_name?: string | null;
  role_id?: string | null;
};

export type SniktMetadataRequest = {
  art_type?: string | null;
  comic_publisher?: string | null;
  series_title?: string | null;
  issue_number?: string | null;
  series_page_number?: string | null;
  year?: string | null;
  character?: string | null;
  subcategory?: string | null;
  animation_studio?: string | null;
  episode_number?: string | null;
  episode_title?: string | null;
  published_date?: string | null;
  strip_title?: string | null;
  is_sunday_strip: boolean;
  other?: string | null;
  tags?: string | null;
  is_nsfw: boolean;
  is_for_sale: boolean;
  price?: string | null;
  is_open_to_offers: boolean;
};

export type MetadataSaveRequest = {
  artwork_id: number;
  title: string;
  description?: string | null;
  for_sale_status?: string | null;
  media_type_id?: string | null;
  art_type_id?: string | null;
  publication_status_id?: string | null;
  active: boolean;
  illustration_exchange: boolean;
  ix_for_sale: boolean;
  artist_credits: ArtistCreditRequest[];
  media?: string | null;
  format?: string | null;
  caf_url?: string | null;
  snikt_url?: string | null;
  raremarq_url?: string | null;
  generic_url?: string | null;
  snikt_metadata: SniktMetadataRequest;
  purchase_price?: string | null;
  estimated_value?: string | null;
  purchase_date?: string | null;
  provenance?: string | null;
  personal_notes?: string | null;
};

export type ArtworkSummary = {
  id: number;
  canonical_id: string;
  display_id?: string;
  caf_artwork_id?: string | null;
  snikt_artwork_id?: string | null;
  raremarq_artwork_id?: string | null;
  title: string;
  media?: string | null;
  format?: string | null;
  source_folder: string;
  thumbnail_path?: string | null;
  file_count: number;
  manifest_path?: string | null;
  gallery_ids: number[];
  gallery_names: string[];
  artist_credits: ArtistCredit[];
};

export type WorkspaceMode = "none" | "collection" | "loose" | (string & {});

export type WorkspaceState = {
  mode: WorkspaceMode;
  collection?: CollectionSummary | null;
  galleries: GallerySummary[];
  selected_gallery_id?: number | null;
  artworks: ArtworkSummary[];
};

export type MergeGalleryRequest = {
  collection_id: number;
  source_gallery_id: number;
  target_gallery_id: number;
  name: string;
  caf_gallery_room_id?: string | null;
  raremarq_gallery_id?: string | null;
  snikt_gallery_inherits_collection: boolean;
};

export type MergeArtworkRequest = {
  collection_id: number;
  source_gallery_id: number;
  source_artwork_id: number;
  target_artwork_id: number;
  metadata: MetadataSaveRequest;
};

export type AddArtworkToGalleryRequest = {
  collection_id: number;
  artwork_id: number;
  gallery_id: number;
};

export type CafImportReport = {
  caf_collection_id: string;
  collection: CollectionSummary;
  galleries_imported: number;
  artworks_imported: number;
  images_downloaded: number;
  image_download_failures: number;
  skipped_artworks: number;
  missing_artworks: CafMissingArtworkReportRow[];
  reconciliation_items: CafImportReconciliationItem[];
  debug_log_path?: string | null;
  messages: string[];
};

export type CafImportReconciliationCandidate = {
  artwork_id: number;
  display_id: string;
  title: string;
  thumbnail_path?: string | null;
};

export type CafImportReconciliationItem = {
  gallery_id: number;
  gallery_name: string;
  row: CafImportReconciliationRow;
  candidates: CafImportReconciliationCandidate[];
};

export type CafImportReconciliationRow = {
  csv_row_number?: number;
  gcat: string;
  gsub: string;
  image_link: string;
  full_image_url: string;
  added_to_caf?: string | null;
  title: string;
  description?: string | null;
  for_sale_status?: string | null;
  media_type_id?: string | null;
  art_type_id?: string | null;
  artist_credits: ArtistCredit[];
  purchase_price?: string | null;
  estimated_value?: string | null;
  purchase_date?: string | null;
  personal_notes?: string | null;
};

export type CafMissingArtworkReportRow = {
  image_link: string;
  title: string;
  artists: string;
  media_type: string;
  art_type: string;
  for_sale: string;
  added_to_caf: string;
  description: string;
  purchase_date: string;
  purchase_price: string;
  estimated_value: string;
  personal_notes: string;
};

export type CafImportProgress = {
  phase: string;
  message: string;
  galleries_total: number;
  galleries_imported: number;
  artworks_total: number;
  artworks_imported: number;
  images_downloaded: number;
  image_download_failures: number;
  skipped_artworks: number;
  current_gallery?: string | null;
  current_artwork?: string | null;
  done: boolean;
};

export type WorkspaceLoadProgress = {
  phase: string;
  message: string;
  artworks_total: number;
  artworks_loaded: number;
  current_artwork?: string | null;
  done: boolean;
};

export type SniktImportReport = {
  snikt_collection_id: string;
  collection: CollectionSummary;
  galleries_imported: number;
  artworks_imported: number;
  images_downloaded: number;
  image_download_failures: number;
  reconciliation_items: SniktImportReconciliationItem[];
  messages: string[];
};

export type SniktImportReconciliationCandidate = {
  artwork_id: number;
  display_id: string;
  title: string;
  thumbnail_path?: string | null;
};

export type SniktImportReconciliationItem = {
  gallery_id: number;
  gallery_name: string;
  row: SniktImportReconciliationRow;
  candidates: SniktImportReconciliationCandidate[];
};

export type SniktImportReconciliationRow = {
  title: string;
  created_date?: string | null;
  description?: string | null;
  active: boolean;
  artist_credits: ArtistCredit[];
  snikt_metadata: SniktMetadata;
  estimated_value?: string | null;
};

export type SniktImportProgress = {
  phase: string;
  message: string;
  artworks_total: number;
  artworks_imported: number;
  images_downloaded: number;
  image_download_failures: number;
  current_artwork?: string | null;
  done: boolean;
};

export type RaremarqCsvExportScope = "all" | "untracked";
export type RaremarqCsvUrlMode = "generic_url" | "blank" | "tmpfiles";

export type RaremarqCsvExportPlanScope = {
  rows_exported: number;
  duplicate_raremarq_url_count: number;
  generic_url_blank_count: number;
  blank_url_count: number;
  tmpfiles_upload_count: number;
  tmpfiles_missing_file_count: number;
  tmpfiles_unrenderable_file_count: number;
  tmpfiles_large_file_count: number;
};

export type RaremarqCsvExportPlan = {
  collection_id: number;
  total_artworks: number;
  raremarq_tracked_artworks: number;
  all: RaremarqCsvExportPlanScope;
  untracked: RaremarqCsvExportPlanScope;
};

export type RaremarqCsvExportProgress = {
  phase: string;
  message: string;
  current: number;
  total: number;
  done: boolean;
};

export type FileAsset = {
  id: number;
  artwork_id: number;
  original_path: string;
  current_path: string;
  relative_path: string;
  file_name: string;
  extension: string;
  size_bytes: number;
  width?: number | null;
  height?: number | null;
  dpi_x?: number | null;
  dpi_y?: number | null;
  image_role?: ImageRole | null;
  source_kind: FileSourceKind;
  is_primary: boolean;
};

export type DerivedAsset = {
  id: number;
  artwork_id: number;
  source_file_asset_id?: number | null;
  derivative_type: string;
  format: string;
  path: string;
  width: number;
  height: number;
  image_role?: ImageRole | null;
};

export type DeleteFilePreview = {
  path: string;
  label: string;
  reason: string;
};

export type DeletePreview = {
  files_to_trash: DeleteFilePreview[];
};

export type DeleteTrashFailure = {
  path: string;
  error: string;
};

export type DeleteResult = {
  trashed_files: DeleteFilePreview[];
  trash_failures: DeleteTrashFailure[];
};

export type DeleteArtworkFileResult = {
  detail: ArtworkDetail;
  result: DeleteResult;
};

export type FileRenamePlan = {
  asset_kind: AssetKind;
  asset_id: number;
  artwork_id: number;
  current_path: string;
  new_path: string;
  new_file_name: string;
  physical_file_rename: boolean;
};

export type FileRenameExecution = {
  plan: FileRenamePlan;
  confirmed_physical_file_rename: boolean;
};

export type FileRenameResult = {
  detail: ArtworkDetail;
  plan: FileRenamePlan;
  renamed: boolean;
  rolled_back: boolean;
};

export type ArtworkDetail = {
  id: number;
  canonical_id: string;
  display_id?: string;
  caf_artwork_id?: string | null;
  snikt_artwork_id?: string | null;
  raremarq_artwork_id?: string | null;
  title: string;
  description?: string | null;
  for_sale_status?: string | null;
  media_type_id?: string | null;
  media?: string | null;
  art_type_id?: string | null;
  format?: string | null;
  publication_status_id?: string | null;
  active: boolean;
  illustration_exchange: boolean;
  ix_for_sale: boolean;
  caf_url?: string | null;
  snikt_url?: string | null;
  raremarq_url?: string | null;
  generic_url?: string | null;
  snikt_metadata: SniktMetadata;
  purchase_price?: string | null;
  estimated_value?: string | null;
  purchase_date?: string | null;
  provenance?: string | null;
  personal_notes?: string | null;
  source_folder: string;
  artist_credits: ArtistCredit[];
  file_assets: FileAsset[];
  derived_assets: DerivedAsset[];
  cache_warnings?: ArtworkCacheWarning[];
};

export type ArtworkCacheWarning = {
  file_asset_id: number;
  path: string;
  message: string;
};

export type SniktMetadata = {
  art_type?: string | null;
  comic_publisher?: string | null;
  series_title?: string | null;
  issue_number?: string | null;
  series_page_number?: string | null;
  year?: string | null;
  character?: string | null;
  subcategory?: string | null;
  animation_studio?: string | null;
  episode_number?: string | null;
  episode_title?: string | null;
  published_date?: string | null;
  strip_title?: string | null;
  is_sunday_strip: boolean;
  other?: string | null;
  tags?: string | null;
  is_nsfw: boolean;
  is_for_sale: boolean;
  price?: string | null;
  is_open_to_offers: boolean;
};

export type DetailForm = {
  title: string;
  description: string;
  forSaleStatus: string;
  mediaTypeId: string;
  artTypeId: string;
  publicationStatusId: string;
  active: boolean;
  illustrationExchange: boolean;
  ixForSale: boolean;
  artistCredits: ArtistCreditForm[];
  cafUrl: string;
  sniktUrl: string;
  raremarqUrl: string;
  genericUrl: string;
  sniktMetadata: SniktMetadataForm;
  purchasePrice: string;
  estimatedValue: string;
  purchaseDate: string;
  provenance: string;
  personalNotes: string;
};

export type SniktMetadataForm = {
  artType: string;
  comicPublisher: string;
  seriesTitle: string;
  issueNumber: string;
  seriesPageNumber: string;
  year: string;
  character: string;
  subcategory: string;
  animationStudio: string;
  episodeNumber: string;
  episodeTitle: string;
  publishedDate: string;
  stripTitle: string;
  isSundayStrip: boolean;
  other: string;
  tags: string;
  isNsfw: boolean;
  isForSale: boolean;
  price: string;
  isOpenToOffers: boolean;
};

export type ArtistCreditForm = {
  firstName: string;
  lastName: string;
  roleId: string;
};

export type WorkspaceCommandMode =
  | "new_collection"
  | "open_collection"
  | "import_caf_collection"
  | "import_oaa_archive"
  | "import_snikt_collection"
  | "new_gallery";
export type AttachMode = "copy" | "link";
export type ArtworkIdLabelPreference = "oac" | "caf" | "snikt" | "raremarq";
export type DefaultProviderFocus = "all" | "caf" | "snikt" | "raremarq";
export type ThemePreference = "dracula" | "alucard";
export type StartupBehaviorPreference = "reopen_last" | "show_start_window" | "start_empty";

export type AppPreferences = {
  default_attach_mode: AttachMode;
  default_png_export_variant: PngExportVariant;
  default_provider_focus: DefaultProviderFocus;
  artwork_id_label_preference: ArtworkIdLabelPreference;
  theme: ThemePreference;
  startup_behavior: StartupBehaviorPreference;
  default_workspace_root: string;
  raremarq_csv_export_scope: RaremarqCsvExportScope;
  raremarq_csv_url_mode: RaremarqCsvUrlMode;
};

export type DragDropEventPayload = {
  type: string;
  paths?: string[];
  altKey?: boolean;
  modifiers?: {
    altKey?: boolean;
  };
};

export type OaaImportReport = {
  collection_id: number;
  galleries_imported: number;
  artworks_imported: number;
  files_imported: number;
  messages: string[];
};

export type OaaImportProgress = {
  phase: string;
  message: string;
  galleries_total: number;
  galleries_imported: number;
  artworks_total: number;
  artworks_imported: number;
  files_total: number;
  files_imported: number;
  current_artwork?: string | null;
  done: boolean;
};

export type OaaExportReport = {
  collection_id: number;
  archive_path: string;
  galleries_exported: number;
  artworks_exported: number;
  files_exported: number;
};

export type OaaExportProgress = {
  phase: string;
  message: string;
  current: number;
  total: number;
};

export type ThumbnailCacheProgress = {
  phase: string;
  message: string;
  total: number;
  completed: number;
  succeeded: number;
  failed: number;
  current_path?: string | null;
  done: boolean;
};

export type RaremarqCsvExportReport = {
  collection_id: number;
  csv_path: string;
  rows_exported: number;
  rows_missing_primary_image_url: number;
  rows_skipped_existing_raremarq_url: number;
  tmpfiles_uploaded: number;
  tmpfiles_resized: number;
  messages: string[];
};

export type CarouselImageItem = {
  kind: AssetKind;
  key: string;
  id: number;
  name: string;
  path: string;
  format: string;
  width?: number | null;
  height?: number | null;
  dpi_x?: number | null;
  dpi_y?: number | null;
  image_role?: ImageRole | null;
  status: string;
  thumbnailPath?: string | null;
  previewPath?: string | null;
  thumbnailSource?: ImageDataUrlSource | null;
  previewSource?: ImageDataUrlSource | null;
  sourceFileAssetId?: number | null;
};

export type ImageDataUrlSource =
  | { kind: "cache"; path: string }
  | { kind: "file_asset"; fileAssetId: number }
  | { kind: "derived_asset"; derivedAssetId: number };
