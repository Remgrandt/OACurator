import type { RefObject } from "react";
import type {
  CafImportReconciliationItem,
  GallerySummary,
  SniktImportReconciliationItem,
  UnreferencedArtworkReport,
} from "../../domain/types";

export type CafReconciliationState = {
  items: CafImportReconciliationItem[];
  index: number;
  isResolving: boolean;
};

export type SniktReconciliationState = {
  items: SniktImportReconciliationItem[];
  index: number;
  isResolving: boolean;
};

export type UnreferencedArtworkReconciliationState = {
  report: UnreferencedArtworkReport;
  selectedPaths: string[];
  galleryId: string;
  isImporting: boolean;
};

export function UnreferencedArtworkDialog({
  reconciliation,
  galleries,
  onChange,
  onImport,
  onIgnore,
}: {
  reconciliation: UnreferencedArtworkReconciliationState | null;
  galleries: GallerySummary[];
  onChange: (next: UnreferencedArtworkReconciliationState) => void;
  onImport: () => void;
  onIgnore: () => void;
}) {
  if (!reconciliation) return null;
  const importableCount = reconciliation.report.items.filter((item) => item.can_import).length;
  const togglePath = (path: string, selected: boolean) => {
    onChange({
      ...reconciliation,
      selectedPaths: selected
        ? [...reconciliation.selectedPaths, path]
        : reconciliation.selectedPaths.filter((candidate) => candidate !== path),
    });
  };

  return (
    <div className="workspace-command-backdrop">
      <section
        className="workspace-command workspace-command-modal unreferenced-artwork-modal"
        role="dialog"
        aria-modal="true"
        aria-labelledby="unreferenced-artwork-title"
      >
        <h3 id="unreferenced-artwork-title">Unreferenced Artwork Found</h3>
        <p className="workspace-command-warning">
          OA Curator found {reconciliation.report.items.length} Artwork{" "}
          {reconciliation.report.items.length === 1 ? "folder" : "folders"} on disk that this
          Collection does not reference. Import selected records or leave every file untouched.
        </p>
        <div className="unreferenced-artwork-list">
          {reconciliation.report.items.map((item) => {
            const selected = reconciliation.selectedPaths.includes(item.manifest_path);
            return (
              <article className="unreferenced-artwork-item" key={item.manifest_path}>
                <label>
                  <input
                    type="checkbox"
                    aria-label={`Select ${item.canonical_id} for import`}
                    checked={selected}
                    disabled={!item.can_import || reconciliation.isImporting}
                    onChange={(event) =>
                      togglePath(item.manifest_path, event.currentTarget.checked)
                    }
                  />
                  <strong>
                    {item.canonical_id} {item.title || "Unreadable Artwork record"}
                  </strong>
                </label>
                <span className="unreferenced-artwork-path">{item.manifest_path}</span>
                {item.error && <span className="unreferenced-artwork-error">{item.error}</span>}
                <span>
                  {item.declared_file_count} declared; {item.undeclared_files.length} undeclared;{" "}
                  {item.missing_declared_files.length} missing
                </span>
                {item.undeclared_files.length > 0 && (
                  <span>Undeclared: {item.undeclared_files.join(", ")}</span>
                )}
                {item.duplicate_candidates.length > 0 && (
                  <span>
                    Possible duplicate:{" "}
                    {item.duplicate_candidates
                      .map((candidate) => candidate.canonical_id + " " + candidate.title)
                      .join(", ")}
                  </span>
                )}
              </article>
            );
          })}
        </div>
        {importableCount > 0 && (
          <label>
            Import selected Artwork into Gallery
            <select
              aria-label="Gallery for imported Artwork"
              value={reconciliation.galleryId}
              disabled={reconciliation.isImporting}
              onChange={(event) =>
                onChange({ ...reconciliation, galleryId: event.currentTarget.value })
              }
            >
              {galleries.map((gallery) => (
                <option key={gallery.id} value={gallery.id}>
                  {gallery.name}
                </option>
              ))}
            </select>
          </label>
        )}
        <div className="workspace-command-actions">
          <button
            type="button"
            className="primary"
            disabled={
              reconciliation.selectedPaths.length === 0 ||
              !reconciliation.galleryId ||
              reconciliation.isImporting
            }
            onClick={onImport}
          >
            {reconciliation.isImporting ? "Importing..." : "Import selected"}
          </button>
          <button type="button" disabled={reconciliation.isImporting} onClick={onIgnore}>
            Leave files untouched
          </button>
        </div>
      </section>
    </div>
  );
}

