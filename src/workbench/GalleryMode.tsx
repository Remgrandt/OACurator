import { useEffect, useRef, useState, type CSSProperties } from "react";
import { Allotment } from "allotment";
import type { ArtworkSummary, CollectionSummary, GallerySummary } from "../domain/types";
import { ToolbarIcon } from "../ui/CommandBar";

type GalleryModeProps = {
  collection: CollectionSummary | null;
  galleries: GallerySummary[];
  artworks: ArtworkSummary[];
  searchQuery: string;
  selectedGalleryId: number | null;
  selectedArtworkId: number | null;
  previewUrls: Record<string, string>;
  onSelectGallery: (galleryId: number | null) => void;
  onSelectArtwork: (artworkId: number) => void;
  onSearchQueryChange: (query: string) => void;
  onRequestPreview: (artwork: ArtworkSummary) => void;
  onOpenInWorkbench: (artworkId: number) => void;
};

export function GalleryMode({
  collection,
  galleries,
  artworks,
  searchQuery,
  selectedGalleryId,
  selectedArtworkId,
  previewUrls,
  onSelectGallery,
  onSelectArtwork,
  onSearchQueryChange,
  onRequestPreview,
  onOpenInWorkbench,
}: GalleryModeProps) {
  const [tileSize, setTileSize] = useState(220);
  const selectedGallery = galleries.find((gallery) => gallery.id === selectedGalleryId) ?? null;
  const visibleArtworks = selectedGallery
    ? artworks.filter((artwork) => artwork.gallery_ids.includes(selectedGallery.id))
    : artworks;
  const focusedArtwork =
    visibleArtworks.find((artwork) => artwork.id === selectedArtworkId) ??
    visibleArtworks[0] ??
    null;
  const focusedImageUrl = focusedArtwork
    ? (previewUrls[focusedArtwork.canonical_id] ?? null)
    : null;
  const galleryStyle = { "--gallery-tile-size": `${tileSize}px` } as CSSProperties;

  return (
    <div className="gallery-mode-frame">
      <Allotment defaultSizes={[240, 760, 360]}>
        <Allotment.Pane minSize={220} maxSize={360}>
          <aside className="gallery-browser" aria-label="Collection Browser">
            <header>
              <span>Collection Browser</span>
            </header>
            {collection ? (
              <nav aria-label={`${collection.name} galleries`}>
                <div className="gallery-browser-collection">
                  <span className="gallery-browser-icon" aria-hidden="true">
                    <ToolbarIcon name="collection-open" />
                  </span>
                  <span>{collection.name}</span>
                </div>
                <button
                  type="button"
                  className={selectedGalleryId === null ? "selected" : ""}
                  aria-pressed={selectedGalleryId === null}
                  onClick={() => onSelectGallery(null)}
                >
                  <span className="gallery-browser-name">
                    <ToolbarIcon name="artwork-open" />
                    All Artwork
                  </span>
                  <strong>{artworks.length}</strong>
                </button>
                {galleries.map((gallery) => (
                  <button
                    type="button"
                    className={gallery.id === selectedGalleryId ? "selected" : ""}
                    aria-label={`Browse gallery ${gallery.name}`}
                    aria-pressed={gallery.id === selectedGalleryId}
                    key={gallery.id}
                    onClick={() => onSelectGallery(gallery.id)}
                  >
                    <span className="gallery-browser-name">
                      <ToolbarIcon name="gallery-open" />
                      {gallery.name}
                    </span>
                    <strong>{artworkCountForGallery(artworks, gallery.id)}</strong>
                  </button>
                ))}
              </nav>
            ) : (
              <div className="gallery-mode-empty">
                <span className="gallery-mode-empty-icon" aria-hidden="true">
                  <ToolbarIcon name="collection-open" />
                </span>
                <p>Open a Collection to browse its Artwork.</p>
              </div>
            )}
          </aside>
        </Allotment.Pane>

        <Allotment.Pane minSize={420}>
          <section className="gallery-wall-panel" aria-label="Artwork gallery" style={galleryStyle}>
            <header className="gallery-wall-toolbar">
              <h2>{`${selectedGallery?.name ?? "All Artwork"} · ${visibleArtworks.length}`}</h2>
              <label className="gallery-size-control">
                <span>Size</span>
                <input
                  type="range"
                  aria-label="Artwork size"
                  min="120"
                  max="360"
                  step="20"
                  value={tileSize}
                  onChange={(event) => setTileSize(Number(event.currentTarget.value))}
                />
              </label>
              <input
                type="search"
                aria-label="Search artwork"
                placeholder="Search artwork"
                value={searchQuery}
                onChange={(event) => onSearchQueryChange(event.currentTarget.value)}
              />
            </header>
            {visibleArtworks.length > 0 ? (
              <div className="gallery-wall-scroll">
                <div className="gallery-wall">
                  {visibleArtworks.map((artwork) => (
                    <GalleryArtworkCard
                      key={artwork.id}
                      artwork={artwork}
                      selected={artwork.id === focusedArtwork?.id}
                      previewUrl={previewUrls[artwork.canonical_id]}
                      onSelect={onSelectArtwork}
                      onRequestPreview={onRequestPreview}
                    />
                  ))}
                </div>
              </div>
            ) : (
              <div className="gallery-mode-empty">
                <p>No Artwork matches this view.</p>
              </div>
            )}
          </section>
        </Allotment.Pane>

        <Allotment.Pane minSize={300} maxSize={520}>
          <aside className="gallery-focus-panel" aria-label="Focused Artwork">
            {focusedArtwork ? (
              <>
                <div className="gallery-focus-image">
                  {focusedImageUrl ? (
                    <img src={focusedImageUrl} alt={`Preview ${focusedArtwork.title}`} />
                  ) : (
                    <span aria-hidden="true">
                      <ToolbarIcon name="artwork-open" />
                    </span>
                  )}
                </div>
                <div className="gallery-focus-details">
                  <p className="gallery-focus-id">
                    {focusedArtwork.display_id || focusedArtwork.canonical_id}
                  </p>
                  <h2>{focusedArtwork.title}</h2>
                  <p>{artistCreditLabel(focusedArtwork)}</p>
                  <dl>
                    {focusedArtwork.media ? (
                      <>
                        <dt>Media</dt>
                        <dd>{focusedArtwork.media}</dd>
                      </>
                    ) : null}
                    <dt>Gallery</dt>
                    <dd>{focusedArtwork.gallery_names.join(", ") || "No Gallery"}</dd>
                    <dt>Files</dt>
                    <dd>{focusedArtwork.file_count}</dd>
                  </dl>
                  <button
                    type="button"
                    className="gallery-open-workbench"
                    onClick={() => onOpenInWorkbench(focusedArtwork.id)}
                  >
                    View artwork details
                  </button>
                </div>
              </>
            ) : (
              <div className="gallery-mode-empty">
                <p>Select Artwork to see it here.</p>
              </div>
            )}
          </aside>
        </Allotment.Pane>
      </Allotment>
    </div>
  );
}

