import type { RefObject } from "react";
import type {
  CafImportReconciliationItem,
  SniktImportReconciliationItem,
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