type ReconciliationDialogProps<TState> = {
  reconciliation: TState | null;
  dialogRef: RefObject<HTMLElement | null>;
  thumbUrls: Record<number, string | null>;
  onOpenUrl?: (label: string, url: string) => void;
  onResolve: (targetArtworkId: number | null) => void;
  onSkip: () => void;
};

export function CafReconciliationDialog({
  reconciliation,
  dialogRef,
  thumbUrls,
  onOpenUrl,
  onResolve,
  onSkip,
}: ReconciliationDialogProps<CafReconciliationState>) {
  if (!reconciliation) return null;
  const item = reconciliation.items[reconciliation.index];
  if (!item) return null;
  const row = item.row;
  const csvImageUrl = row.image_link || row.full_image_url;
  const artists = row.artist_credits
    .map((credit) => {
      const splitName = [credit.first_name, credit.last_name].filter(Boolean).join(" ");
      return splitName || credit.name || "";
    })
    .filter(Boolean)
    .join(", ");
  const csvRowLabel = row.csv_row_number ? `CSV row ${row.csv_row_number}` : "CAF CSV row";
  const progressLabel = `${reconciliation.index + 1} of ${reconciliation.items.length}`;

  return (
    <div className="workspace-command-backdrop">
      <section
        ref={dialogRef}
        className="workspace-command workspace-command-modal caf-reconciliation-modal"
        role="dialog"
        aria-modal="true"
        aria-labelledby="caf-reconciliation-title"
      >
        <h3 id="caf-reconciliation-title">Resolve CAF CSV Match</h3>
        <p className="workspace-command-warning">
          {csvRowLabel} ({progressLabel}) has the same title as existing Artwork in{" "}
          {item.gallery_name}. Choose an existing Artwork, import the CSV row as new, or skip it.
        </p>

        <div className="caf-reconciliation-grid">
          <span>Title</span>
          <strong>{row.title}</strong>
          <span>Gallery</span>
          <span>
            {item.gallery_name} (GSub {row.gsub})
          </span>
          <span>CAF collection</span>
          <span>GCat {row.gcat}</span>
          <span>Added to CAF</span>
          <span>{row.added_to_caf || "Not provided"}</span>
          <span>Artists</span>
          <span>{artists || "Not provided"}</span>
          <span>Media / type</span>
          <span>
            {[row.media_type_id, row.art_type_id].filter(Boolean).join(" / ") || "Not provided"}
          </span>
          <span>CAF image URL</span>
          <span>
            {csvImageUrl && onOpenUrl ? (
              <button
                type="button"
                className="link-button"
                onClick={() => onOpenUrl("CAF CSV image URL", csvImageUrl)}
              >
                Open URL
              </button>
            ) : (
              "Not provided"
            )}
          </span>
        </div>

        {row.description && (
          <div className="caf-reconciliation-description">
            <strong>Description</strong>
            <p>{row.description}</p>
          </div>
        )}

        <CandidateList
          candidates={item.candidates}
          isResolving={reconciliation.isResolving}
          thumbUrls={thumbUrls}
          onResolve={onResolve}
        />

        <ReconciliationActions
          isResolving={reconciliation.isResolving}
          onResolveAsNew={() => onResolve(null)}
          onSkip={onSkip}
        />
      </section>
    </div>
  );
}

