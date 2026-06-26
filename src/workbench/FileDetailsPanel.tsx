import type { ChangeEvent, ReactNode } from "react";
import type { CarouselImageItem, PngExportVariant } from "../domain/types";
import { formatDimensions, formatDpi, formatImageFormat } from "../domain/formatters";

type FileDetailsPanelProps = {
  selectedCarouselItem: CarouselImageItem | null;
  pngExportVariant: PngExportVariant;
  exportDestination: string;
  showPngExportControls: boolean;
  canCreatePngExport: boolean;
  onImageRoleChange: (event: ChangeEvent<HTMLSelectElement>) => void;
  onCopyPath: () => void;
  onShowInExplorer: () => void;
  onPngExportVariantChange: (variant: PngExportVariant) => void;
  onExportDestinationChange: (value: string) => void;
  onCreatePngExport: () => void;
};

export function FileDetailsPanel({
  selectedCarouselItem,
  pngExportVariant,
  exportDestination,
  showPngExportControls,
  canCreatePngExport,
  onImageRoleChange,
  onCopyPath,
  onShowInExplorer,
  onPngExportVariantChange,
  onExportDestinationChange,
  onCreatePngExport,
}: FileDetailsPanelProps) {
  const hasDimensions = Boolean(selectedCarouselItem?.width && selectedCarouselItem.height);
  const hasDpi = Boolean(selectedCarouselItem?.dpi_x || selectedCarouselItem?.dpi_y);

  return (
    <section className="selected-image-details" aria-label="Selected File Details">
      <div className="image-details-layout">
        {selectedCarouselItem ? (
          <div className="image-detail-card" role="group" aria-label="File metadata">
            <div className="image-detail-title">
              <strong title={selectedCarouselItem.name}>{selectedCarouselItem.name}</strong>
              <span>{selectedCarouselItem.status}</span>
              <button type="button" onClick={onShowInExplorer}>
                Show in Explorer
              </button>
            </div>
            <dl className="image-fact-strip">
              {hasDimensions ? (
                <FileFact label="Dimensions">{formatDimensions(selectedCarouselItem)}</FileFact>
              ) : null}
              {hasDpi ? <FileFact label="DPI">{formatDpi(selectedCarouselItem)}</FileFact> : null}
              <FileFact label="Format">{formatImageFormat(selectedCarouselItem.format)}</FileFact>
            </dl>
            <dl className="image-metadata-grid">
              <FileMetadataRow label="Path">
                <span className="image-path-control">
                  <button
                    type="button"
                    className="icon-button image-path-copy-button"
                    aria-label="Copy path"
                    title="Copy path"
                    onClick={onCopyPath}
                  >
                    <span className="svg-icon icon-copy" aria-hidden="true" />
                  </button>
                  <span className="image-path-value" title={selectedCarouselItem.path}>
                    {selectedCarouselItem.path}
                  </span>
                </span>
              </FileMetadataRow>
            </dl>
            <label className="property-row image-property-row">
              <span className="property-key">File role</span>
              <span className="property-value">
                <select value={selectedCarouselItem.image_role ?? ""} onChange={onImageRoleChange}>
                  <option value=""></option>
                  <option value="raw_scan">Raw Scan</option>
                  <option value="raw_photo">Raw Photo</option>
                  <option value="corrected_scan">Corrected Scan</option>
                  <option value="detail">Detail</option>
                  <option value="verso">Verso</option>
                  <option value="reference">Reference</option>
                  <option value="basic">Basic - 800px height</option>
                  <option value="premium">Premium - 2000px height</option>
                </select>
              </span>
            </label>
          </div>
        ) : (
          <p className="empty-state">No file selected.</p>
        )}
        {selectedCarouselItem && showPngExportControls ? (
          <div className="image-export-card export-box" role="group" aria-label="Preview/export">
            <span className="property-block-label">Preview/export</span>
            <label className="property-row">
              <span className="property-key">Format</span>
              <span className="property-value">
                <select
                  aria-label="PNG export format"
                  value={pngExportVariant}
                  onChange={(event) =>
                    onPngExportVariantChange(event.currentTarget.value as PngExportVariant)
                  }
                >
                  <option value="basic">Basic - 800px height</option>
                  <option value="premium">Premium - 2000px height</option>
                </select>
              </span>
            </label>
            <label className="property-row">
              <span className="property-key">Destination</span>
              <span className="property-value">
                <input
                  aria-label="Export destination"
                  value={exportDestination}
                  onChange={(event) => onExportDestinationChange(event.currentTarget.value)}
                />
              </span>
            </label>
            <div className="button-row image-export-actions">
              <button type="button" disabled={!canCreatePngExport} onClick={onCreatePngExport}>
                Create PNG export
              </button>
            </div>
          </div>
        ) : null}
      </div>
    </section>
  );
}

function FileFact({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="image-fact">
      <dt className="property-key">{label}</dt>
      <dd className="property-value">{children}</dd>
    </div>
  );
}

function FileMetadataRow({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="property-row image-property-row">
      <dt className="property-key">{label}</dt>
      <dd className="property-value">{children}</dd>
    </div>
  );
}
