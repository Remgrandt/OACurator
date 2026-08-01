import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, test, vi } from "vitest";
import type { ArtworkSummary, CollectionSummary, GallerySummary } from "../domain/types";
import { GalleryMode } from "./GalleryMode";

const collection: CollectionSummary = {
  id: 1,
  stable_id: "collection-1",
  name: "Studio Archive",
  manifest_path: "C:\\art\\.oacollection",
};

const galleries: GallerySummary[] = [
  {
    id: 10,
    stable_id: "gallery-10",
    name: "Covers",
    manifest_path: "C:\\art\\galleries\\covers\\.oagallery",
    snikt_gallery_inherits_collection: false,
  },
  {
    id: 20,
    stable_id: "gallery-20",
    name: "Splash Pages",
    manifest_path: "C:\\art\\galleries\\splash\\.oagallery",
    snikt_gallery_inherits_collection: false,
  },
];

const artworks: ArtworkSummary[] = [
  {
    id: 100,
    canonical_id: "OAC-00100",
    title: "Moon Knight Cover",
    source_folder: "C:\\art\\artworks\\OAC-00100",
    file_count: 1,
    gallery_ids: [10],
    gallery_names: ["Covers"],
    artist_credits: [{ name: "Bill Sienkiewicz", role: "Artist" }],
  },
  {
    id: 200,
    canonical_id: "OAC-00200",
    title: "City Splash",
    source_folder: "C:\\art\\artworks\\OAC-00200",
    file_count: 1,
    gallery_ids: [20],
    gallery_names: ["Splash Pages"],
    artist_credits: [{ name: "John Doe" }],
  },
];

afterEach(() => cleanup());

describe("GalleryMode", () => {
  test("browses galleries, reports search changes, resizes tiles, and opens the selection", () => {
    const onSelectGallery = vi.fn();
    const onSelectArtwork = vi.fn();
    const onSearchQueryChange = vi.fn();
    const onOpenInWorkbench = vi.fn();

    const { rerender } = render(
      <GalleryMode
        collection={collection}
        galleries={galleries}
        artworks={artworks}
        searchQuery=""
        selectedGalleryId={null}
        selectedArtworkId={100}
        previewUrls={{ "OAC-00100": "data:image/png;base64,preview" }}
        onSelectGallery={onSelectGallery}
        onSelectArtwork={onSelectArtwork}
        onSearchQueryChange={onSearchQueryChange}
        onRequestPreview={vi.fn()}
        onOpenInWorkbench={onOpenInWorkbench}
      />,
    );

    expect(screen.getByRole("heading", { name: "All Artwork · 2" })).toBeInTheDocument();
    expect(screen.queryByRole("searchbox", { name: "Search collection" })).toBeNull();
    expect(screen.getByRole("button", { name: "Browse gallery Covers" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Browse gallery Splash Pages" })).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Browse gallery Covers" }));
    expect(onSelectGallery).toHaveBeenCalledWith(10);

    rerender(
      <GalleryMode
        collection={collection}
        galleries={galleries}
        artworks={artworks}
        searchQuery=""
        selectedGalleryId={10}
        selectedArtworkId={100}
        previewUrls={{ "OAC-00100": "data:image/png;base64,preview" }}
        onSelectGallery={onSelectGallery}
        onSelectArtwork={onSelectArtwork}
        onSearchQueryChange={onSearchQueryChange}
        onRequestPreview={vi.fn()}
        onOpenInWorkbench={onOpenInWorkbench}
      />,
    );

    expect(screen.getByRole("heading", { name: "Covers · 1" })).toBeInTheDocument();
    fireEvent.change(screen.getByRole("searchbox", { name: "Search artwork" }), {
      target: { value: "Bill Sienkiewicz" },
    });
    expect(onSearchQueryChange).toHaveBeenCalledWith("Bill Sienkiewicz");
    const artworkButton = screen.getByRole("button", {
      name: "Open artwork OAC-00100 Moon Knight Cover",
    });
    fireEvent.click(artworkButton);
    expect(onSelectArtwork).toHaveBeenCalledWith(100);

    fireEvent.change(screen.getByRole("slider", { name: "Artwork size" }), {
      target: { value: "300" },
    });
    const galleryRegion = screen.getByRole("region", { name: "Artwork gallery" });
    expect(galleryRegion).toHaveStyle("--gallery-tile-size: 300px");
    expect(galleryRegion.querySelector(".gallery-wall-scroll > .gallery-wall")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Open in Workbench" }));
    expect(onOpenInWorkbench).toHaveBeenCalledWith(100);
  });
});