function GalleryArtworkCard({
  artwork,
  selected,
  previewUrl,
  onSelect,
  onRequestPreview,
}: {
  artwork: ArtworkSummary;
  selected: boolean;
  previewUrl: string | undefined;
  onSelect: (artworkId: number) => void;
  onRequestPreview: (artwork: ArtworkSummary) => void;
}) {
  const cardRef = useRef<HTMLElement>(null);

  useEffect(() => {
    if (previewUrl !== undefined) return;
    const card = cardRef.current;
    if (!card || typeof IntersectionObserver === "undefined") {
      onRequestPreview(artwork);
      return;
    }
    const observer = new IntersectionObserver(
      ([entry]) => {
        if (!entry?.isIntersecting) return;
        onRequestPreview(artwork);
        observer.disconnect();
      },
      { rootMargin: "320px" },
    );
    observer.observe(card);
    return () => observer.disconnect();
  }, [artwork, onRequestPreview, previewUrl]);

  const artworkLabel = artwork.display_id || artwork.canonical_id;
  return (
    <article ref={cardRef} className={`gallery-card ${selected ? "selected" : ""}`}>
      <button
        type="button"
        aria-label={`Open artwork ${artworkLabel} ${artwork.title}`}
        aria-pressed={selected}
        onClick={() => onSelect(artwork.id)}
      >
        {previewUrl ? (
          <img src={previewUrl} alt="" loading="lazy" />
        ) : (
          <span className="gallery-card-no-image" aria-hidden="true">
            <ToolbarIcon name="artwork-open" />
          </span>
        )}
      </button>
    </article>
  );
}

function artworkCountForGallery(artworks: ArtworkSummary[], galleryId: number) {
  return artworks.filter((artwork) => artwork.gallery_ids.includes(galleryId)).length;
}

function artistCreditLabel(artwork: ArtworkSummary) {
  return (
    [...new Set(artwork.artist_credits.map((credit) => credit.name).filter(Boolean))].join(", ") ||
    "Unknown artist"
  );
}
