// Copyright (c) 2026 Remgrandt Works. All rights reserved.

import { useCallback, useState } from "react";
import type { ArtworkSummary, GallerySummary, WorkspaceState } from "../../domain/types";

type UseCollectionExplorerOptions = {
  galleries: GallerySummary[];
  artworks: ArtworkSummary[];
};

export function useCollectionExplorer({ galleries, artworks }: UseCollectionExplorerOptions) {
  const [collapsedTreeNodes, setCollapsedTreeNodes] = useState<Set<string>>(() => new Set());
  const [expandedFileTreeNodes, setExpandedFileTreeNodes] = useState<Set<string>>(() => new Set());
  const [collectionSearchQuery, setCollectionSearchQuery] = useState("");

  const isTreeNodeExpanded = useCallback(
    (key: string) => !collapsedTreeNodes.has(key),
    [collapsedTreeNodes],
  );

  const isFilesTreeExpanded = useCallback(
    (artworkId: number, selectedDetailId: number | null | undefined) =>
      selectedDetailId === artworkId && expandedFileTreeNodes.has(treeKeyForFiles(artworkId)),
    [expandedFileTreeNodes],
  );

  const toggleTreeNode = useCallback((key: string) => {
    setCollapsedTreeNodes((current) => {
      const next = new Set(current);
      if (next.has(key)) {
        next.delete(key);
      } else {
        next.add(key);
      }
      return next;
    });
  }, []);

  const collapseAllTreeNodes = useCallback(() => {
    setCollapsedTreeNodes(new Set(allCollapsibleTreeKeys(galleries, artworks)));
    setExpandedFileTreeNodes(new Set());
  }, [artworks, galleries]);

  const expandAllTreeNodes = useCallback(() => {
    setCollapsedTreeNodes(new Set());
    setExpandedFileTreeNodes(new Set());
  }, []);

  const defaultCollapsedTreeKeys = useCallback(
    (nextWorkspace: WorkspaceState) =>
      new Set([
        ...nextWorkspace.galleries.map((gallery) => treeKeyForGallery(gallery.id)),
        ...nextWorkspace.artworks.map((artwork) => treeKeyForArtwork(artwork.id)),
      ]),
    [],
  );

  const expandTreeNodes = useCallback((keys: string[]) => {
    setCollapsedTreeNodes((current) => {
      const next = new Set(current);
      for (const key of keys) {
        next.delete(key);
      }
      return next;
    });
  }, []);

  return {
    collapsedTreeNodes,
    setCollapsedTreeNodes,
    expandedFileTreeNodes,
    setExpandedFileTreeNodes,
    collectionSearchQuery,
    setCollectionSearchQuery,
    isTreeNodeExpanded,
    isFilesTreeExpanded,
    toggleTreeNode,
    collapseAllTreeNodes,
    expandAllTreeNodes,
    defaultCollapsedTreeKeys,
    expandTreeNodes,
  };
}

export function treeKeyForGallery(galleryId: number) {
  return `gallery:${galleryId}`;
}

export function treeKeyForArtwork(artworkId: number) {
  return `artwork:${artworkId}`;
}

export function treeKeyForFiles(recordId: number) {
  return `files:${recordId}`;
}

function allCollapsibleTreeKeys(galleries: GallerySummary[], artworks: ArtworkSummary[]) {
  return [
    "collection",
    ...galleries.map((gallery) => treeKeyForGallery(gallery.id)),
    ...artworks.map((artwork) => treeKeyForArtwork(artwork.id)),
  ];
}
