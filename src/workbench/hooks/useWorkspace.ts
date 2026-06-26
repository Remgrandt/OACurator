import { useMemo, useState } from "react";
import type { ArtworkSummary, GallerySummary, WorkspaceState } from "../../domain/types";

export type InspectorTarget =
  | { type: "collection"; collectionId: number }
  | { type: "gallery"; galleryId: number }
  | { type: "artwork"; artworkId: number }
  | null;

export function useWorkspace() {
  const [workspace, setWorkspace] = useState<WorkspaceState | null>(null);
  const [selectedGalleryId, setSelectedGalleryId] = useState<number | null>(null);
  const [selectedArtworkId, setSelectedArtworkId] = useState<number | null>(null);
  const [inspectorTarget, setInspectorTarget] = useState<InspectorTarget>(null);

  const artworks = useMemo<ArtworkSummary[]>(() => workspace?.artworks ?? [], [workspace]);
  const galleries = useMemo<GallerySummary[]>(() => workspace?.galleries ?? [], [workspace]);

  return {
    workspace,
    setWorkspace,
    artworks,
    galleries,
    selectedGalleryId,
    setSelectedGalleryId,
    selectedArtworkId,
    setSelectedArtworkId,
    inspectorTarget,
    setInspectorTarget,
  };
}
