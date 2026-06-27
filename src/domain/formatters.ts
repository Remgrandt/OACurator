import { ARTIST_ROLE_OPTIONS, ART_TYPE_OPTIONS, MEDIA_TYPE_OPTIONS } from "./constants";
import type {
  ArtworkDetail,
  ArtworkSummary,
  CarouselImageItem,
  DetailForm,
  WorkspaceCommandMode,
} from "./types";

export function artistGroups(records: ArtworkSummary[]) {
  const groups = new Map<string, ArtworkSummary[]>();
  for (const record of records) {
    const artist = record.artist_credits[0]?.name || "Unknown Artist";
    groups.set(artist, [...(groups.get(artist) ?? []), record]);
  }
  return Array.from(groups.entries())
    .sort(([left], [right]) => left.localeCompare(right))
    .map(([artist, groupedRecords]) => ({ artist, records: groupedRecords }));
}

export function requiresWorkspaceCommandName(command: WorkspaceCommandMode): boolean {
  return command === "new_collection" || command === "new_gallery";
}

export function workspaceCommandTitle(command: WorkspaceCommandMode): string {
  if (command === "new_collection") return "New Collection";
  if (command === "open_collection") return "Open Collection";
  if (command === "import_caf_collection") return "Import CAF Collection";
  if (command === "import_oaa_archive") return "Import OAA Archive";
  if (command === "import_snikt_collection") return "Import SNIKT.com Collection";
  return "New Gallery";
}

export function workspaceCommandSubmitLabel(command: WorkspaceCommandMode): string {
  if (command === "new_collection") return "Create Collection";
  if (command === "open_collection") return "Open Collection Manifest";
  if (command === "import_caf_collection") return "Import CAF Collection";
  if (command === "import_oaa_archive") return "Import OAA Archive";
  if (command === "import_snikt_collection") return "Import SNIKT.com Collection";
  return "Create Gallery";
}

export function workspaceCommandPlaceholder(command: WorkspaceCommandMode): string {
  if (
    command === "import_caf_collection" ||
    command === "import_oaa_archive" ||
    command === "import_snikt_collection"
  )
    return "~/OACurator/";
  if (command === "new_collection" || command === "open_collection") {
    return "~/OACurator/Personal/.oacollection";
  }
  return "~/OACurator/Personal/galleries/Gallery/.oagallery";
}

export function workspaceCommandSourceFilePlaceholder(command: WorkspaceCommandMode): string {
  if (command === "import_oaa_archive") return "~/Downloads/collection.oaa";
  if (command === "import_snikt_collection") return "~/Downloads/snikt.csv";
  return "~/Downloads/caf.csv";
}

export function workspaceCommandExtension(command: WorkspaceCommandMode): string {
  return command === "new_collection" ||
    command === "open_collection" ||
    command === "import_caf_collection" ||
    command === "import_oaa_archive" ||
    command === "import_snikt_collection"
    ? "oacollection"
    : "oagallery";
}

export function suggestedWorkspaceManifestPath(
  command: WorkspaceCommandMode,
  name: string,
  currentPath: string,
): string {
  const stem = suggestedManifestStem(name);
  const root = manifestDirectoryPrefix(currentPath, command);
  if (!stem) return root;
  const separator = pathSeparatorFor(root || currentPath);
  return `${root}${stem}${separator}.${workspaceCommandExtension(command)}`;
}