export function SniktReconciliationDialog({
  reconciliation,
  dialogRef,
  thumbUrls,
  onResolve,
  onSkip,
}: ReconciliationDialogProps<SniktReconciliationState>) {
  if (!reconciliation) return null;
  const item = reconciliation.items[reconciliation.index];
  if (!item) return null;
  const row = item.row;
  const artists = row.artist_credits
    .map((credit) => {
      const splitName = [credit.first_name, credit.last_name].filter(Boolean).join(" ");
      return splitName || credit.name || "";
    })
    .filter(Boolean)
    .join(", ");
  const progressLabel = `${reconciliation.index + 1} of ${reconciliation.items.length}`;

  return (
    <div className="workspace-command-backdrop">
      <section
        ref={dialogRef}
        className="workspace-command workspace-command-modal caf-reconciliation-modal"
        role="dialog"
        aria-modal="true"
        aria-labelledby="snikt-reconciliation-title"
      >
        <h3 id="snikt-reconciliation-title">Resolve SNIKT.com CSV Match</h3>
        <p className="workspace-command-warning">
          SNIKT.com CSV row {progressLabel} may match existing Artwork in {item.gallery_name}.
          Choose an existing Artwork, import the CSV row as new, or skip it.
        </p>

        <div className="caf-reconciliation-grid">
          <span>Title</span>
          <strong>{row.title}</strong>
          <span>Gallery</span>
          <span>{item.gallery_name}</span>
          <span>Created date</span>
          <span>{row.created_date || "Not provided"}</span>
          <span>Artists</span>
          <span>{artists || "Not provided"}</span>
          <span>SNIKT art type</span>
          <span>{row.snikt_metadata.art_type || "Not provided"}</span>
          <span>Estimated value</span>
          <span>{row.estimated_value || "Not provided"}</span>
        </div>

        {row.description && (
          <div className="caf-reconciliation-description">
            <strong>Description</strong>
            <p>{row.description}</p>
          </div>
        )}

        <CandidateList
          candidates={item.candidates}
          isResolving={reconciliation.isResolving}
          thumbUrls={thumbUrls}
          onResolve={onResolve}
        />

        <ReconciliationActions
          isResolving={reconciliation.isResolving}
          onResolveAsNew={() => onResolve(null)}
          onSkip={onSkip}
        />
      </section>
    </div>
  );
}

type Candidate = {
  artwork_id: number;
  display_id: string;
  title: string;
};

function CandidateList({
  candidates,
  isResolving,
  thumbUrls,
  onResolve,
}: {
  candidates: Candidate[];
  isResolving: boolean;
  thumbUrls: Record<number, string | null>;
  onResolve: (targetArtworkId: number) => void;
}) {
  return (
    <div className="caf-reconciliation-candidates" aria-label="Existing Artwork candidates">
      {candidates.map((candidate) => {
        const thumbUrl = thumbUrls[candidate.artwork_id];
        return (
          <article className="caf-reconciliation-candidate" key={candidate.artwork_id}>
            <div className="caf-reconciliation-thumb" aria-hidden="true">
              {thumbUrl ? <img src={thumbUrl} alt="" /> : <span>No thumbnail</span>}
            </div>
            <div>
              <strong>
                {candidate.display_id} {candidate.title}
              </strong>
              <button
                type="button"
                disabled={isResolving}
                onClick={() => onResolve(candidate.artwork_id)}
              >
                Match this Artwork
              </button>
            </div>
          </article>
        );
      })}
    </div>
  );
}

function ReconciliationActions({
  isResolving,
  onResolveAsNew,
  onSkip,
}: {
  isResolving: boolean;
  onResolveAsNew: () => void;
  onSkip: () => void;
}) {
  return (
    <div className="workspace-command-actions">
      <button type="button" onClick={onResolveAsNew} disabled={isResolving}>
        Import as new
      </button>
      <button type="button" onClick={onSkip} disabled={isResolving}>
        Skip
      </button>
    </div>
  );
}