export function suggestedManifestStem(name: string): string {
  return (
    name
      .trim()
      // eslint-disable-next-line no-control-regex -- Windows filenames cannot contain ASCII control characters.
      .replace(/[<>:"/\\|?*\u0000-\u001f]+/g, " ")
      .replace(/\s+/g, " ")
      .replace(/[. ]+$/g, "")
  );
}

export function manifestDirectoryPrefix(path: string, command: WorkspaceCommandMode): string {
  const value = path.trim();
  if (!value) return "";
  const separatorIndex = Math.max(value.lastIndexOf("\\"), value.lastIndexOf("/"));
  if (isDirectoryLikeManifestPath(value, command)) {
    return value.endsWith("\\") || value.endsWith("/")
      ? value
      : `${value}${pathSeparatorFor(value)}`;
  }
  return separatorIndex >= 0 ? value.slice(0, separatorIndex + 1) : "";
}

export function isDirectoryLikeManifestPath(path: string, command: WorkspaceCommandMode): boolean {
  const value = path.trim();
  if (!value) return true;
  if (value.endsWith("\\") || value.endsWith("/")) return true;
  const separatorIndex = Math.max(value.lastIndexOf("\\"), value.lastIndexOf("/"));
  const leaf = value.slice(separatorIndex + 1).toLowerCase();
  return !leaf.endsWith(`.${workspaceCommandExtension(command)}`);
}

export function ensureTrailingPathSeparator(path: string): string {
  const value = path.trim();
  if (!value || value.endsWith("\\") || value.endsWith("/")) return value;
  return `${value}${pathSeparatorFor(value)}`;
}

export function parentDirectory(path: string): string {
  const value = path.trim();
  const separatorIndex = Math.max(value.lastIndexOf("\\"), value.lastIndexOf("/"));
  return separatorIndex >= 0 ? value.slice(0, separatorIndex + 1) : "";
}

function pathSeparatorFor(path: string): "\\" | "/" {
  const value = path.trim();
  return value.lastIndexOf("/") > value.lastIndexOf("\\") ? "/" : "\\";
}

export function carouselItemsForDetail(detail: ArtworkDetail): CarouselImageItem[] {
  const fileItems = detail.file_assets.map((asset): CarouselImageItem => {
    const previewable = isRenderableImageExtension(asset.extension);
    const thumbnailAsset = derivedAssetForFile(detail, asset.id, "thumbnail");
    const previewAsset = derivedAssetForFile(detail, asset.id, "preview");
    const thumbnailPath = previewable ? (thumbnailAsset?.path ?? null) : null;
    const previewPath = previewable ? (previewAsset?.path ?? asset.current_path) : null;
    return {
      kind: "file",
      key: `file:${asset.id}`,
      id: asset.id,
      name: asset.file_name,
      path: asset.current_path,
      format: asset.extension,
      width: asset.width ?? null,
      height: asset.height ?? null,
      dpi_x: asset.dpi_x ?? null,
      dpi_y: asset.dpi_y ?? null,
      image_role: asset.image_role ?? null,
      status: fileSourceStatus(asset.source_kind),
      thumbnailPath,
      previewPath,
      thumbnailSource: thumbnailAsset ? { kind: "cache", path: thumbnailAsset.path } : null,
      previewSource: previewAsset
        ? { kind: "cache", path: previewAsset.path }
        : previewPath
          ? { kind: "file_asset", fileAssetId: asset.id }
          : null,
    };
  });

  return fileItems;
}

export function isRenderableImageExtension(extension: string): boolean {
  switch (extension.trim().toLowerCase()) {
    case "jpg":
    case "jpeg":
    case "png":
    case "tif":
    case "tiff":
      return true;
    default:
      return false;
  }
}

function fileSourceStatus(sourceKind: string): string {
  switch (sourceKind) {
    case "copied":
      return "Copied";
    case "imported":
      return "Imported";
    case "linked":
      return "Linked";
    default:
      return "Unknown";
  }
}

export function derivedAssetForFile(
  detail: ArtworkDetail,
  fileAssetId: number,
  derivativeType: string,
) {
  return detail.derived_assets.find(
    (asset) =>
      asset.source_file_asset_id === fileAssetId && asset.derivative_type === derivativeType,
  );
}

export function formatDimensions(item: CarouselImageItem): string {
  if (!item.width || !item.height) return "Dimensions unknown";
  return `${item.width} x ${item.height} px`;
}

export function formatDpi(item: CarouselImageItem): string {
  if (!item.dpi_x || !item.dpi_y) return "DPI unknown";
  if (item.dpi_x === item.dpi_y) return `${formatNumber(item.dpi_x)} DPI`;
  return `${formatNumber(item.dpi_x)} x ${formatNumber(item.dpi_y)} DPI`;
}

export function formatImageFormat(value: string): string {
  const format = value.trim().toLowerCase();
  if (format === "tif" || format === "tiff") return "TIFF";
  if (format === "jpg" || format === "jpeg") return "JPEG";
  if (format === "png") return "PNG";
  return value.toUpperCase();
}

export function formatNumber(value: number): string {
  return Number.isInteger(value) ? value.toFixed(0) : value.toString();
}

export function fileNameFromPath(path: string): string {
  const separatorIndex = Math.max(path.lastIndexOf("\\"), path.lastIndexOf("/"));
  return separatorIndex >= 0 ? path.slice(separatorIndex + 1) : path;
}

export function formFromDetail(detail: ArtworkDetail): DetailForm {
  return {
    title: detail.title,
    description: detail.description ?? "",
    forSaleStatus: detail.for_sale_status ?? "NFS",
    mediaTypeId: detail.media_type_id ?? idForLabel(MEDIA_TYPE_OPTIONS, detail.media) ?? "7",
    artTypeId: detail.art_type_id ?? idForLabel(ART_TYPE_OPTIONS, detail.format) ?? "3",
    publicationStatusId: detail.publication_status_id ?? "2",
    active: detail.active,
    illustrationExchange: detail.illustration_exchange,
    ixForSale: detail.ix_for_sale,
    artistCredits:
      detail.artist_credits.length > 0
        ? detail.artist_credits.map((credit) => ({
            firstName: credit.first_name ?? firstNameFromDisplayName(credit.name),
            lastName: credit.last_name ?? lastNameFromDisplayName(credit.name),
            roleId: credit.role_id ?? idForLabel(ARTIST_ROLE_OPTIONS, credit.role) ?? "",
          }))
        : [{ firstName: "", lastName: "", roleId: "" }],
    cafUrl: detail.caf_url ?? "",
    sniktUrl: detail.snikt_url ?? "",
    raremarqUrl: detail.raremarq_url ?? "",
    genericUrl: detail.generic_url ?? "",
    sniktMetadata: {
      artType: detail.snikt_metadata?.art_type ?? "",
      comicPublisher: detail.snikt_metadata?.comic_publisher ?? "",
      seriesTitle: detail.snikt_metadata?.series_title ?? "",
      issueNumber: detail.snikt_metadata?.issue_number ?? "",
      seriesPageNumber: detail.snikt_metadata?.series_page_number ?? "",
      year: detail.snikt_metadata?.year ?? "",
      character: detail.snikt_metadata?.character ?? "",
      subcategory: detail.snikt_metadata?.subcategory ?? "",
      animationStudio: detail.snikt_metadata?.animation_studio ?? "",
      episodeNumber: detail.snikt_metadata?.episode_number ?? "",
      episodeTitle: detail.snikt_metadata?.episode_title ?? "",
      publishedDate: detail.snikt_metadata?.published_date ?? "",
      stripTitle: detail.snikt_metadata?.strip_title ?? "",
      isSundayStrip: detail.snikt_metadata?.is_sunday_strip ?? false,
      other: detail.snikt_metadata?.other ?? "",
      tags: detail.snikt_metadata?.tags ?? "",
      isNsfw: detail.snikt_metadata?.is_nsfw ?? false,
      isForSale: detail.snikt_metadata?.is_for_sale ?? false,
      price: detail.snikt_metadata?.price ?? "",
      isOpenToOffers: detail.snikt_metadata?.is_open_to_offers ?? false,
    },
    purchasePrice: detail.purchase_price ?? "",
    estimatedValue: detail.estimated_value ?? "",
    purchaseDate: detail.purchase_date ?? "",
    provenance: detail.provenance ?? "",
    personalNotes: detail.personal_notes ?? "",
  };
}

export function idForLabel(
  options: { id: string; label: string }[],
  label?: string | null,
): string | null {
  if (!label) return null;
  return (
    options.find((option) => option.label.toLowerCase() === label.trim().toLowerCase())?.id ?? null
  );
}

export function labelForId(options: { id: string; label: string }[], id: string): string | null {
  const value = id.trim();
  if (!value) return null;
  return options.find((option) => option.id === value)?.label ?? null;
}

export function firstNameFromDisplayName(name: string): string {
  const parts = name.trim().split(/\s+/).filter(Boolean);
  if (parts.length <= 1) return "";
  return parts.slice(0, -1).join(" ");
}

export function lastNameFromDisplayName(name: string): string {
  const parts = name.trim().split(/\s+/).filter(Boolean);
  return parts[parts.length - 1] ?? "";
}

export function blankToNull(value: string): string | null {
  const trimmed = value.trim();
  return trimmed ? trimmed : null;
}
