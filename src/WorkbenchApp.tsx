// Copyright (c) 2026 Remgrandt Works. All rights reserved.

import {
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type FormEvent,
  type KeyboardEvent as ReactKeyboardEvent,
  type MouseEvent,
  type ReactNode,
} from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import {
  confirm as confirmDialog,
  open as openDialog,
  save as saveDialog,
} from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";
import { Allotment } from "allotment";
import {
  ARTIST_ROLE_OPTIONS,
  ART_TYPE_OPTIONS,
  emptyForm,
  MEDIA_TYPE_OPTIONS,
  PUBLICATION_STATUS_OPTIONS,
  SNIKT_ANIMATION_SUBCATEGORY_OPTIONS,
  SNIKT_ART_TYPE_OPTIONS,
} from "./domain/constants";
import {
  blankToNull,
  carouselItemsForDetail,
  ensureTrailingPathSeparator,
  formFromDetail,
  isDirectoryLikeManifestPath,
  parentDirectory,
  requiresWorkspaceCommandName,
  suggestedWorkspaceManifestPath,
  workspaceCommandExtension,
  workspaceCommandSubmitLabel,
  workspaceCommandTitle,
} from "./domain/formatters";
import {
  checkForAppUpdate,
  installAppUpdate,
  type AppUpdateInfo,
  type AppUpdateProgress,
} from "./domain/appUpdates";
import { suggestedExportFileStem } from "./domain/fileNameSuggestions";
import { metadataRequestForForm } from "./domain/metadataRequests";
import { selectRenderableSourceForPngExport } from "./domain/pngExportSources";
import {
  propertyHelpForLabel,
  propertyLabelVisible,
  type PropertySource,
  type PropertySourceFilters,
} from "./domain/propertyDefinitions";
import {
  effectiveSniktArtType,
  sniktExtensionFieldVisible,
  type SniktExtensionFieldLabel,
} from "./domain/sniktFieldVisibility";
import {
  cafImportReportSummary,
  cafImportScreenMessages,
  oaaExportReportSummary,
  oaaImportReportSummary,
  pluralize,
  raremarqCsvExportReportSummary,
  raremarqExportPlanScope,
  sniktImportReportSummary,
} from "./domain/reportSummaries";
import type {
  AppPreferences,
  ArtistCreditForm,
  ArtworkIdLabelPreference,
  ArtworkDetail,
  ArtworkSummary,
  AttachMode,
  CafImportProgress,
  CafImportReconciliationItem,
  CafImportReport,
  CafMissingArtworkReportRow,
  CollectionSummary,
  DeleteFilePreview,
  DeleteArtworkFileResult,
  DeletePreview,
  DeleteResult,
  DeleteTrashFailure,
  DerivedAsset,
  FileRenameExecution,
  FileRenameResult,
  DefaultProviderFocus,
  DragDropEventPayload,
  DetailForm,
  CarouselImageItem,
  ImageDataUrlSource,
  MergeArtworkRequest,
  MergeGalleryRequest,
  OaaImportProgress,
  OaaExportProgress,
  OaaExportReport,
  OaaImportReport,
  PngExportVariant,
  RaremarqCsvExportPlan,
  RaremarqCsvExportProgress,
  RaremarqCsvExportReport,
  RaremarqCsvExportScope,
  RaremarqCsvUrlMode,
  RecentCollection,
  SniktImportProgress,
  SniktImportReport,
  SniktMetadataForm,
  StartupBehaviorPreference,
  ThemePreference,
  ThumbnailCacheProgress,
  WorkspaceCommandMode,
  WorkspaceLoadProgress,
  WorkspaceState,
  GallerySummary,
} from "./domain/types";
import { openUserGuideWindow } from "./help/openUserGuideWindow";
import { finishStartupTrace, markStartupTrace } from "./startupTrace";
import { CommandBar, ToolbarIcon } from "./ui/CommandBar";
import { StatusBar } from "./ui/StatusBar";
import { DeleteConfirmDialog, TrashFailureDialog } from "./workbench/dialogs/DeleteConfirmDialog";
import {
  CafReconciliationDialog,
  SniktReconciliationDialog,
  type CafReconciliationState,
  type SniktReconciliationState,
} from "./workbench/dialogs/ReconciliationDialogs";
import { OaaExportDialog, type OaaExportWizardState } from "./workbench/dialogs/OaaExportDialog";
import {
  RaremarqExportDialog,
  type RaremarqExportWizardState,
} from "./workbench/dialogs/RaremarqExportDialog";
import { WorkspaceCommandDialog } from "./workbench/dialogs/WorkspaceCommandDialog";
import { FileDetailsPanel } from "./workbench/FileDetailsPanel";
import { useArtworkDetail } from "./workbench/hooks/useArtworkDetail";
import {
  treeKeyForArtwork,
  treeKeyForFiles,
  treeKeyForGallery,
  useCollectionExplorer,
} from "./workbench/hooks/useCollectionExplorer";
import { useCollectionTreeViewport } from "./workbench/hooks/useCollectionTreeViewport";
import { useImagePreviews } from "./workbench/hooks/useImagePreviews";
import { useMetadataAutosave } from "./workbench/hooks/useMetadataAutosave";
import { useWorkspace } from "./workbench/hooks/useWorkspace";
import "allotment/dist/style.css";
import "./App.css";

type CurrentWebview = ReturnType<typeof getCurrentWebview>;
type ExplorerContextItem =
  | { type: "collection"; collection: CollectionSummary }
  | { type: "gallery"; gallery: GallerySummary; collectionId: number | null }
  | { type: "artwork"; artwork: ArtworkSummary; galleryId: number }
  | { type: "file"; file: CarouselImageItem; artworkId: number };
type ExplorerContextMenu = {
  x: number;
  y: number;
  item: ExplorerContextItem;
};
type PendingDelete = {
  item: ExplorerContextItem;
  preview: DeletePreview;
  isDeleting: boolean;
};
type PendingRename = {
  item: ExplorerContextItem;
  value: string;
  isSaving: boolean;
};
type GalleryMergeDraft = {
  source: GallerySummary;
  targetId: string;
  name: string;
  cafGalleryRoomId: string;
  raremarqGalleryId: string;
  sniktGalleryInheritsCollection: boolean;
  isMerging: boolean;
};
type ArtworkMergeDraft = {
  source: ArtworkSummary;
  sourceGalleryId: number;
  sourceDetail: ArtworkDetail | null;
  targetId: string;
  targetDetail: ArtworkDetail | null;
  form: DetailForm;
  isLoadingSource: boolean;
  isLoadingTarget: boolean;
  isMerging: boolean;
};
type RenameOutcome = "renamed" | "canceled" | "renamed_reload_workspace";
type DeleteExecutionOutcome = {
  result: DeleteResult;
  detail?: ArtworkDetail;
};
type TrashFailureReport = {
  trashedFiles: DeleteFilePreview[];
  failures: DeleteTrashFailure[];
};
type UpdateDialogState =
  | { state: "checking" }
  | { state: "none" }
  | { state: "available"; update: AppUpdateInfo; progress: AppUpdateProgress | null }
  | { state: "installing"; update: AppUpdateInfo; progress: AppUpdateProgress | null }
  | { state: "error"; message: string };
type SniktUploadPrefillUrlRequest = {
  artwork_id: number;
};
type CafMissingReportState = {
  rows: CafMissingArtworkReportRow[];
  isWriting: boolean;
};
type LoadWorkspaceOptions = {
  resetTree?: boolean;
  searchQuery?: string;
  startupBehavior?: StartupBehaviorPreference;
};
type StartupReadyKey = "frames" | "workspace" | "defaultRoot";
type SniktMetadataBooleanField = "isSundayStrip" | "isNsfw" | "isForSale" | "isOpenToOffers";
type SniktMetadataTextField = Exclude<keyof SniktMetadataForm, SniktMetadataBooleanField>;

const PROPERTY_SOURCE_OPTIONS: { key: PropertySource; label: string }[] = [
  { key: "caf", label: "CAF" },
  { key: "snikt", label: "SNIKT.com" },
  { key: "raremarq", label: "Raremarq" },
];

const DEFAULT_PROPERTY_SOURCE_FILTERS: PropertySourceFilters = {
  caf: true,
  snikt: true,
  raremarq: true,
};

async function loadImageDataUrl(source: ImageDataUrlSource): Promise<string> {
  switch (source.kind) {
    case "cache":
      return invoke<string>("cache_image_data_url_command", { path: source.path });
    case "file_asset":
      return invoke<string>("file_asset_image_data_url_command", {
        fileAssetId: source.fileAssetId,
      });
    case "derived_asset":
      return invoke<string>("derived_asset_image_data_url_command", {
        derivedAssetId: source.derivedAssetId,
      });
  }
}

function cacheImageDataUrl(path: string): Promise<string> {
  return loadImageDataUrl({ kind: "cache", path });
}

const DEFAULT_APP_PREFERENCES: AppPreferences = {
  default_attach_mode: "copy",
  default_png_export_variant: "basic",
  default_provider_focus: "all",
  artwork_id_label_preference: "oac",
  theme: "dracula",
  startup_behavior: "reopen_last",
  default_workspace_root: "",
  raremarq_csv_export_scope: "untracked",
  raremarq_csv_url_mode: "generic_url",
};

const PRIVATE_DATA_PROPERTY_LABELS = [
  "Purchase price",
  "Estimated value",
  "Purchase date",
  "Provenance",
  "Personal notes",
];

const EXPLORER_VIRTUALIZATION_THRESHOLD = 300;
const EXPLORER_TREE_ROW_ESTIMATE_PX = 26;
const EXPLORER_TREE_OVERSCAN_ROWS = 12;

const startupReadiness: Record<StartupReadyKey, boolean> & { finished: boolean } = {
  frames: false,
  workspace: false,
  defaultRoot: false,
  finished: false,
};

function markStartupReady(key: StartupReadyKey, detail?: Record<string, unknown>) {
  if (!startupReadiness[key]) {
    markStartupTrace(`startup_${key}_ready`, "startup", detail);
  }
  startupReadiness[key] = true;
  maybeFinishStartupTrace();
}

function maybeFinishStartupTrace() {
  if (startupReadiness.finished) return;
  if (!startupReadiness.frames || !startupReadiness.workspace || !startupReadiness.defaultRoot)
    return;
  startupReadiness.finished = true;
  scheduleStartupIdleTrace({
    framesReady: startupReadiness.frames,
    workspaceReady: startupReadiness.workspace,
    defaultRootReady: startupReadiness.defaultRoot,
  });
}

function scheduleStartupIdleTrace(detail?: Record<string, unknown>) {
  const windowWithIdleCallback = window as Window & {
    requestIdleCallback?: (callback: IdleRequestCallback, options?: IdleRequestOptions) => number;
  };

  const onIdle = () => {
    markStartupTrace("frontend_idle_ready", "startup", detail);
    finishStartupTrace(detail).catch((error: unknown) => {
      console.error("failed to write OAC startup trace", error);
    });
  };

  if (windowWithIdleCallback.requestIdleCallback) {
    windowWithIdleCallback.requestIdleCallback(onIdle, { timeout: 1000 });
  } else {
    window.setTimeout(onIdle, 250);
  }
}

function WorkbenchApp() {
  markStartupTrace("workbench_render_enter");

  const {
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
  } = useWorkspace();
  const [thumbnailUrls, setThumbnailUrls] = useState<Record<string, string>>({});
  const {
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
  } = useCollectionExplorer({ galleries, artworks });
  const { detail, form, setDetail, setForm, setArtworkDetailFromSnapshot, clearArtworkDetail } =
    useArtworkDetail();
  const {
    detailImageUrls,
    selectedCarouselItemKey,
    setSelectedCarouselItemKey,
    resetImagePreviews,
  } = useImagePreviews({ detail, loadImageDataUrl });
  const { flushMetadataAutosave, markMetadataAutosaveBaseline, markMetadataAutosaveSaved } =
    useMetadataAutosave({
      detail,
      form,
      onSave: (artworkId, detailForm, savedKey) =>
        saveMetadataRequest(artworkId, detailForm, savedKey),
    });
  const {
    ref: collectionTreeRef,
    viewport: collectionTreeViewport,
    handleScroll: handleCollectionTreeScroll,
  } = useCollectionTreeViewport(
    `${artworks.length}:${galleries.length}:${workspace?.collection?.id ?? ""}:${
      workspace?.mode ?? ""
    }`,
  );
  const [theme, setTheme] = useState<"dark" | "light">("dark");
  const [appPreferences, setAppPreferences] = useState<AppPreferences>(DEFAULT_APP_PREFERENCES);
  const [preferencesDraft, setPreferencesDraft] = useState<AppPreferences | null>(null);
  const [preferencesDialogOpen, setPreferencesDialogOpen] = useState(false);
  const [defaultAttachMode, setDefaultAttachMode] = useState<AttachMode>("copy");
  const [propertySourceFilters, setPropertySourceFilters] = useState<PropertySourceFilters>(
    DEFAULT_PROPERTY_SOURCE_FILTERS,
  );
  const [exportDestination, setExportDestination] = useState("");
  const [exportDestinationIsAuto, setExportDestinationIsAuto] = useState(true);
  const [pngExportVariant, setPngExportVariant] = useState<PngExportVariant>("basic");
  const [pngExportRunning, setPngExportRunning] = useState(false);
  const [status, setStatus] = useState("Ready");
  const [error, setError] = useState("");
  const [workspaceCommand, setWorkspaceCommand] = useState<WorkspaceCommandMode | null>(null);
  const [recentCollections, setRecentCollections] = useState<RecentCollection[]>([]);
  const [recentCollectionsLoaded, setRecentCollectionsLoaded] = useState(false);
  const [startupDialogDismissed, setStartupDialogDismissed] = useState(false);
  const [startupOpeningPath, setStartupOpeningPath] = useState<string | null>(null);
  const [cafMissingReport, setCafMissingReport] = useState<CafMissingReportState | null>(null);
  const [cafReconciliation, setCafReconciliation] = useState<CafReconciliationState | null>(null);
  const cafReconciliationDialogRef = useRef<HTMLElement | null>(null);
  const [cafReconciliationThumbUrls, setCafReconciliationThumbUrls] = useState<
    Record<number, string>
  >({});
  const [sniktReconciliation, setSniktReconciliation] = useState<SniktReconciliationState | null>(
    null,
  );
  const sniktReconciliationDialogRef = useRef<HTMLElement | null>(null);
  const [sniktReconciliationThumbUrls, setSniktReconciliationThumbUrls] = useState<
    Record<number, string>
  >({});
  const [oaaExportWizard, setOaaExportWizard] = useState<OaaExportWizardState | null>(null);
  const [raremarqExportWizard, setRaremarqExportWizard] =
    useState<RaremarqExportWizardState | null>(null);
  const [workspaceCommandName, setWorkspaceCommandName] = useState("");
  const [workspaceCommandPath, setWorkspaceCommandPath] = useState("");
  const [workspaceCommandCafId, setWorkspaceCommandCafId] = useState("");
  const [workspaceCommandSniktId, setWorkspaceCommandSniktId] = useState("");
  const [workspaceCommandRaremarqId, setWorkspaceCommandRaremarqId] = useState("");
  const [
    workspaceCommandSniktGalleryInheritsCollection,
    setWorkspaceCommandSniktGalleryInheritsCollection,
  ] = useState(true);
  const [workspaceCommandCsvPath, setWorkspaceCommandCsvPath] = useState("");
  const [workspaceCommandPathIsAuto, setWorkspaceCommandPathIsAuto] = useState(true);
  const [workspaceCommandBasePath, setWorkspaceCommandBasePath] = useState("");
  const [defaultWorkspaceRoot, setDefaultWorkspaceRoot] = useState("");
  const [attachMenuOpen, setAttachMenuOpen] = useState(false);
  const [helpPage, setHelpPage] = useState<"about" | "licensing" | null>(null);
  const [explorerContextMenu, setExplorerContextMenu] = useState<ExplorerContextMenu | null>(null);
  const [pendingDelete, setPendingDelete] = useState<PendingDelete | null>(null);
  const [pendingRename, setPendingRename] = useState<PendingRename | null>(null);
  const [galleryMerge, setGalleryMerge] = useState<GalleryMergeDraft | null>(null);
  const [artworkMerge, setArtworkMerge] = useState<ArtworkMergeDraft | null>(null);
  const [trashFailureReport, setTrashFailureReport] = useState<TrashFailureReport | null>(null);
  const [updateDialog, setUpdateDialog] = useState<UpdateDialogState | null>(null);
  const [workspaceCommandInFlight, setWorkspaceCommandInFlight] =
    useState<WorkspaceCommandMode | null>(null);
  const workspaceCommandInitialFocusRef = useRef<HTMLInputElement | null>(null);
  const renameCommitInFlightRef = useRef(false);
  const selectedArtworkIdRef = useRef<number | null>(null);
  const collectionSearchQueryRef = useRef("");
  const collectionSearchDidMountRef = useRef(false);

  useLayoutEffect(() => {
    markStartupTrace("workbench_layout_effect");
  }, []);

  useEffect(() => {
    markStartupTrace("workbench_effect");
    requestAnimationFrame(() => {
      markStartupTrace("first_animation_frame");
      requestAnimationFrame(() => {
        markStartupTrace("second_animation_frame");
        markStartupReady("frames");
      });
    });
  }, []);

  useEffect(() => {
    const iconPath = theme === "light" ? "/oac-logo-light-mode.svg" : "/oac-logo-dark-mode.svg";
    const favicon =
      document.querySelector<HTMLLinkElement>('link[rel="icon"]') ?? document.createElement("link");
    favicon.id = "app-icon";
    favicon.rel = "icon";
    favicon.type = "image/svg+xml";
    favicon.href = iconPath;
    if (!favicon.parentNode) {
      document.head.appendChild(favicon);
    }

    if (theme === "light") {
      document.documentElement.dataset["theme"] = "alucard";
    } else {
      delete document.documentElement.dataset["theme"];
    }

    return () => {
      delete document.documentElement.dataset["theme"];
    };
  }, [theme]);
  const linkDropModifierPressed = useRef(false);

  useEffect(() => {
    void initializeApp();
  }, []);

  useEffect(() => {
    collectionSearchQueryRef.current = collectionSearchQuery;
    if (!collectionSearchDidMountRef.current) {
      collectionSearchDidMountRef.current = true;
      return;
    }

    const timeout = window.setTimeout(() => {
      void loadWorkspace({ resetTree: true, searchQuery: collectionSearchQuery });
    }, 120);
    return () => window.clearTimeout(timeout);
  }, [collectionSearchQuery]);

  useEffect(() => {
    selectedArtworkIdRef.current = selectedArtworkId;
  }, [selectedArtworkId]);

  useEffect(() => {
    if (!workspaceCommand) return;
    workspaceCommandInitialFocusRef.current?.focus();
  }, [workspaceCommand]);

  useEffect(() => {
    function closeExplorerContextMenu() {
      setExplorerContextMenu(null);
    }

    function closeExplorerContextMenuOnEscape(event: KeyboardEvent) {
      if (event.key === "Escape") {
        setExplorerContextMenu(null);
      }
    }

    window.addEventListener("click", closeExplorerContextMenu);
    window.addEventListener("keydown", closeExplorerContextMenuOnEscape);
    return () => {
      window.removeEventListener("click", closeExplorerContextMenu);
      window.removeEventListener("keydown", closeExplorerContextMenuOnEscape);
    };
  }, []);

  useEffect(() => {
    let cancelled = false;
    let cleanup: (() => void) | undefined;
    void listen<WorkspaceLoadProgress>("workspace-load-progress", (event) => {
      setStatus(event.payload.done ? "Ready" : event.payload.message);
    })
      .then((unlisten) => {
        if (cancelled) {
          unlisten();
        } else {
          cleanup = unlisten;
        }
      })
      .catch(() => {});

    return () => {
      cancelled = true;
      cleanup?.();
    };
  }, []);

  useEffect(() => {
    let cancelled = false;
    let cleanup: (() => void) | undefined;
    void listen<CafImportProgress>("caf-import-progress", (event) => {
      setStatus(event.payload.message);
    })
      .then((unlisten) => {
        if (cancelled) {
          unlisten();
        } else {
          cleanup = unlisten;
        }
      })
      .catch(() => {});

    return () => {
      cancelled = true;
      cleanup?.();
    };
  }, []);

  useEffect(() => {
    let cancelled = false;
    let cleanup: (() => void) | undefined;
    void listen<SniktImportProgress>("snikt-import-progress", (event) => {
      setStatus(event.payload.message);
    })
      .then((unlisten) => {
        if (cancelled) {
          unlisten();
        } else {
          cleanup = unlisten;
        }
      })
      .catch(() => {});

    return () => {
      cancelled = true;
      cleanup?.();
    };
  }, []);

  useEffect(() => {
    let cancelled = false;
    let cleanup: (() => void) | undefined;
    void listen<OaaImportProgress>("oaa-import-progress", (event) => {
      setStatus(event.payload.message);
    })
      .then((unlisten) => {
        if (cancelled) {
          unlisten();
        } else {
          cleanup = unlisten;
        }
      })
      .catch(() => {});

    return () => {
      cancelled = true;
      cleanup?.();
    };
  }, []);

  useEffect(() => {
    let cancelled = false;
    let cleanup: (() => void) | undefined;
    void listen<ThumbnailCacheProgress>("thumbnail-cache-progress", (event) => {
      const progress = event.payload;
      if (progress.done && progress.failed > 0) {
        setStatus(`${progress.message}; ${progress.failed} failed`);
      } else if (progress.done) {
        setStatus("Ready");
      } else {
        setStatus(progress.message);
      }
    })
      .then((unlisten) => {
        if (cancelled) {
          unlisten();
        } else {
          cleanup = unlisten;
        }
      })
      .catch(() => {});

    return () => {
      cancelled = true;
      cleanup?.();
    };
  }, []);

  useEffect(() => {
    let cancelled = false;
    let cleanup: (() => void) | undefined;
    void listen<OaaExportProgress>("oaa-export-progress", (event) => {
      setStatus(event.payload.message);
      setOaaExportWizard((current) =>
        current ? { ...current, progress: event.payload } : current,
      );
    })
      .then((unlisten) => {
        if (cancelled) {
          unlisten();
        } else {
          cleanup = unlisten;
        }
      })
      .catch(() => {});

    return () => {
      cancelled = true;
      cleanup?.();
    };
  }, []);

  useEffect(() => {
    let cancelled = false;
    let cleanup: (() => void) | undefined;
    void listen<RaremarqCsvExportProgress>("raremarq-export-progress", (event) => {
      setStatus(event.payload.message);
      setRaremarqExportWizard((current) =>
        current ? { ...current, progress: event.payload } : current,
      );
    })
      .then((unlisten) => {
        if (cancelled) {
          unlisten();
        } else {
          cleanup = unlisten;
        }
      })
      .catch(() => {});

    return () => {
      cancelled = true;
      cleanup?.();
    };
  }, []);

  useEffect(() => {
    let cancelled = false;
    let cleanup: (() => void) | undefined;
    const currentWebview = getOptionalCurrentWebview();
    if (!currentWebview) {
      return () => {
        cancelled = true;
      };
    }

    void currentWebview
      .onDragDropEvent((event: { payload: DragDropEventPayload }) => {
        if (event.payload.type === "drop") {
          void attachFilePaths(event.payload.paths ?? [], attachModeForDrop(event.payload));
        }
      })
      .then((unlisten) => {
        if (cancelled) {
          unlisten();
        } else {
          cleanup = unlisten;
        }
      })
      .catch(() => {});

    return () => {
      cancelled = true;
      cleanup?.();
    };
  }, [selectedArtworkId]);

  useEffect(() => {
    function handleKeyDown(event: KeyboardEvent) {
      if (event.altKey) {
        linkDropModifierPressed.current = true;
      }
    }

    function handleKeyUp(event: KeyboardEvent) {
      if (event.key === "Alt" || !event.altKey) {
        linkDropModifierPressed.current = false;
      }
    }

    function handleBlur() {
      linkDropModifierPressed.current = false;
    }

    window.addEventListener("keydown", handleKeyDown);
    window.addEventListener("keyup", handleKeyUp);
    window.addEventListener("blur", handleBlur);
    return () => {
      window.removeEventListener("keydown", handleKeyDown);
      window.removeEventListener("keyup", handleKeyUp);
      window.removeEventListener("blur", handleBlur);
    };
  }, []);

  const artworksByGalleryId = useMemo(() => {
    const byGalleryId = new Map<number, ArtworkSummary[]>();
    for (const gallery of galleries) {
      byGalleryId.set(gallery.id, []);
    }
    for (const artwork of artworks) {
      for (const galleryId of artwork.gallery_ids) {
        const galleryArtworks = byGalleryId.get(galleryId);
        if (galleryArtworks) {
          galleryArtworks.push(artwork);
        } else {
          byGalleryId.set(galleryId, [artwork]);
        }
      }
    }
    return byGalleryId;
  }, [artworks, galleries]);
  const selectedGallery = galleries.find((gallery) => gallery.id === selectedGalleryId) ?? null;
  const selectedSummary = artworks.find((artwork) => artwork.id === selectedArtworkId) ?? null;
  const inspectedCollection =
    inspectorTarget?.type === "collection" &&
    workspace?.collection?.id === inspectorTarget.collectionId
      ? workspace.collection
      : null;
  const inspectedGallery =
    inspectorTarget?.type === "gallery"
      ? (galleries.find((gallery) => gallery.id === inspectorTarget.galleryId) ?? null)
      : null;
  const showProperty = (label: string) => propertyLabelVisible(label, propertySourceFilters);
  const showSniktUploadGroup = showProperty("SNIKT extension fields");
  const showPrivateDataGroup = PRIVATE_DATA_PROPERTY_LABELS.some(showProperty);
  const inspectorPanelTitle = inspectedCollection
    ? "Collection Properties"
    : inspectedGallery
      ? "Gallery Properties"
      : detail
        ? "Artwork Properties"
        : "Properties";
  useEffect(() => {
    if (exportDestinationIsAuto && selectedGallery) {
      const defaultDestination = parentDirectory(selectedGallery.manifest_path);
      if (exportDestination !== defaultDestination) {
        setExportDestination(defaultDestination);
      }
    }
  }, [exportDestination, exportDestinationIsAuto, selectedGallery]);
  const carouselItems = useMemo(() => (detail ? carouselItemsForDetail(detail) : []), [detail]);
  const selectedCarouselItem =
    carouselItems.find((item) => item.key === selectedCarouselItemKey) ?? carouselItems[0] ?? null;
  const selectedPreviewPath = selectedCarouselItem?.previewPath ?? null;
  const selectedPreviewUrl = selectedPreviewPath ? detailImageUrls[selectedPreviewPath] : null;
  const selectedPreviewIsLoading = Boolean(
    selectedPreviewPath && !(selectedPreviewPath in detailImageUrls),
  );
  const selectedRenderableSourceForPngExport = selectRenderableSourceForPngExport(
    detail?.file_assets ?? [],
    selectedCarouselItem,
  );
  const summaryPreviewUrl = selectedSummary ? thumbnailUrls[selectedSummary.canonical_id] : null;
  const cacheWarnings = detail?.cache_warnings ?? [];

  function togglePropertySourceFilter(source: PropertySource) {
    setPropertySourceFilters((current) => ({ ...current, [source]: !current[source] }));
  }

  function renderFilteredProperty(label: string, node: ReactNode) {
    return showProperty(label) ? node : null;
  }

  useEffect(() => {
    if (!selectedSummary) return;
    if (thumbnailUrls[selectedSummary.canonical_id] !== undefined) return;

    let disposed = false;
    const canonicalId = selectedSummary.canonical_id;
    const artworkId = selectedSummary.id;
    const existingPath = selectedSummary.thumbnail_path ?? null;

    async function loadThumbnail() {
      const path =
        existingPath ??
        (await invoke<string | null>("ensure_artwork_thumbnail_command", {
          artworkId,
        }));
      if (!path) {
        if (!disposed) {
          setThumbnailUrls((current) =>
            current[canonicalId] === undefined ? { ...current, [canonicalId]: "" } : current,
          );
        }
        return;
      }
      const url = await cacheImageDataUrl(path);
      if (!disposed) {
        setThumbnailUrls((current) =>
          current[canonicalId] === undefined ? { ...current, [canonicalId]: url } : current,
        );
      }
    }

    void loadThumbnail().catch(() => {
      if (!disposed) {
        setThumbnailUrls((current) =>
          current[canonicalId] === undefined ? { ...current, [canonicalId]: "" } : current,
        );
      }
    });

    return () => {
      disposed = true;
    };
  }, [
    selectedSummary?.id,
    selectedSummary?.canonical_id,
    selectedSummary?.thumbnail_path,
    thumbnailUrls,
  ]);

  useEffect(() => {
    if (!artworkMerge) return;
    const targetSummary =
      artworkMerge.targetId.trim() === ""
        ? null
        : (artworks.find((artwork) => String(artwork.id) === artworkMerge.targetId) ?? null);
    const summaries = [artworkMerge.source, targetSummary].filter(
      (summary): summary is ArtworkSummary => Boolean(summary),
    );
    if (summaries.length === 0) return;

    let disposed = false;
    for (const summary of summaries) {
      if (thumbnailUrls[summary.canonical_id] !== undefined) continue;

      async function loadThumbnail() {
        const path =
          summary.thumbnail_path ??
          (await invoke<string | null>("ensure_artwork_thumbnail_command", {
            artworkId: summary.id,
          }));
        if (!path) {
          if (!disposed) {
            setThumbnailUrls((current) =>
              current[summary.canonical_id] === undefined
                ? { ...current, [summary.canonical_id]: "" }
                : current,
            );
          }
          return;
        }
        const url = await cacheImageDataUrl(path);
        if (!disposed) {
          setThumbnailUrls((current) =>
            current[summary.canonical_id] === undefined
              ? { ...current, [summary.canonical_id]: url }
              : current,
          );
        }
      }

      void loadThumbnail().catch(() => {
        if (!disposed) {
          setThumbnailUrls((current) =>
            current[summary.canonical_id] === undefined
              ? { ...current, [summary.canonical_id]: "" }
              : current,
          );
        }
      });
    }

    return () => {
      disposed = true;
    };
  }, [
    artworkMerge?.source.id,
    artworkMerge?.source.canonical_id,
    artworkMerge?.source.thumbnail_path,
    artworkMerge?.targetId,
    artworks,
    thumbnailUrls,
  ]);

  useEffect(() => {
    const item = cafReconciliation?.items[cafReconciliation.index] ?? null;
    if (!item) return;

    let disposed = false;
    item.candidates.forEach((candidate) => {
      if (cafReconciliationThumbUrls[candidate.artwork_id] !== undefined) return;

      async function loadThumbnail() {
        const path =
          candidate.thumbnail_path ??
          (await invoke<string | null>("ensure_artwork_thumbnail_command", {
            artworkId: candidate.artwork_id,
          }));
        if (!path) {
          if (!disposed) {
            setCafReconciliationThumbUrls((current) =>
              current[candidate.artwork_id] === undefined
                ? { ...current, [candidate.artwork_id]: "" }
                : current,
            );
          }
          return;
        }
        const url = await cacheImageDataUrl(path);
        if (!disposed) {
          setCafReconciliationThumbUrls((current) =>
            current[candidate.artwork_id] === undefined
              ? { ...current, [candidate.artwork_id]: url }
              : current,
          );
        }
      }

      void loadThumbnail().catch(() => {
        if (!disposed) {
          setCafReconciliationThumbUrls((current) =>
            current[candidate.artwork_id] === undefined
              ? { ...current, [candidate.artwork_id]: "" }
              : current,
          );
        }
      });
    });

    return () => {
      disposed = true;
    };
  }, [cafReconciliation, cafReconciliationThumbUrls]);

  useEffect(() => {
    const item = sniktReconciliation?.items[sniktReconciliation.index] ?? null;
    if (!item) return;

    let disposed = false;
    item.candidates.forEach((candidate) => {
      if (sniktReconciliationThumbUrls[candidate.artwork_id] !== undefined) return;

      async function loadThumbnail() {
        const path =
          candidate.thumbnail_path ??
          (await invoke<string | null>("ensure_artwork_thumbnail_command", {
            artworkId: candidate.artwork_id,
          }));
        if (!path) {
          if (!disposed) {
            setSniktReconciliationThumbUrls((current) =>
              current[candidate.artwork_id] === undefined
                ? { ...current, [candidate.artwork_id]: "" }
                : current,
            );
          }
          return;
        }
        const url = await cacheImageDataUrl(path);
        if (!disposed) {
          setSniktReconciliationThumbUrls((current) =>
            current[candidate.artwork_id] === undefined
              ? { ...current, [candidate.artwork_id]: url }
              : current,
          );
        }
      }

      void loadThumbnail().catch(() => {
        if (!disposed) {
          setSniktReconciliationThumbUrls((current) =>
            current[candidate.artwork_id] === undefined
              ? { ...current, [candidate.artwork_id]: "" }
              : current,
          );
        }
      });
    });

    return () => {
      disposed = true;
    };
  }, [sniktReconciliation, sniktReconciliationThumbUrls]);

  async function initializeApp() {
    const fallbackRoot = await loadDefaultWorkspaceRoot();
    const loadedPreferences = await loadAppPreferences(fallbackRoot);
    applyAppPreferences(loadedPreferences);
    await loadWorkspace({
      resetTree: true,
      startupBehavior: loadedPreferences.startup_behavior,
    });
    await loadRecentCollections();
  }

  async function loadAppPreferences(fallbackRoot: string) {
    try {
      const preferences = await invoke<AppPreferences>("app_preferences_command");
      return normalizeAppPreferences(preferences, fallbackRoot);
    } catch {
      return normalizeAppPreferences(null, fallbackRoot);
    }
  }

  function applyAppPreferences(preferences: AppPreferences) {
    setAppPreferences(preferences);
    setDefaultAttachMode(preferences.default_attach_mode);
    setPngExportVariant(preferences.default_png_export_variant);
    setPropertySourceFilters(propertySourceFiltersForFocus(preferences.default_provider_focus));
    setTheme(themeStateFromPreference(preferences.theme));
    setDefaultWorkspaceRoot(preferences.default_workspace_root);
  }

  async function persistAppPreferences(
    preferences: AppPreferences,
    options: { refreshWorkspace: boolean; closeDialog?: boolean } = { refreshWorkspace: false },
  ) {
    setError("");
    const nextWorkspace = await invoke<WorkspaceState>("set_app_preferences_command", {
      preferences,
    });
    applyAppPreferences(preferences);
    if (isWorkspaceState(nextWorkspace) && options.refreshWorkspace) {
      setWorkspace(nextWorkspace);
      setSelectedGalleryId(
        nextWorkspace.selected_gallery_id ?? nextWorkspace.galleries[0]?.id ?? null,
      );
      if (detail) {
        const nextDetail = await invoke<ArtworkDetail>("artwork_detail_command", {
          artworkId: detail.id,
        });
        setDetail(nextDetail);
      }
    }
    if (options.closeDialog) {
      setPreferencesDialogOpen(false);
      setPreferencesDraft(null);
    }
  }

  async function savePreferencesDialog() {
    if (!preferencesDraft) return;
    try {
      await persistAppPreferences(preferencesDraft, { refreshWorkspace: true, closeDialog: true });
      setStatus("Preferences saved");
    } catch (caught) {
      setError(errorMessage(caught));
    }
  }

  function openPreferencesDialog() {
    setPreferencesDraft({ ...appPreferences, default_workspace_root: defaultWorkspaceRoot });
    setPreferencesDialogOpen(true);
  }

  function updatePreferencesDraft<K extends keyof AppPreferences>(
    key: K,
    value: AppPreferences[K],
  ) {
    setPreferencesDraft((current) => (current ? { ...current, [key]: value } : current));
  }

  async function pickDefaultWorkspaceRoot() {
    if (!preferencesDraft) return;
    try {
      const selected = await openDialog({
        defaultPath: preferencesDraft.default_workspace_root || defaultWorkspaceRoot,
        directory: true,
        multiple: false,
      });
      if (!selected || Array.isArray(selected)) return;
      updatePreferencesDraft("default_workspace_root", selected);
    } catch (caught) {
      setError(errorMessage(caught));
    }
  }

  function toggleThemePreference() {
    const nextTheme: ThemePreference = theme === "dark" ? "alucard" : "dracula";
    const nextPreferences = { ...appPreferences, theme: nextTheme };
    applyAppPreferences(nextPreferences);
    void persistAppPreferences(nextPreferences).catch((caught) => {
      setError(errorMessage(caught));
    });
  }

  async function loadWorkspace(options: LoadWorkspaceOptions = {}): Promise<WorkspaceState | null> {
    const traceInitialWorkspaceLoad = !startupReadiness.workspace;
    const searchQuery = options.searchQuery ?? collectionSearchQueryRef.current;
    const trimmedSearchQuery = searchQuery.trim();
    const searchArgs = trimmedSearchQuery ? { searchQuery } : undefined;
    if (traceInitialWorkspaceLoad) {
      markStartupTrace("workspace_state_command_begin", "tauri-command", {
        resetTree: Boolean(options.resetTree),
        searchActive: Boolean(searchArgs),
      });
    }
    try {
      setError("");
      const nextWorkspace = searchArgs
        ? await invoke<WorkspaceState>("workspace_state_command", searchArgs)
        : await invoke<WorkspaceState>("workspace_state_command");
      const visibleWorkspace = startupVisibleWorkspace(nextWorkspace, options.startupBehavior);
      if (traceInitialWorkspaceLoad) {
        markStartupTrace("workspace_state_command_end", "tauri-command", {
          mode: visibleWorkspace.mode,
          galleries: visibleWorkspace.galleries.length,
          artworks: visibleWorkspace.artworks.length,
        });
        markStartupReady("workspace", {
          mode: visibleWorkspace.mode,
          galleries: visibleWorkspace.galleries.length,
          artworks: visibleWorkspace.artworks.length,
        });
      }
      if (options.startupBehavior === "start_empty") {
        setStartupDialogDismissed(true);
      }
      setWorkspace(visibleWorkspace);
      setSelectedGalleryId(
        visibleWorkspace.selected_gallery_id ?? visibleWorkspace.galleries[0]?.id ?? null,
      );
      if (options.resetTree) {
        setCollapsedTreeNodes(searchArgs ? new Set() : defaultCollapsedTreeKeys(visibleWorkspace));
        setExpandedFileTreeNodes(new Set());
      }
      return visibleWorkspace;
    } catch (caught) {
      if (isTauriIpcUnavailable(caught)) {
        if (traceInitialWorkspaceLoad) {
          markStartupTrace("workspace_state_command_ipc_unavailable", "tauri-command");
          markStartupReady("workspace", { ipcUnavailable: true });
        }
        setWorkspace(emptyWorkspaceState());
        if (options.resetTree) {
          setCollapsedTreeNodes(new Set());
          setExpandedFileTreeNodes(new Set());
        }
        return emptyWorkspaceState();
      }
      if (traceInitialWorkspaceLoad) {
        markStartupTrace("workspace_state_command_error", "tauri-command", {
          message: errorMessage(caught),
        });
        markStartupReady("workspace", { error: errorMessage(caught) });
      }
      setError(errorMessage(caught));
      return null;
    }
  }

  async function loadRecentCollections() {
    try {
      const recent = await invoke<RecentCollection[]>("recent_collections_command");
      setRecentCollections(Array.isArray(recent) ? recent : []);
    } catch {
      setRecentCollections([]);
    } finally {
      setRecentCollectionsLoaded(true);
    }
  }

  async function openRecentCollection(recent: RecentCollection) {
    try {
      setError("");
      setStartupOpeningPath(recent.path);
      setStatus("Opening Collection");
      await allowUiUpdate();
      await invoke<CollectionSummary>("open_collection_command", {
        request: { path: recent.path },
      });
      setStatus("Collection opened");
      setStartupDialogDismissed(true);
      await loadWorkspace({ resetTree: true });
      await loadRecentCollections();
    } catch (caught) {
      setError(errorMessage(caught));
      setStatus("Ready");
    } finally {
      setStartupOpeningPath(null);
    }
  }

  function beginStartupWorkspaceCommand(command: WorkspaceCommandMode) {
    setStartupDialogDismissed(true);
    void beginWorkspaceCommand(command);
  }

  async function closeCollection() {
    if (!workspace?.collection) return;
    try {
      setError("");
      const nextWorkspace = await invoke<WorkspaceState>("close_collection_command");
      applyWorkspaceReset(nextWorkspace);
      setStatus("Collection closed");
    } catch (caught) {
      setError(errorMessage(caught));
    }
  }

  function clearWorkspaceForOpening() {
    applyWorkspaceReset(emptyWorkspaceState());
  }

  function applyWorkspaceReset(nextWorkspace: WorkspaceState) {
    setWorkspace(nextWorkspace);
    setSelectedGalleryId(
      nextWorkspace.selected_gallery_id ?? nextWorkspace.galleries[0]?.id ?? null,
    );
    setThumbnailUrls({});
    resetImagePreviews();
    setCollapsedTreeNodes(defaultCollapsedTreeKeys(nextWorkspace));
    setExpandedFileTreeNodes(new Set());
    clearSelectedArtwork();
    setInspectorTarget(null);
  }

  async function loadDefaultWorkspaceRoot() {
    const traceInitialDefaultRootLoad = !startupReadiness.defaultRoot;
    if (traceInitialDefaultRootLoad) {
      markStartupTrace("default_oac_root_command_begin", "tauri-command");
    }
    try {
      const root = await invoke<string>("default_oac_root_command");
      if (traceInitialDefaultRootLoad) {
        markStartupTrace("default_oac_root_command_end", "tauri-command", {
          hasRoot: root.length > 0,
        });
        markStartupReady("defaultRoot", { hasRoot: root.length > 0 });
      }
      setDefaultWorkspaceRoot(root);
      return root;
    } catch {
      if (traceInitialDefaultRootLoad) {
        markStartupTrace("default_oac_root_command_error", "tauri-command");
        markStartupReady("defaultRoot", { error: true });
      }
      return "";
    }
  }

  async function ensureDefaultWorkspaceRoot() {
    return defaultWorkspaceRoot || (await loadDefaultWorkspaceRoot());
  }

  async function selectGallery(galleryId: number) {
    try {
      setError("");
      setSelectedGalleryId(galleryId);
      await invoke("select_gallery_command", { galleryId });
      setStatus("Gallery selected");
    } catch (caught) {
      setError(errorMessage(caught));
    }
  }

  async function selectGalleryFromTree(gallery: GallerySummary) {
    expandTreeNodes(["collection", treeKeyForGallery(gallery.id)]);
    clearSelectedArtwork();
    setInspectorTarget({ type: "gallery", galleryId: gallery.id });
    await selectGallery(gallery.id);
  }

  function selectCollectionFromTree(collection: CollectionSummary) {
    expandTreeNodes(["collection"]);
    clearSelectedArtwork();
    setInspectorTarget({ type: "collection", collectionId: collection.id });
    setStatus("Collection selected");
  }

  async function loadArtwork(artworkId: number) {
    try {
      setError("");
      setSelectedArtworkId(artworkId);
      setInspectorTarget({ type: "artwork", artworkId });
      const nextDetail = await invoke<ArtworkDetail>("artwork_detail_command", { artworkId });
      const nextForm = setArtworkDetailFromSnapshot(nextDetail);
      markMetadataAutosaveBaseline(nextDetail.id, nextForm);
      setStatus("Artwork selected");
    } catch (caught) {
      setError(errorMessage(caught));
    }
  }

  function applyArtworkDetailUpdate(
    nextDetail: ArtworkDetail,
    options: { resetSelectedCarouselItem?: boolean } = {},
  ) {
    const nextForm = setArtworkDetailFromSnapshot(nextDetail);
    markMetadataAutosaveBaseline(nextDetail.id, nextForm);
    setWorkspace((current) => updateWorkspaceArtworkSummary(current, nextDetail));
    if (options.resetSelectedCarouselItem) {
      setSelectedCarouselItemKey(null);
    }
  }

  function applyCollectionSummaryUpdate(nextCollection: CollectionSummary) {
    setWorkspace((current) =>
      current?.collection?.id === nextCollection.id
        ? { ...current, collection: nextCollection }
        : current,
    );
  }

  function applyGallerySummaryUpdate(nextGallery: GallerySummary) {
    setWorkspace((current) =>
      current
        ? {
            ...current,
            galleries: current.galleries.map((gallery) =>
              gallery.id === nextGallery.id ? nextGallery : gallery,
            ),
            artworks: current.artworks.map((artwork) =>
              artwork.gallery_ids.includes(nextGallery.id)
                ? {
                    ...artwork,
                    gallery_names: artwork.gallery_ids.map((galleryId, index) =>
                      galleryId === nextGallery.id
                        ? nextGallery.name
                        : (artwork.gallery_names[index] ?? ""),
                    ),
                  }
                : artwork,
            ),
          }
        : current,
    );
  }

  function updateCollectionNameField(collection: CollectionSummary, value: string) {
    applyCollectionSummaryUpdate({ ...collection, name: value });
  }

  async function saveCollectionName(collection: CollectionSummary, value: string) {
    const name = value.trim();
    if (!name) {
      setError("Collection name is required");
      return;
    }
    try {
      setError("");
      const nextCollection = await invoke<CollectionSummary>("rename_collection_command", {
        collectionId: collection.id,
        name,
      });
      applyCollectionSummaryUpdate(nextCollection);
      setStatus("Collection renamed");
      await loadRecentCollections();
    } catch (caught) {
      setError(errorMessage(caught));
    }
  }

  function updateCollectionProviderField(
    collection: CollectionSummary,
    field: "caf_collection_id" | "snikt_collection_id" | "raremarq_collection_id",
    value: string,
  ) {
    applyCollectionSummaryUpdate({ ...collection, [field]: value });
  }

  async function saveCollectionProviderIds(
    collection: CollectionSummary,
    updates: Partial<
      Pick<
        CollectionSummary,
        "caf_collection_id" | "snikt_collection_id" | "raremarq_collection_id"
      >
    >,
  ) {
    try {
      setError("");
      const nextCollection = await invoke<CollectionSummary>(
        "save_collection_provider_ids_command",
        {
          request: {
            collection_id: collection.id,
            caf_collection_id: blankToNull(
              updates.caf_collection_id ?? collection.caf_collection_id ?? "",
            ),
            snikt_collection_id: blankToNull(
              updates.snikt_collection_id ?? collection.snikt_collection_id ?? "",
            ),
            raremarq_collection_id: blankToNull(
              updates.raremarq_collection_id ?? collection.raremarq_collection_id ?? "",
            ),
          },
        },
      );
      applyCollectionSummaryUpdate(nextCollection);
      setStatus("Collection provider IDs saved");
    } catch (caught) {
      setError(errorMessage(caught));
    }
  }

  function updateGalleryNameField(gallery: GallerySummary, value: string) {
    applyGallerySummaryUpdate({ ...gallery, name: value });
  }

  async function saveGalleryName(gallery: GallerySummary, value: string) {
    const name = value.trim();
    if (!name) {
      setError("Gallery name is required");
      return;
    }
    try {
      setError("");
      const nextGallery = await invoke<GallerySummary>("rename_gallery_command", {
        galleryId: gallery.id,
        name,
      });
      applyGallerySummaryUpdate(nextGallery);
      setStatus("Gallery renamed");
    } catch (caught) {
      setError(errorMessage(caught));
    }
  }

  function updateGalleryProviderField(
    gallery: GallerySummary,
    field: "caf_gallery_room_id" | "raremarq_gallery_id" | "snikt_gallery_inherits_collection",
    value: string | boolean,
  ) {
    applyGallerySummaryUpdate({ ...gallery, [field]: value });
  }

  async function saveGalleryProviderIds(
    gallery: GallerySummary,
    updates: Partial<
      Pick<
        GallerySummary,
        "caf_gallery_room_id" | "raremarq_gallery_id" | "snikt_gallery_inherits_collection"
      >
    >,
  ) {
    try {
      setError("");
      const nextGallery = await invoke<GallerySummary>("save_gallery_provider_ids_command", {
        request: {
          gallery_id: gallery.id,
          caf_gallery_room_id: blankToNull(
            updates.caf_gallery_room_id ?? gallery.caf_gallery_room_id ?? "",
          ),
          raremarq_gallery_id: blankToNull(
            updates.raremarq_gallery_id ?? gallery.raremarq_gallery_id ?? "",
          ),
          snikt_gallery_inherits_collection:
            updates.snikt_gallery_inherits_collection ?? gallery.snikt_gallery_inherits_collection,
        },
      });
      applyGallerySummaryUpdate(nextGallery);
      setStatus("Gallery provider IDs saved");
    } catch (caught) {
      setError(errorMessage(caught));
    }
  }

  async function loadArtworkFromTree(artwork: ArtworkSummary, galleryId: number) {
    expandTreeNodes(["collection", treeKeyForGallery(galleryId), treeKeyForArtwork(artwork.id)]);
    setSelectedGalleryId(galleryId);
    try {
      await invoke("select_gallery_command", { galleryId });
    } catch (caught) {
      setError(errorMessage(caught));
    }
    await loadArtwork(artwork.id);
  }

  async function toggleFilesTreeNode(artwork: ArtworkSummary, galleryId: number) {
    const filesKey = treeKeyForFiles(artwork.id);
    if (isFilesTreeExpanded(artwork.id, detail?.id)) {
      setExpandedFileTreeNodes((current) => {
        const next = new Set(current);
        next.delete(filesKey);
        return next;
      });
      return;
    }

    expandTreeNodes(["collection", treeKeyForGallery(galleryId), treeKeyForArtwork(artwork.id)]);
    setSelectedGalleryId(galleryId);
    try {
      await invoke("select_gallery_command", { galleryId });
    } catch (caught) {
      setError(errorMessage(caught));
    }
    setExpandedFileTreeNodes((current) => {
      const next = new Set(current);
      next.add(filesKey);
      return next;
    });
    if (detail?.id !== artwork.id) {
      await loadArtwork(artwork.id);
    }
  }

  async function createArtworkForGallery(galleryId: number) {
    try {
      setError("");
      setSelectedGalleryId(galleryId);
      await invoke("select_gallery_command", { galleryId });
      const artwork = await invoke<ArtworkSummary>("create_artwork_command", {
        request: { gallery_id: galleryId, title: "Untitled Artwork" },
      });
      await loadWorkspace({ resetTree: true });
      expandTreeNodes(["collection", treeKeyForGallery(galleryId), treeKeyForArtwork(artwork.id)]);
      await loadArtwork(artwork.id);
      setStatus("Artwork created");
    } catch (caught) {
      setError(errorMessage(caught));
    }
  }

  async function createArtworkInActiveGallery() {
    if (!workspace?.collection) {
      setError("Open a Collection before creating Artwork.");
      return;
    }
    const gallery = selectedGallery ?? galleries[0] ?? null;
    if (!gallery) {
      setError("Create a Gallery before adding Artwork.");
      return;
    }
    await createArtworkForGallery(gallery.id);
  }

  function attachModeForDrop(payload: DragDropEventPayload): AttachMode {
    const modifierPressed =
      payload.modifiers?.altKey || payload.altKey || linkDropModifierPressed.current;
    if (!modifierPressed) return defaultAttachMode;
    return defaultAttachMode === "copy" ? "link" : "copy";
  }

  async function attachFilePaths(paths: string[], mode: AttachMode) {
    if (!selectedArtworkId || paths.length === 0) return;
    await attachFilePathsToArtwork(selectedArtworkId, paths, mode);
  }

  async function attachFilePathsToArtwork(artworkId: number, paths: string[], mode: AttachMode) {
    if (paths.length === 0) return;
    try {
      setError("");
      setStatus(mode === "copy" ? "Copying files" : "Linking files");
      setSelectedArtworkId(artworkId);
      const nextDetail = await invoke<ArtworkDetail>("attach_file_assets_command", {
        request: { artwork_id: artworkId, paths, mode },
      });
      applyArtworkDetailUpdate(nextDetail);
      const verb = mode === "copy" ? "copied" : "linked";
      setStatus(paths.length === 1 ? `File ${verb}` : `${paths.length} files ${verb}`);
    } catch (caught) {
      setError(errorMessage(caught));
      setStatus("Ready");
    }
  }

  function fileAssetIdsAfterCarouselMove(sourceKey: string, targetKey: string): number[] | null {
    if (!detail || sourceKey === targetKey) return null;
    const source = carouselItems.find((item) => item.key === sourceKey);
    const target = carouselItems.find((item) => item.key === targetKey);
    if (!source || !target || source.kind !== "file" || target.kind !== "file") return null;

    const orderedIds = detail.file_assets.map((asset) => asset.id);
    const sourceIndex = orderedIds.indexOf(source.id);
    const targetIndex = orderedIds.indexOf(target.id);
    if (sourceIndex < 0 || targetIndex < 0 || sourceIndex === targetIndex) return null;

    orderedIds.splice(sourceIndex, 1);
    orderedIds.splice(targetIndex, 0, source.id);
    return orderedIds;
  }

  async function reorderCarouselFileAsset(sourceKey: string, targetKey: string) {
    if (!detail) return;
    const fileAssetIds = fileAssetIdsAfterCarouselMove(sourceKey, targetKey);
    if (!fileAssetIds) return;

    try {
      setError("");
      setStatus("Saving image order");
      const nextDetail = await invoke<ArtworkDetail>("reorder_file_assets_command", {
        request: {
          artwork_id: detail.id,
          file_asset_ids: fileAssetIds,
        },
      });
      applyArtworkDetailUpdate(nextDetail);
      setStatus("Image order saved");
    } catch (caught) {
      setError(errorMessage(caught));
      setStatus("Ready");
    }
  }

  function adjacentCarouselFileKey(sourceKey: string, direction: "left" | "right") {
    const fileItems = carouselItems.filter((item) => item.kind === "file");
    const sourceIndex = fileItems.findIndex((item) => item.key === sourceKey);
    if (sourceIndex < 0) return null;

    const targetIndex = sourceIndex + (direction === "left" ? -1 : 1);
    return fileItems[targetIndex]?.key ?? null;
  }

  async function moveCarouselFileAsset(sourceKey: string, direction: "left" | "right") {
    const targetKey = adjacentCarouselFileKey(sourceKey, direction);
    if (!targetKey) return;
    await reorderCarouselFileAsset(sourceKey, targetKey);
  }

  async function pickFilesForSelectedArtwork(mode: AttachMode) {
    if (!selectedArtworkId) return;
    await pickFilesForArtwork(selectedArtworkId, mode);
  }

  async function pickFilesForArtwork(artworkId: number, mode: AttachMode) {
    try {
      setError("");
      const selected = await openDialog({
        ...(selectedGallery ? { defaultPath: parentDirectory(selectedGallery.manifest_path) } : {}),
        directory: false,
        multiple: true,
      });
      if (!selected) return;
      await attachFilePathsToArtwork(
        artworkId,
        Array.isArray(selected) ? selected : [selected],
        mode,
      );
    } catch (caught) {
      setError(errorMessage(caught));
    }
  }

  async function beginWorkspaceCommand(command: WorkspaceCommandMode) {
    setError("");
    if (command === "open_collection") {
      await pickWorkspaceManifest(command);
      return;
    }
    if (command === "new_gallery" && !workspace?.collection) {
      setError("Open a Collection before creating a Gallery.");
      return;
    }

    const root = await suggestedWorkspaceCommandBasePath(command);
    const existingProviderId = workspaceCommandProviderId(command);
    const importsIntoOpenCollection = workspaceCommandImportsIntoOpenCollection(command);
    setWorkspaceCommand(command);
    setWorkspaceCommandName("");
    setWorkspaceCommandCafId(existingProviderId ?? "");
    setWorkspaceCommandSniktId("");
    setWorkspaceCommandRaremarqId("");
    setWorkspaceCommandSniktGalleryInheritsCollection(true);
    setWorkspaceCommandCsvPath("");
    setWorkspaceCommandBasePath(root);
    setWorkspaceCommandPath(importsIntoOpenCollection ? "" : root);
    setWorkspaceCommandPathIsAuto(true);
  }

  async function suggestedWorkspaceCommandBasePath(command: WorkspaceCommandMode) {
    if (command === "new_gallery" && workspace?.collection?.manifest_path) {
      const collectionRoot = ensureTrailingPathSeparator(
        parentDirectory(workspace.collection.manifest_path),
      );
      const separator = collectionRoot.endsWith("/") ? "/" : "\\";
      return `${collectionRoot}galleries${separator}`;
    }
    const root = await ensureDefaultWorkspaceRoot();
    return root ? ensureTrailingPathSeparator(root) : "";
  }

  async function pickWorkspaceManifest(command: Extract<WorkspaceCommandMode, "open_collection">) {
    try {
      const root = await ensureDefaultWorkspaceRoot();
      const extension = workspaceCommandExtension(command);
      const selected = await openDialog({
        ...(root ? { defaultPath: root } : {}),
        directory: false,
        multiple: false,
        filters: [
          {
            name: "OA Collection",
            extensions: [extension],
          },
        ],
      });
      if (!selected || Array.isArray(selected)) return;

      if (workspace?.collection) {
        setStatus("Closing active Collection");
        await invoke<WorkspaceState>("close_collection_command");
        clearWorkspaceForOpening();
        await allowUiUpdate();
      }
      setStatus("Opening Collection");
      await allowUiUpdate();
      await invoke<CollectionSummary>("open_collection_command", {
        request: { path: selected },
      });
      setStatus("Collection opened");
      await loadWorkspace({ resetTree: true });
      await loadRecentCollections();
    } catch (caught) {
      setError(errorMessage(caught));
    }
  }

  async function pickCafCsvPath() {
    try {
      const root = await ensureDefaultWorkspaceRoot();
      const selected = await openDialog({
        ...(root ? { defaultPath: root } : {}),
        directory: false,
        multiple: false,
        filters: [
          {
            name: "CAF CSV",
            extensions: ["csv"],
          },
        ],
      });
      if (!selected || Array.isArray(selected)) return;
      setWorkspaceCommandCsvPath(selected);
    } catch (caught) {
      setError(errorMessage(caught));
    }
  }

  async function pickOaaArchivePath() {
    try {
      const root = await ensureDefaultWorkspaceRoot();
      const selected = await openDialog({
        ...(root ? { defaultPath: root } : {}),
        directory: false,
        multiple: false,
        filters: [
          {
            name: "OAA Archive",
            extensions: ["oaa"],
          },
        ],
      });
      if (!selected || Array.isArray(selected)) return;
      setWorkspaceCommandCsvPath(selected);
    } catch (caught) {
      setError(errorMessage(caught));
    }
  }

  function updateWorkspaceCommandName(event: React.ChangeEvent<HTMLInputElement>) {
    const value = event.currentTarget.value;
    setWorkspaceCommandName(value);
    if (!workspaceCommand || !requiresWorkspaceCommandName(workspaceCommand)) return;

    if (
      workspaceCommandPathIsAuto ||
      isDirectoryLikeManifestPath(workspaceCommandPath, workspaceCommand)
    ) {
      setWorkspaceCommandPathIsAuto(true);
      setWorkspaceCommandPath(
        suggestedWorkspaceManifestPath(workspaceCommand, value, workspaceCommandBasePath),
      );
    }
  }

  function updateWorkspaceCommandPath(event: React.ChangeEvent<HTMLInputElement>) {
    const value = event.currentTarget.value;
    setWorkspaceCommandPath(value);
    if (!workspaceCommand || !requiresWorkspaceCommandName(workspaceCommand)) {
      setWorkspaceCommandPathIsAuto(false);
      return;
    }
    const isAuto = value.trim() === "" || isDirectoryLikeManifestPath(value, workspaceCommand);
    if (isAuto) {
      setWorkspaceCommandBasePath(ensureTrailingPathSeparator(value));
    }
    setWorkspaceCommandPathIsAuto(isAuto);
  }

  function selectSuggestedWorkspaceCommandPath(event: React.FocusEvent<HTMLInputElement>) {
    if (workspaceCommandPathIsAuto) {
      event.currentTarget.select();
    }
  }

  function isImportWorkspaceCommand(
    command: WorkspaceCommandMode | null,
  ): command is "import_caf_collection" | "import_oaa_archive" | "import_snikt_collection" {
    return (
      command === "import_caf_collection" ||
      command === "import_oaa_archive" ||
      command === "import_snikt_collection"
    );
  }

  function workspaceCommandProviderId(command: WorkspaceCommandMode | null) {
    if (command === "import_caf_collection")
      return workspace?.collection?.caf_collection_id ?? null;
    if (command === "import_snikt_collection")
      return workspace?.collection?.snikt_collection_id ?? null;
    return null;
  }

  function workspaceCommandImportsIntoOpenCollection(command: WorkspaceCommandMode | null) {
    return isImportWorkspaceCommand(command) && Boolean(workspace?.collection);
  }

  function isSourceFileWorkspaceCommand(
    command: WorkspaceCommandMode | null,
  ): command is "import_caf_collection" | "import_oaa_archive" | "import_snikt_collection" {
    return (
      command === "import_caf_collection" ||
      command === "import_oaa_archive" ||
      command === "import_snikt_collection"
    );
  }

  function workspaceCommandNeedsPath(command: WorkspaceCommandMode | null) {
    if (!command) return false;
    if (isImportWorkspaceCommand(command))
      return !workspaceCommandImportsIntoOpenCollection(command);
    return command !== "new_gallery";
  }

  function workspaceCommandDisplayLabel(command: WorkspaceCommandMode) {
    if (command === "import_caf_collection") return "Import CAF Collection";
    if (command === "import_oaa_archive") return "Import OAA Archive";
    if (command === "import_snikt_collection") return "Import SNIKT.com Collection";
    return workspaceCommandTitle(command);
  }

  function workspaceCommandSubmitDisplayLabel(command: WorkspaceCommandMode) {
    return isImportWorkspaceCommand(command)
      ? workspaceCommandDisplayLabel(command)
      : workspaceCommandSubmitLabel(command);
  }

  function workspaceCommandImportMessage(command: WorkspaceCommandMode) {
    if (!workspaceCommandImportsIntoOpenCollection(command)) return "";
    const collectionName = workspace?.collection?.name ?? "the open Collection";
    const providerLabel =
      command === "import_caf_collection"
        ? "CAF CSV"
        : command === "import_oaa_archive"
          ? "OAA"
          : command === "import_snikt_collection"
            ? "SNIKT.com CSV"
            : "Raremarq CSV";
    return `This will merge ${providerLabel} data into the open Collection "${collectionName}". Close the Collection first if you want to import into a new Collection instead.`;
  }

  async function submitWorkspaceCommand() {
    if (!workspaceCommand) return;
    const path = workspaceCommandPath.trim();
    const name = workspaceCommandName.trim();
    const needsName = workspaceCommand === "new_collection" || workspaceCommand === "new_gallery";
    const needsPath = workspaceCommandNeedsPath(workspaceCommand);

    if ((needsPath && !path) || (needsName && !name)) return;

    try {
      setError("");
      setWorkspaceCommandInFlight(workspaceCommand);
      let postCommandMessages: string[] = [];
      if (workspaceCommand === "new_collection") {
        await invoke<CollectionSummary>("create_collection_command", {
          request: {
            name,
            path,
            caf_collection_id: blankToNull(workspaceCommandCafId),
            snikt_collection_id: blankToNull(workspaceCommandSniktId),
            raremarq_collection_id: blankToNull(workspaceCommandRaremarqId),
          },
        });
        setStatus("Collection created");
      } else if (workspaceCommand === "import_caf_collection") {
        setStatus("Preparing CAF CSV import");
        setCafReconciliation(null);
        setCafMissingReport(null);
        const request = workspace?.collection
          ? {
              csv_path: workspaceCommandCsvPath.trim(),
              target_collection_id: workspace.collection.id,
            }
          : { csv_path: workspaceCommandCsvPath.trim(), destination_root: path };
        const importPromise = importCafCsvWithMismatchPrompt(request);
        setWorkspaceCommand(null);
        const report = await importPromise;
        setStatus(cafImportReportSummary(report));
        const unresolvedReconciliationItems = await autoResolveCafReconciliationItems(
          report.reconciliation_items,
        );
        if (unresolvedReconciliationItems.length > 0) {
          setCafReconciliation({
            items: unresolvedReconciliationItems,
            index: 0,
            isResolving: false,
          });
        } else {
          setCafReconciliation(null);
        }
        if (report.missing_artworks.length > 0) {
          setCafMissingReport({ rows: report.missing_artworks, isWriting: false });
        }
        if (report.messages.length > 0) {
          postCommandMessages = cafImportScreenMessages(report);
        }
      } else if (workspaceCommand === "import_oaa_archive") {
        setStatus("Preparing OAA archive import");
        const request = workspace?.collection
          ? {
              archive_path: workspaceCommandCsvPath.trim(),
              target_collection_id: workspace.collection.id,
            }
          : {
              archive_path: workspaceCommandCsvPath.trim(),
              destination_root: path,
            };
        const importPromise = invoke<OaaImportReport>("import_oaa_archive_command", {
          request,
        });
        setWorkspaceCommand(null);
        const report = await importPromise;
        setStatus(oaaImportReportSummary(report));
        if (report.messages.length > 0) {
          postCommandMessages = report.messages;
        }
      } else if (workspaceCommand === "import_snikt_collection") {
        setStatus("Preparing SNIKT.com CSV import");
        const request = workspace?.collection
          ? {
              csv_path: workspaceCommandCsvPath.trim(),
              target_collection_id: workspace.collection.id,
            }
          : { csv_path: workspaceCommandCsvPath.trim(), destination_root: path };
        const importPromise = invoke<SniktImportReport>("import_snikt_collection_command", {
          request,
        });
        setWorkspaceCommand(null);
        const report = await importPromise;
        setStatus(sniktImportReportSummary(report));
        if (report.reconciliation_items.length > 0) {
          setSniktReconciliation({
            items: report.reconciliation_items,
            index: 0,
            isResolving: false,
          });
        }
        if (report.messages.length > 0) {
          postCommandMessages = report.messages;
        }
      } else if (workspaceCommand === "open_collection") {
        setStatus("Opening Collection");
        await allowUiUpdate();
        await invoke<CollectionSummary>("open_collection_command", {
          request: { path },
        });
        setStatus("Collection opened");
      } else if (workspaceCommand === "new_gallery") {
        if (!workspace?.collection) {
          setError("Open a Collection before creating a Gallery.");
          return;
        }
        const galleryPath = suggestedWorkspaceManifestPath(
          workspaceCommand,
          name,
          workspaceCommandBasePath,
        );
        await invoke<GallerySummary>("create_gallery_command", {
          request: {
            name,
            path: galleryPath,
            caf_gallery_room_id: blankToNull(workspaceCommandCafId),
            raremarq_gallery_id: blankToNull(workspaceCommandRaremarqId),
            snikt_gallery_inherits_collection: workspaceCommandSniktGalleryInheritsCollection,
            collection_id: workspace.collection.id,
          },
        });
        setStatus("Gallery created");
      }

      setWorkspaceCommand(null);
      setWorkspaceCommandName("");
      setWorkspaceCommandPath("");
      setWorkspaceCommandCafId("");
      setWorkspaceCommandSniktId("");
      setWorkspaceCommandRaremarqId("");
      setWorkspaceCommandSniktGalleryInheritsCollection(true);
      setWorkspaceCommandCsvPath("");
      setWorkspaceCommandBasePath("");
      const nextWorkspace = await loadWorkspace();
      if (workspaceCommand !== "new_gallery") {
        await loadRecentCollections();
      }
      if (isImportWorkspaceCommand(workspaceCommand)) {
        const currentSelectedArtworkId = selectedArtworkIdRef.current;
        if (
          currentSelectedArtworkId &&
          nextWorkspace?.artworks.some((artwork) => artwork.id === currentSelectedArtworkId)
        ) {
          await loadArtwork(currentSelectedArtworkId);
        }
      }
      if (postCommandMessages.length > 0) {
        setError(postCommandMessages.join("\n"));
      }
    } catch (caught) {
      setError(errorMessage(caught));
    } finally {
      setWorkspaceCommandInFlight(null);
    }
  }

  async function importCafCsvWithMismatchPrompt(request: {
    csv_path: string;
    destination_root?: string;
    target_collection_id?: number;
  }) {
    try {
      return await invoke<CafImportReport>("import_caf_csv_command", { request });
    } catch (caught) {
      const message = errorMessage(caught);
      if (request.target_collection_id && isCafCollectionIdMismatchError(message)) {
        const shouldOverride = await confirmDialog(
          `${message}\n\nImporting this CSV will replace the Collection's tracked CAF Collection ID. Continue?`,
          {
            title: "Replace tracked CAF Collection ID?",
            kind: "warning",
          },
        );
        if (shouldOverride) {
          setStatus("Importing CAF CSV with approved Collection ID override");
          return await invoke<CafImportReport>("import_caf_csv_command", {
            request: {
              ...request,
              allow_caf_collection_id_override: true,
            },
          });
        }
      }
      throw caught;
    }
  }

  async function autoResolveCafReconciliationItems(items: CafImportReconciliationItem[]) {
    if (items.length === 0) return items;
    const unresolved: CafImportReconciliationItem[] = [];
    let resolvedCount = 0;
    for (const item of items) {
      const summary = await invoke<ArtworkSummary | null>(
        "try_auto_resolve_caf_reconciliation_command",
        {
          request: { item },
        },
      );
      if (summary) {
        resolvedCount += 1;
      } else {
        unresolved.push(item);
      }
    }
    if (resolvedCount > 0) {
      setStatus(
        unresolved.length > 0
          ? `Auto-resolved ${resolvedCount} CAF CSV ${resolvedCount === 1 ? "row" : "rows"}`
          : `Auto-resolved all CAF CSV matches`,
      );
    }
    return unresolved;
  }

  async function saveMetadataRequest(artworkId: number, detailForm: DetailForm, savedKey: string) {
    try {
      setError("");
      setStatus("Saving metadata");
      const nextDetail = await invoke<ArtworkDetail>("save_metadata_command", {
        request: metadataRequestForForm(artworkId, detailForm),
      });
      setDetail(nextDetail);
      setWorkspace((current) => updateWorkspaceArtworkSummary(current, nextDetail));
      markMetadataAutosaveSaved(savedKey);
      setStatus("Metadata saved");
      return true;
    } catch (caught) {
      setError(errorMessage(caught));
      setStatus("Metadata save failed");
      return false;
    }
  }

  async function saveSelectedImageRole(event: React.ChangeEvent<HTMLSelectElement>) {
    if (!selectedCarouselItem) return;
    try {
      setError("");
      const nextDetail = await invoke<ArtworkDetail>("save_image_metadata_command", {
        request: {
          asset_kind: selectedCarouselItem.kind,
          asset_id: selectedCarouselItem.id,
          image_role: blankToNull(event.currentTarget.value),
        },
      });
      applyArtworkDetailUpdate(nextDetail);
      setStatus("File metadata saved");
    } catch (caught) {
      setError(errorMessage(caught));
    }
  }

  async function showSelectedImageInExplorer() {
    if (!selectedCarouselItem) return;
    try {
      setError("");
      await invoke("show_path_in_file_manager_command", {
        path: selectedCarouselItem.path,
      });
      setStatus("Opened file location");
    } catch (caught) {
      setError(errorMessage(caught));
    }
  }

  async function copySelectedFilePath() {
    if (!selectedCarouselItem) return;
    if (!navigator.clipboard?.writeText) {
      setError("Clipboard is not available.");
      return;
    }
    try {
      setError("");
      await navigator.clipboard.writeText(selectedCarouselItem.path);
      setStatus("File path copied");
    } catch (caught) {
      setError(errorMessage(caught));
    }
  }

  async function openArtworkUrl(label: string, url: string) {
    const trimmedUrl = url.trim();
    if (!trimmedUrl) return;
    try {
      setError("");
      await openUrl(trimmedUrl);
      setStatus(`${label} opened`);
    } catch (caught) {
      setError(errorMessage(caught));
      setStatus("Could not open URL");
    }
  }

  async function openSniktUploadPrefill() {
    if (!detail) return;
    const request: SniktUploadPrefillUrlRequest = {
      artwork_id: detail.id,
    };
    try {
      setError("");
      const url = await invoke<string>("snikt_upload_prefill_url_command", { request });
      await openUrl(url);
      setStatus("SNIKT upload prefill opened");
    } catch (caught) {
      setError(errorMessage(caught));
      setStatus("Could not open SNIKT upload prefill");
    }
  }

  async function checkForUpdates() {
    setUpdateDialog({ state: "checking" });
    setStatus("Checking for updates");
    try {
      const update = await checkForAppUpdate();
      if (!update) {
        setUpdateDialog({ state: "none" });
        setStatus("OA Curator is up to date");
        return;
      }
      setUpdateDialog({ state: "available", update, progress: null });
      setStatus(`Update ${update.version} available`);
    } catch (caught) {
      const message = errorMessage(caught);
      setUpdateDialog({ state: "error", message });
      setError(message);
      setStatus("Update check failed");
    }
  }

  function updateInstallBlockedReason() {
    if (workspaceCommandInFlight) {
      return isImportWorkspaceCommand(workspaceCommandInFlight)
        ? "Finish the current import before installing an update."
        : "Finish the current workspace operation before installing an update.";
    }
    if (pngExportRunning) {
      return "Finish the PNG export before installing an update.";
    }
    if (oaaExportWizard?.isRunning || raremarqExportWizard?.isRunning) {
      return "Finish the current export before installing an update.";
    }
    if (cafMissingReport?.isWriting) {
      return "Finish writing the report before installing an update.";
    }
    if (pendingDelete?.isDeleting) {
      return "Finish the delete operation before installing an update.";
    }
    if (pendingRename?.isSaving) {
      return "Finish the rename operation before installing an update.";
    }
    if (galleryMerge?.isMerging || artworkMerge?.isMerging) {
      return "Finish the merge operation before installing an update.";
    }
    return null;
  }

  async function installSelectedUpdate(update: AppUpdateInfo) {
    const blockedReason = updateInstallBlockedReason();
    if (blockedReason) {
      setStatus("Update install blocked");
      setUpdateDialog({ state: "error", message: blockedReason });
      return;
    }

    const confirmed = await confirmDialog(
      "OA Curator will close to finish installing this update. Continue?",
      {
        title: "Install Update",
        kind: "warning",
      },
    );
    if (!confirmed) {
      setStatus("Update install canceled");
      return;
    }

    const flushResult = await flushMetadataAutosave();
    if (flushResult === "failed") {
      const message = "Metadata save failed; update install was not started.";
      setStatus("Update install blocked");
      setUpdateDialog({ state: "error", message });
      return;
    }

    setUpdateDialog({ state: "installing", update, progress: null });
    setStatus(`Installing update ${update.version}`);
    try {
      await installAppUpdate(update, (progress) => {
        setUpdateDialog({ state: "installing", update, progress });
      });
    } catch (caught) {
      const message = errorMessage(caught);
      setUpdateDialog({ state: "error", message });
      setError(message);
      setStatus("Update install failed");
    }
  }

  async function createPngExport() {
    if (!detail || !selectedRenderableSourceForPngExport) return;
    if (!exportDestination.trim()) {
      setError("Export destination is required.");
      return;
    }
    try {
      setError("");
      setStatus("Creating PNG export");
      setPngExportRunning(true);
      const derivative = await invoke<DerivedAsset>("create_png_derivative_command", {
        artworkId: detail.id,
        sourceFileAssetId: selectedRenderableSourceForPngExport.id,
        exportRoot: exportDestination,
        variant: pngExportVariant,
      });
      setDetail({
        ...detail,
        derived_assets: [...detail.derived_assets, derivative],
      });
      setSelectedCarouselItemKey(`derived:${derivative.id}`);
      setStatus(`PNG export created: ${derivative.path}`);
    } catch (caught) {
      setError(errorMessage(caught));
      setStatus("PNG export failed");
    } finally {
      setPngExportRunning(false);
    }
  }

  async function exportOpenCollectionAsOaa() {
    if (!workspace?.collection) {
      setError("Open a Collection before exporting an OAA archive.");
      return;
    }
    try {
      setError("");
      const root = await ensureDefaultWorkspaceRoot();
      const defaultName = `${suggestedExportFileStem(workspace.collection.name) || "collection"}.oaa`;
      setOaaExportWizard({
        archivePath: root ? `${ensureTrailingPathSeparator(root)}${defaultName}` : defaultName,
        includeImages: true,
        includePrivateMetadata: true,
        isRunning: false,
        progress: null,
        report: null,
      });
      setStatus("Preparing OAA archive export");
    } catch (caught) {
      setError(errorMessage(caught));
      setStatus("OAA export failed");
    }
  }

  async function pickOaaExportArchivePath() {
    if (!oaaExportWizard || oaaExportWizard.isRunning) return;
    try {
      setError("");
      const selected = await saveDialog({
        defaultPath: oaaExportWizard.archivePath,
        filters: [
          {
            name: "OAA Archive",
            extensions: ["oaa"],
          },
        ],
      });
      if (!selected || Array.isArray(selected)) return;
      setOaaExportWizard((current) =>
        current ? { ...current, archivePath: selected, report: null } : current,
      );
    } catch (caught) {
      setError(errorMessage(caught));
    }
  }

  async function submitOaaExportWizard() {
    if (!workspace?.collection || !oaaExportWizard || oaaExportWizard.isRunning) return;
    const archivePath = oaaExportWizard.archivePath.trim();
    if (!archivePath) {
      setError("OAA archive path is required.");
      return;
    }
    try {
      setError("");
      setOaaExportWizard((current) =>
        current ? { ...current, progress: null, report: null } : current,
      );
      const destinationExists = await invoke<boolean>("destination_file_exists_command", {
        path: archivePath,
      });
      if (destinationExists) {
        const shouldReplace = await confirmDialog(
          `An OAA archive already exists at this path:\n\n${archivePath}\n\nReplace it?`,
          {
            title: "Replace OAA archive?",
            kind: "warning",
          },
        );
        if (!shouldReplace) {
          setStatus("OAA export canceled");
          return;
        }
      }
      setStatus("Exporting OAA archive");
      setOaaExportWizard((current) =>
        current ? { ...current, isRunning: true, progress: null, report: null } : current,
      );
      const report = await invoke<OaaExportReport>("export_oaa_archive_command", {
        request: {
          collection_id: workspace.collection.id,
          archive_path: archivePath,
          include_images: oaaExportWizard.includeImages,
          include_private_metadata: oaaExportWizard.includePrivateMetadata,
          ...(destinationExists ? { allow_overwrite: true } : {}),
        },
      });
      setOaaExportWizard((current) =>
        current ? { ...current, isRunning: false, report } : current,
      );
      setStatus(oaaExportReportSummary(report));
    } catch (caught) {
      setOaaExportWizard((current) => (current ? { ...current, isRunning: false } : current));
      setError(errorMessage(caught));
      setStatus("OAA export failed");
    }
  }

  function closeOaaExportWizard() {
    if (!oaaExportWizard || oaaExportWizard.isRunning) return;
    if (!oaaExportWizard.report) {
      setStatus("OAA export canceled");
    }
    setOaaExportWizard(null);
  }

  function advanceCafReconciliation(currentReconciliation = cafReconciliation) {
    if (
      currentReconciliation &&
      currentReconciliation.index + 1 < currentReconciliation.items.length
    ) {
      scrollElementToTop(cafReconciliationDialogRef.current);
    }
    setCafReconciliation((current) => {
      if (!current) return null;
      const nextIndex = current.index + 1;
      if (nextIndex >= current.items.length) return null;
      return { ...current, index: nextIndex, isResolving: false };
    });
  }

  async function resolveCafReconciliation(targetArtworkId: number | null) {
    const currentReconciliation = cafReconciliation;
    if (!currentReconciliation) return;
    const item = currentReconciliation.items[currentReconciliation.index] ?? null;
    if (!item || currentReconciliation.isResolving) return;

    try {
      setError("");
      setStatus("Resolving CAF CSV match");
      setCafReconciliation((current) => (current ? { ...current, isResolving: true } : current));
      const summary = await invoke<ArtworkSummary>("resolve_caf_reconciliation_command", {
        request: {
          item,
          target_artwork_id: targetArtworkId,
        },
      });
      const nextWorkspace = await loadWorkspace();
      if (
        selectedArtworkIdRef.current &&
        nextWorkspace?.artworks.some((artwork) => artwork.id === selectedArtworkIdRef.current)
      ) {
        await loadArtwork(selectedArtworkIdRef.current);
      }
      setStatus(`Resolved CAF CSV row: ${summary.title}`);
      advanceCafReconciliation(currentReconciliation);
    } catch (caught) {
      setError(errorMessage(caught));
      setStatus("CAF CSV reconciliation failed");
      setCafReconciliation((current) => (current ? { ...current, isResolving: false } : current));
    }
  }

  function skipCafReconciliation() {
    const currentReconciliation = cafReconciliation;
    if (!currentReconciliation) return;
    const item = currentReconciliation.items[currentReconciliation.index] ?? null;
    if (!item || currentReconciliation.isResolving) return;
    setStatus(`Skipped CAF CSV row: ${item.row.title}`);
    advanceCafReconciliation(currentReconciliation);
  }

  function advanceSniktReconciliation(currentReconciliation = sniktReconciliation) {
    if (
      currentReconciliation &&
      currentReconciliation.index + 1 < currentReconciliation.items.length
    ) {
      scrollElementToTop(sniktReconciliationDialogRef.current);
    }
    setSniktReconciliation((current) => {
      if (!current) return null;
      const nextIndex = current.index + 1;
      if (nextIndex >= current.items.length) return null;
      return { ...current, index: nextIndex, isResolving: false };
    });
  }

  async function resolveSniktReconciliation(targetArtworkId: number | null) {
    const currentReconciliation = sniktReconciliation;
    if (!currentReconciliation) return;
    const item = currentReconciliation.items[currentReconciliation.index] ?? null;
    if (!item || currentReconciliation.isResolving) return;

    try {
      setError("");
      setStatus("Resolving SNIKT.com CSV match");
      setSniktReconciliation((current) => (current ? { ...current, isResolving: true } : current));
      const summary = await invoke<ArtworkSummary>("resolve_snikt_reconciliation_command", {
        request: {
          item,
          target_artwork_id: targetArtworkId,
        },
      });
      const nextWorkspace = await loadWorkspace();
      if (
        selectedArtworkIdRef.current &&
        nextWorkspace?.artworks.some((artwork) => artwork.id === selectedArtworkIdRef.current)
      ) {
        await loadArtwork(selectedArtworkIdRef.current);
      }
      setStatus(`Resolved SNIKT.com CSV row: ${summary.title}`);
      advanceSniktReconciliation(currentReconciliation);
    } catch (caught) {
      setError(errorMessage(caught));
      setStatus("SNIKT.com CSV reconciliation failed");
      setSniktReconciliation((current) => (current ? { ...current, isResolving: false } : current));
    }
  }

  function skipSniktReconciliation() {
    const currentReconciliation = sniktReconciliation;
    if (!currentReconciliation) return;
    const item = currentReconciliation.items[currentReconciliation.index] ?? null;
    if (!item || currentReconciliation.isResolving) return;
    setStatus(`Skipped SNIKT.com CSV row: ${item.row.title}`);
    advanceSniktReconciliation(currentReconciliation);
  }

  async function saveCafMissingReport() {
    if (!cafMissingReport || cafMissingReport.isWriting) return;
    try {
      const defaultName = workspace?.collection
        ? `${suggestedExportFileStem(workspace.collection.name) || "collection"}-caf-missing.csv`
        : "caf-missing.csv";
      const target = await saveDialog({
        defaultPath: defaultName,
        filters: [{ name: "CAF Missing Report", extensions: ["csv"] }],
      });
      if (!target) return;
      setCafMissingReport((current) => (current ? { ...current, isWriting: true } : current));
      const rowsWritten = await invoke<number>("write_caf_missing_report_command", {
        request: {
          path: target,
          rows: cafMissingReport.rows,
        },
      });
      setStatus(`Wrote CAF missing item report: ${rowsWritten} rows`);
      setCafMissingReport(null);
    } catch (caught) {
      setError(errorMessage(caught));
      setCafMissingReport((current) => (current ? { ...current, isWriting: false } : current));
    }
  }

  async function exportOpenCollectionToRaremarqCsv() {
    if (!workspace?.collection) {
      setError("Open a Collection before exporting to Raremarq.");
      return;
    }
    try {
      setError("");
      const root = await ensureDefaultWorkspaceRoot();
      const defaultName = `${suggestedExportFileStem(workspace.collection.name) || "collection"}-raremarq.csv`;
      const plan = await invoke<RaremarqCsvExportPlan>("raremarq_csv_export_plan_command", {
        collectionId: workspace.collection.id,
      });
      setRaremarqExportWizard({
        plan,
        csvPath: root ? `${ensureTrailingPathSeparator(root)}${defaultName}` : defaultName,
        scope: appPreferences.raremarq_csv_export_scope,
        urlMode: appPreferences.raremarq_csv_url_mode,
        isRunning: false,
        progress: null,
        report: null,
      });
      setStatus("Preparing Raremarq CSV export");
    } catch (caught) {
      setError(errorMessage(caught));
      setStatus("Raremarq CSV export failed");
    }
  }

  async function pickRaremarqExportCsvPath() {
    if (!raremarqExportWizard || raremarqExportWizard.isRunning) return;
    try {
      setError("");
      const selected = await saveDialog({
        defaultPath: raremarqExportWizard.csvPath,
        filters: [
          {
            name: "Raremarq CSV",
            extensions: ["csv"],
          },
        ],
      });
      if (!selected || Array.isArray(selected)) return;
      setRaremarqExportWizard((current) =>
        current ? { ...current, csvPath: selected, report: null } : current,
      );
    } catch (caught) {
      setError(errorMessage(caught));
    }
  }

  async function submitRaremarqExportWizard() {
    if (!workspace?.collection || !raremarqExportWizard || raremarqExportWizard.isRunning) return;
    const csvPath = raremarqExportWizard.csvPath.trim();
    if (!csvPath) {
      setError("Raremarq CSV path is required.");
      return;
    }
    const selectedPlan = raremarqExportPlanScope(raremarqExportWizard);
    if (
      raremarqExportWizard.urlMode === "tmpfiles" &&
      selectedPlan.tmpfiles_missing_file_count > 0
    ) {
      setError(
        `${pluralize(selectedPlan.tmpfiles_missing_file_count, "entry", "entries")} cannot be uploaded because no primary file is attached.`,
      );
      return;
    }

    try {
      setError("");
      const destinationExists = await invoke<boolean>("destination_file_exists_command", {
        path: csvPath,
      });
      if (destinationExists) {
        const shouldReplace = await confirmDialog(
          `A Raremarq CSV already exists at this path:\n\n${csvPath}\n\nReplace it?`,
          {
            title: "Replace Raremarq CSV?",
            kind: "warning",
          },
        );
        if (!shouldReplace) {
          setStatus("Raremarq CSV export canceled");
          return;
        }
      }
      let confirmedTemporaryUpload = false;
      if (raremarqExportWizard.urlMode === "tmpfiles") {
        const uploadCount = selectedPlan.tmpfiles_upload_count;
        const downsizedCount = selectedPlan.tmpfiles_large_file_count;
        const shouldUpload = await confirmDialog(
          `Upload ${pluralize(uploadCount, "file")} to tmpfiles.org?\n\n${pluralize(downsizedCount, "downsized file")} will be resized before upload.\n\nThe host is tmpfiles.org, the files have a 24-hour expiry, and temporary public URLs will be written into the CSV.`,
          {
            title: "Upload temporary Raremarq images?",
            kind: "warning",
          },
        );
        if (!shouldUpload) {
          setStatus("Raremarq CSV export canceled");
          return;
        }
        confirmedTemporaryUpload = true;
      }
      setStatus("Exporting Raremarq CSV");
      setRaremarqExportWizard((current) =>
        current ? { ...current, isRunning: true, progress: null, report: null } : current,
      );
      const report = await invoke<RaremarqCsvExportReport>("export_raremarq_csv_command", {
        request: {
          collection_id: workspace.collection.id,
          csv_path: csvPath,
          scope: raremarqExportWizard.scope,
          url_mode: raremarqExportWizard.urlMode,
          ...(destinationExists ? { allow_overwrite: true } : {}),
          ...(confirmedTemporaryUpload ? { confirmed_temporary_upload: true } : {}),
        },
      });
      setRaremarqExportWizard((current) =>
        current ? { ...current, isRunning: false, report } : current,
      );
      await persistAppPreferences({
        ...appPreferences,
        raremarq_csv_export_scope: raremarqExportWizard.scope,
        raremarq_csv_url_mode: raremarqExportWizard.urlMode,
      });
      setStatus(raremarqCsvExportReportSummary(report));
      if (report.messages.length > 0) {
        setError(report.messages.join("\n"));
      }
    } catch (caught) {
      setRaremarqExportWizard((current) => (current ? { ...current, isRunning: false } : current));
      setError(errorMessage(caught));
      setStatus("Raremarq CSV export failed");
    }
  }

  function closeRaremarqExportWizard() {
    if (!raremarqExportWizard || raremarqExportWizard.isRunning) return;
    if (!raremarqExportWizard.report) {
      setStatus("Raremarq CSV export canceled");
    }
    setRaremarqExportWizard(null);
  }

  function workspaceCommandSubmitDisabled() {
    if (!workspaceCommand) return true;
    return (
      (workspaceCommand === "new_gallery" && !workspace?.collection) ||
      (workspaceCommandNeedsPath(workspaceCommand) && !workspaceCommandPath.trim()) ||
      (requiresWorkspaceCommandName(workspaceCommand) && !workspaceCommandName.trim()) ||
      (isSourceFileWorkspaceCommand(workspaceCommand) && !workspaceCommandCsvPath.trim()) ||
      (isImportWorkspaceCommand(workspaceCommand) &&
        !isSourceFileWorkspaceCommand(workspaceCommand) &&
        !workspaceCommandCafId.trim())
    );
  }

  function renderUpdateDialog() {
    if (!updateDialog) return null;
    const title = updateDialogTitle(updateDialog);

    return (
      <div className="workspace-command-backdrop">
        <section
          className="workspace-command workspace-command-modal update-modal"
          role="dialog"
          aria-modal="true"
          aria-labelledby="update-dialog-title"
          onKeyDown={(event) => {
            if (event.key === "Escape") {
              event.preventDefault();
              setUpdateDialog(null);
            }
          }}
        >
          <h3 id="update-dialog-title">{title}</h3>
          {updateDialog.state === "checking" && <p>Checking for OA Curator updates...</p>}
          {updateDialog.state === "none" && <p>This copy of OA Curator is current.</p>}
          {(updateDialog.state === "available" || updateDialog.state === "installing") && (
            <>
              <dl className="update-details">
                <div>
                  <dt>Version</dt>
                  <dd>Version {updateDialog.update.version}</dd>
                </div>
                {updateDialog.update.date && (
                  <div>
                    <dt>Published</dt>
                    <dd>{formatUpdateDate(updateDialog.update.date)}</dd>
                  </div>
                )}
              </dl>
              {updateDialog.update.body && (
                <pre className="update-notes">{updateDialog.update.body}</pre>
              )}
              {updateDialog.state === "installing" && (
                <div className="update-progress" role="status">
                  {updateDialog.progress?.total ? (
                    <progress
                      aria-label="Update download progress"
                      max={updateDialog.progress.total}
                      value={updateDialog.progress.downloaded}
                    />
                  ) : (
                    <progress aria-label="Update download progress" />
                  )}
                  <span>{formatUpdateProgress(updateDialog.progress)}</span>
                </div>
              )}
              {updateDialog.state === "available" && (
                <p className="update-warning">
                  OA Curator will close to finish installing this update on Windows.
                </p>
              )}
            </>
          )}
          {updateDialog.state === "error" && <p>{updateDialog.message}</p>}
          <div className="dialog-actions">
            {updateDialog.state === "available" && (
              <button type="button" onClick={() => void installSelectedUpdate(updateDialog.update)}>
                Install Update
              </button>
            )}
            <button
              type="button"
              disabled={updateDialog.state === "installing"}
              onClick={() => setUpdateDialog(null)}
            >
              {updateDialog.state === "installing" ? "Installing..." : "Close"}
            </button>
          </div>
        </section>
      </div>
    );
  }

  function renderPreferencesDialog() {
    if (!preferencesDialogOpen || !preferencesDraft) return null;

    return (
      <div className="workspace-command-backdrop">
        <section
          className="workspace-command workspace-command-modal preferences-modal"
          role="dialog"
          aria-modal="true"
          aria-labelledby="preferences-title"
          onKeyDown={(event) => {
            if (event.key === "Escape") {
              event.preventDefault();
              setPreferencesDialogOpen(false);
              setPreferencesDraft(null);
            }
          }}
        >
          <h3 id="preferences-title">Preferences</h3>
          <form
            onSubmit={(event) => {
              event.preventDefault();
              void savePreferencesDialog();
            }}
          >
            <fieldset className="preferences-fieldset">
              <legend>Workflow</legend>
              <label>
                Default attach mode
                <select
                  value={preferencesDraft.default_attach_mode}
                  onChange={(event) =>
                    updatePreferencesDraft(
                      "default_attach_mode",
                      event.currentTarget.value as AttachMode,
                    )
                  }
                >
                  <option value="copy">Copy files</option>
                  <option value="link">Link files</option>
                </select>
              </label>
              <label>
                Default PNG export
                <select
                  value={preferencesDraft.default_png_export_variant}
                  onChange={(event) =>
                    updatePreferencesDraft(
                      "default_png_export_variant",
                      event.currentTarget.value as PngExportVariant,
                    )
                  }
                >
                  <option value="basic">Basic - 800px height</option>
                  <option value="premium">Premium - 2000px height</option>
                </select>
              </label>
              <label>
                Default provider focus
                <select
                  value={preferencesDraft.default_provider_focus}
                  onChange={(event) =>
                    updatePreferencesDraft(
                      "default_provider_focus",
                      event.currentTarget.value as DefaultProviderFocus,
                    )
                  }
                >
                  <option value="all">All fields</option>
                  <option value="caf">CAF</option>
                  <option value="snikt">SNIKT.com</option>
                  <option value="raremarq">Raremarq</option>
                </select>
              </label>
            </fieldset>

            <fieldset className="preferences-fieldset">
              <legend>Display</legend>
              <label>
                Artwork ID label style
                <select
                  value={preferencesDraft.artwork_id_label_preference}
                  onChange={(event) =>
                    updatePreferencesDraft(
                      "artwork_id_label_preference",
                      event.currentTarget.value as ArtworkIdLabelPreference,
                    )
                  }
                >
                  <option value="oac">OAC IDs</option>
                  <option value="caf">Prefer CAF IDs</option>
                  <option value="snikt">Prefer SNIKT IDs</option>
                  <option value="raremarq">Prefer Raremarq IDs</option>
                </select>
              </label>
              <label>
                Default theme
                <select
                  value={preferencesDraft.theme}
                  onChange={(event) =>
                    updatePreferencesDraft("theme", event.currentTarget.value as ThemePreference)
                  }
                >
                  <option value="dracula">Dark</option>
                  <option value="alucard">Light</option>
                </select>
              </label>
            </fieldset>

            <fieldset className="preferences-fieldset">
              <legend>Startup</legend>
              <label>
                Startup behavior
                <select
                  value={preferencesDraft.startup_behavior}
                  onChange={(event) =>
                    updatePreferencesDraft(
                      "startup_behavior",
                      event.currentTarget.value as StartupBehaviorPreference,
                    )
                  }
                >
                  <option value="reopen_last">Reopen last Collection</option>
                  <option value="show_start_window">Show Start Window</option>
                  <option value="start_empty">Start empty</option>
                </select>
              </label>
              <label>
                Default workspace root
                <span className="workspace-command-file-row">
                  <input
                    value={preferencesDraft.default_workspace_root}
                    onChange={(event) =>
                      updatePreferencesDraft("default_workspace_root", event.currentTarget.value)
                    }
                  />
                  <button type="button" onClick={() => void pickDefaultWorkspaceRoot()}>
                    Browse
                  </button>
                </span>
              </label>
            </fieldset>

            <div className="button-row">
              <button type="submit" className="primary">
                Save Preferences
              </button>
              <button
                type="button"
                onClick={() => {
                  setPreferencesDialogOpen(false);
                  setPreferencesDraft(null);
                }}
              >
                Cancel
              </button>
            </div>
          </form>
        </section>
      </div>
    );
  }

  function renderStartupDialog() {
    if (
      startupDialogDismissed ||
      !recentCollectionsLoaded ||
      !workspace ||
      workspace.collection ||
      workspaceCommand
    ) {
      return null;
    }

    return (
      <div className="workspace-command-backdrop startup-dialog-backdrop">
        <section
          className="startup-dialog"
          role="dialog"
          aria-modal="true"
          aria-labelledby="startup-dialog-title"
        >
          <header className="startup-dialog-header">
            <img
              src={theme === "light" ? "/oac-logo-light-mode.svg" : "/oac-logo-dark-mode.svg"}
              alt=""
            />
            <div>
              <h2 id="startup-dialog-title">Open OA Curator</h2>
            </div>
          </header>

          <div className="startup-dialog-body">
            <section className="startup-recent-panel" aria-labelledby="startup-recent-title">
              <h3 id="startup-recent-title">Recent Collections</h3>
              <div className="startup-recent-list">
                {recentCollections.length > 0 ? (
                  recentCollections.map((recent) => (
                    <button
                      type="button"
                      className="startup-recent-item"
                      key={recent.path}
                      aria-label={`Open ${recent.name}`}
                      disabled={startupOpeningPath !== null}
                      onClick={() => void openRecentCollection(recent)}
                    >
                      <ToolbarIcon name="collection-open" />
                      <span className="startup-recent-name">{recent.name}</span>
                      <span className="startup-recent-path">{recent.path}</span>
                      <span className="startup-recent-date">
                        {formatRecentCollectionDate(recent.last_opened_at)}
                      </span>
                    </button>
                  ))
                ) : (
                  <div className="startup-empty-recent">
                    <p>No recent Collections yet.</p>
                  </div>
                )}
              </div>
            </section>

            <section className="startup-action-panel" aria-label="Collection actions">
              <button
                type="button"
                className="startup-action"
                onClick={() => beginStartupWorkspaceCommand("new_collection")}
              >
                <ToolbarIcon name="collection-new" />
                <span>New Collection</span>
              </button>
              <button
                type="button"
                className="startup-action"
                onClick={() => beginStartupWorkspaceCommand("open_collection")}
              >
                <ToolbarIcon name="collection-open" />
                <span>Open Collection</span>
              </button>
              <div className="startup-action-divider" />
              <button
                type="button"
                className="startup-action"
                onClick={() => beginStartupWorkspaceCommand("import_caf_collection")}
              >
                <ToolbarIcon name="file-import" />
                <span>Import CAF Collection</span>
              </button>
              <button
                type="button"
                className="startup-action"
                onClick={() => beginStartupWorkspaceCommand("import_snikt_collection")}
              >
                <ToolbarIcon name="file-import" />
                <span>Import SNIKT.com Collection</span>
              </button>
              <button
                type="button"
                className="startup-action"
                onClick={() => beginStartupWorkspaceCommand("import_oaa_archive")}
              >
                <ToolbarIcon name="file-import" />
                <span>Import OAA Archive</span>
              </button>
            </section>
          </div>

          <footer className="startup-dialog-footer">
            <button type="button" onClick={() => setStartupDialogDismissed(true)}>
              Start without a Collection
            </button>
          </footer>
        </section>
      </div>
    );
  }

  function renderCafMissingReportPrompt() {
    if (!cafMissingReport || cafReconciliation || sniktReconciliation) return null;
    const count = cafMissingReport.rows.length;
    const label = count === 1 ? "item" : "items";
    return (
      <div
        className="workspace-command workspace-command-modal"
        role="dialog"
        aria-modal="true"
        aria-labelledby="caf-missing-report-title"
      >
        <h3 id="caf-missing-report-title">CAF Missing Item Report</h3>
        <p className="workspace-command-warning">
          CAF CSV import did not include {count} tracked {label}. Save a report if you want to
          review what CAF omitted from the export.
        </p>
        <div className="workspace-command-actions">
          <button
            type="button"
            onClick={() => void saveCafMissingReport()}
            disabled={cafMissingReport.isWriting}
          >
            {cafMissingReport.isWriting ? "Writing report..." : "Save report CSV"}
          </button>
          <button
            type="button"
            onClick={() => setCafMissingReport(null)}
            disabled={cafMissingReport.isWriting}
          >
            Dismiss
          </button>
        </div>
      </div>
    );
  }

  return (
    <main className="app-shell" role="application" aria-label="OA Curator Workbench">
      <CommandBar
        theme={theme}
        onNewCollection={() => void beginWorkspaceCommand("new_collection")}
        onOpenCollection={() => void beginWorkspaceCommand("open_collection")}
        onCloseCollection={() => void closeCollection()}
        onImportCafCollection={() => void beginWorkspaceCommand("import_caf_collection")}
        onImportOaaArchive={() => void beginWorkspaceCommand("import_oaa_archive")}
        onExportOaaArchive={() => void exportOpenCollectionAsOaa()}
        onExportRaremarqCsv={() => void exportOpenCollectionToRaremarqCsv()}
        onImportSniktCollection={() => void beginWorkspaceCommand("import_snikt_collection")}
        importCafCollectionLabel={workspaceCommandDisplayLabel("import_caf_collection")}
        importSniktCollectionLabel={workspaceCommandDisplayLabel("import_snikt_collection")}
        onNewGallery={() => void beginWorkspaceCommand("new_gallery")}
        onNewArtwork={() => void createArtworkInActiveGallery()}
        onShowUserGuide={() => void openUserGuideWindow()}
        onShowAbout={() => setHelpPage("about")}
        onShowLicensing={() => setHelpPage("licensing")}
        onShowPreferences={openPreferencesDialog}
        onCheckForUpdates={() => void checkForUpdates()}
        onToggleTheme={toggleThemePreference}
        canCloseCollection={Boolean(workspace?.collection)}
        canCreateGallery={Boolean(workspace?.collection)}
        canCreateArtwork={Boolean(workspace?.collection)}
        canExportOaaArchive={Boolean(workspace?.collection)}
        canExportRaremarqCsv={Boolean(workspace?.collection)}
      />
      {error && <div className="error-banner">{error}</div>}
      {helpPage && <HelpPage page={helpPage} onClose={() => setHelpPage(null)} />}
      {renderUpdateDialog()}
      {renderPreferencesDialog()}
      {renderStartupDialog()}
      {workspaceCommand ? (
        <WorkspaceCommandDialog
          command={workspaceCommand}
          commandLabel={workspaceCommandDisplayLabel(workspaceCommand)}
          importMessage={workspaceCommandImportMessage(workspaceCommand)}
          needsPath={workspaceCommandNeedsPath(workspaceCommand)}
          initialFocusRef={workspaceCommandInitialFocusRef}
          name={workspaceCommandName}
          path={workspaceCommandPath}
          cafId={workspaceCommandCafId}
          sniktId={workspaceCommandSniktId}
          raremarqId={workspaceCommandRaremarqId}
          sniktGalleryInheritsCollection={workspaceCommandSniktGalleryInheritsCollection}
          sourceFilePath={workspaceCommandCsvPath}
          submitDisabled={workspaceCommandSubmitDisabled()}
          submitLabel={workspaceCommandSubmitDisplayLabel(workspaceCommand)}
          onSubmit={() => void submitWorkspaceCommand()}
          onCancel={() => setWorkspaceCommand(null)}
          onNameChange={updateWorkspaceCommandName}
          onPathChange={updateWorkspaceCommandPath}
          onPathFocus={selectSuggestedWorkspaceCommandPath}
          onCafIdChange={setWorkspaceCommandCafId}
          onSniktIdChange={setWorkspaceCommandSniktId}
          onRaremarqIdChange={setWorkspaceCommandRaremarqId}
          onSniktGalleryInheritsCollectionChange={setWorkspaceCommandSniktGalleryInheritsCollection}
          onSourceFilePathChange={setWorkspaceCommandCsvPath}
          onBrowseSourceFile={() =>
            void (workspaceCommand === "import_oaa_archive"
              ? pickOaaArchivePath()
              : pickCafCsvPath())
          }
        />
      ) : null}
      <CafReconciliationDialog
        reconciliation={cafReconciliation}
        dialogRef={cafReconciliationDialogRef}
        thumbUrls={cafReconciliationThumbUrls}
        onOpenUrl={(label, url) => void openArtworkUrl(label, url)}
        onResolve={(targetArtworkId) => void resolveCafReconciliation(targetArtworkId)}
        onSkip={skipCafReconciliation}
      />
      <SniktReconciliationDialog
        reconciliation={sniktReconciliation}
        dialogRef={sniktReconciliationDialogRef}
        thumbUrls={sniktReconciliationThumbUrls}
        onResolve={(targetArtworkId) => void resolveSniktReconciliation(targetArtworkId)}
        onSkip={skipSniktReconciliation}
      />
      {renderCafMissingReportPrompt()}
      {oaaExportWizard ? (
        <OaaExportDialog
          wizard={oaaExportWizard}
          onChange={setOaaExportWizard}
          onBrowse={() => void pickOaaExportArchivePath()}
          onSubmit={() => void submitOaaExportWizard()}
          onClose={closeOaaExportWizard}
        />
      ) : null}
      {raremarqExportWizard ? (
        <RaremarqExportDialog
          wizard={raremarqExportWizard}
          onChange={setRaremarqExportWizard}
          onBrowse={() => void pickRaremarqExportCsvPath()}
          onSubmit={() => void submitRaremarqExportWizard()}
          onClose={closeRaremarqExportWizard}
        />
      ) : null}
      {pendingDelete ? (
        <DeleteConfirmDialog
          itemLabel={explorerItemTypeLabel(pendingDelete.item)}
          preview={pendingDelete.preview}
          isDeleting={pendingDelete.isDeleting}
          onConfirm={() => void confirmPendingDelete()}
          onCancel={() => setPendingDelete(null)}
        />
      ) : null}
      {trashFailureReport ? (
        <TrashFailureDialog
          failures={trashFailureReport.failures}
          trashedFiles={trashFailureReport.trashedFiles}
          onClose={() => setTrashFailureReport(null)}
        />
      ) : null}

      <div className="workbench-frame">
        <Allotment defaultSizes={[300, 720, 360]}>
          <Allotment.Pane minSize={240}>
            <section className="collection-explorer" aria-label="Collection Explorer">
              {renderCollectionTreeControls()}
              {renderCollectionTree()}
              {renderCollectionExplorerStatus()}
            </section>
          </Allotment.Pane>

          <Allotment.Pane minSize={420}>
            <Allotment vertical defaultSizes={[600, 240]}>
              <Allotment.Pane minSize={320}>
                <section className="artwork-preview" aria-label="Artwork Preview">
                  {selectedSummary || detail ? (
                    <>
                      <div className="preview-stage">
                        {selectedCarouselItem ? (
                          selectedPreviewUrl ? (
                            <img
                              src={selectedPreviewUrl}
                              alt={`Preview ${selectedCarouselItem.name}`}
                            />
                          ) : selectedPreviewIsLoading && summaryPreviewUrl ? (
                            <img
                              src={summaryPreviewUrl}
                              alt={`Preview ${selectedSummary?.title ?? "selected artwork"}`}
                            />
                          ) : (
                            <span>{selectedCarouselItem.name}</span>
                          )
                        ) : !detail && selectedSummary && summaryPreviewUrl ? (
                          <img src={summaryPreviewUrl} alt={`Preview ${selectedSummary.title}`} />
                        ) : (
                          <span>
                            {detail?.display_id ??
                              selectedSummary?.display_id ??
                              selectedSummary?.canonical_id}
                          </span>
                        )}
                      </div>
                      {cacheWarnings.length > 0 && (
                        <div className="cache-warning-list" role="status">
                          {cacheWarnings.map((warning) => (
                            <p key={`${warning.file_asset_id}:${warning.path}`}>
                              {warning.message}
                            </p>
                          ))}
                        </div>
                      )}
                      {detail && (
                        <div className="thumbnail-carousel-row">
                          <div className="thumbnail-carousel" aria-label="Image thumbnails">
                            {carouselItems.length > 0 ? (
                              carouselItems.map((item) => {
                                const thumbnailUrl = item.thumbnailPath
                                  ? detailImageUrls[item.thumbnailPath]
                                  : null;
                                const isActive = selectedCarouselItem?.key === item.key;
                                const canMoveLeft =
                                  item.kind === "file" &&
                                  adjacentCarouselFileKey(item.key, "left") !== null;
                                const canMoveRight =
                                  item.kind === "file" &&
                                  adjacentCarouselFileKey(item.key, "right") !== null;
                                return (
                                  <div
                                    className={`thumbnail-item ${isActive ? "active" : ""}`}
                                    key={item.key}
                                  >
                                    <button
                                      type="button"
                                      className={`thumbnail-button ${isActive ? "active" : ""}`}
                                      aria-label={`Preview ${item.name}`}
                                      aria-pressed={isActive}
                                      data-carousel-item-key={item.key}
                                      data-carousel-kind={item.kind}
                                      onClick={() => setSelectedCarouselItemKey(item.key)}
                                    >
                                      {thumbnailUrl ? (
                                        <img src={thumbnailUrl} alt="" />
                                      ) : (
                                        <span>{item.name}</span>
                                      )}
                                    </button>
                                    {isActive && item.kind === "file" && (
                                      <div
                                        className="thumbnail-reorder-controls"
                                        aria-label={`Move ${item.name}`}
                                      >
                                        <button
                                          type="button"
                                          className="thumbnail-reorder-button"
                                          aria-label={`Move ${item.name} left`}
                                          title={`Move ${item.name} left`}
                                          disabled={!canMoveLeft}
                                          onClick={() =>
                                            void moveCarouselFileAsset(item.key, "left")
                                          }
                                        >
                                          <ToolbarIcon name="move-left" />
                                        </button>
                                        <button
                                          type="button"
                                          className="thumbnail-reorder-button"
                                          aria-label={`Move ${item.name} right`}
                                          title={`Move ${item.name} right`}
                                          disabled={!canMoveRight}
                                          onClick={() =>
                                            void moveCarouselFileAsset(item.key, "right")
                                          }
                                        >
                                          <ToolbarIcon name="move-right" />
                                        </button>
                                      </div>
                                    )}
                                  </div>
                                );
                              })
                            ) : (
                              <span className="empty-carousel">No images attached</span>
                            )}
                          </div>
                          <div className="attach-split-button carousel-attach">
                            <button
                              type="button"
                              disabled={!detail}
                              onClick={() => void pickFilesForSelectedArtwork(defaultAttachMode)}
                            >
                              Attach files
                            </button>
                            <button
                              type="button"
                              aria-label="Attach file options"
                              aria-haspopup="menu"
                              aria-expanded={attachMenuOpen}
                              disabled={!detail}
                              onClick={() => setAttachMenuOpen((open) => !open)}
                            >
                              v
                            </button>
                            {attachMenuOpen && (
                              <div className="attach-menu" role="menu">
                                <button
                                  type="button"
                                  role="menuitem"
                                  onClick={() => {
                                    setAttachMenuOpen(false);
                                    void pickFilesForSelectedArtwork("copy");
                                  }}
                                >
                                  Attach as Copy
                                </button>
                                <button
                                  type="button"
                                  role="menuitem"
                                  onClick={() => {
                                    setAttachMenuOpen(false);
                                    void pickFilesForSelectedArtwork("link");
                                  }}
                                >
                                  Attach as Link
                                </button>
                              </div>
                            )}
                          </div>
                        </div>
                      )}
                    </>
                  ) : (
                    <div className="empty-state">
                      <h2>Select an Artwork</h2>
                      <p>Choose an Artwork in the Collection Explorer to preview it.</p>
                    </div>
                  )}
                </section>
              </Allotment.Pane>
              <Allotment.Pane minSize={220}>{renderSelectedImageDetails()}</Allotment.Pane>
            </Allotment>
          </Allotment.Pane>

          <Allotment.Pane minSize={300}>
            <section
              className="artwork-properties"
              aria-label={inspectorPanelTitle}
              data-panel-title={inspectorPanelTitle}
            >
              <PropertySourceFilterBar
                filters={propertySourceFilters}
                onToggle={togglePropertySourceFilter}
              />
              {inspectedCollection ? (
                renderCollectionInspector(inspectedCollection)
              ) : inspectedGallery ? (
                renderGalleryInspector(inspectedGallery)
              ) : detail ? (
                <>
                  <div className="property-grid">
                    {renderFilteredProperty(
                      "CAF URL",
                      <UrlPropertyRow
                        label="CAF URL"
                        value={form.cafUrl}
                        onChange={updateField("cafUrl")}
                        onOpen={() => void openArtworkUrl("CAF URL", form.cafUrl)}
                      />,
                    )}
                    {renderFilteredProperty(
                      "SNIKT URL",
                      <UrlPropertyRow
                        label="SNIKT URL"
                        value={form.sniktUrl}
                        onChange={updateField("sniktUrl")}
                        onOpen={() => void openArtworkUrl("SNIKT URL", form.sniktUrl)}
                        extraAction={{
                          iconName: "cloud-upload",
                          label: "SNIKT export",
                          title: "Open SNIKT upload form with OAC metadata",
                          onClick: () => void openSniktUploadPrefill(),
                        }}
                      />,
                    )}
                    {renderFilteredProperty(
                      "Raremarq URL",
                      <UrlPropertyRow
                        label="Raremarq URL"
                        value={form.raremarqUrl}
                        onChange={updateField("raremarqUrl")}
                        onOpen={() => void openArtworkUrl("Raremarq URL", form.raremarqUrl)}
                      />,
                    )}
                    {renderFilteredProperty(
                      "Generic URL",
                      <UrlPropertyRow
                        label="Generic URL"
                        value={form.genericUrl}
                        onChange={updateField("genericUrl")}
                        onOpen={() => void openArtworkUrl("Generic URL", form.genericUrl)}
                      />,
                    )}
                    {renderFilteredProperty(
                      "Artwork ID",
                      <PropertyRow label="Artwork ID">
                        <input value={detail.display_id ?? detail.canonical_id} readOnly />
                      </PropertyRow>,
                    )}
                    {renderFilteredProperty(
                      "Gallery",
                      <PropertyRow label="Gallery">
                        <input
                          value={
                            selectedSummary?.gallery_names.join(", ") ?? selectedGallery?.name ?? ""
                          }
                          readOnly
                        />
                      </PropertyRow>,
                    )}
                    {renderFilteredProperty(
                      "Title",
                      <PropertyRow label="Title">
                        <input value={form.title} onChange={updateField("title")} />
                      </PropertyRow>,
                    )}
                    {renderFilteredProperty(
                      "Description",
                      <PropertyBlock label="Description">
                        <textarea value={form.description} onChange={updateField("description")} />
                      </PropertyBlock>,
                    )}
                    {renderFilteredProperty(
                      "For sale status",
                      <PropertyRow label="For sale status">
                        <input value={form.forSaleStatus} onChange={updateField("forSaleStatus")} />
                      </PropertyRow>,
                    )}
                    {renderFilteredProperty(
                      "Media type",
                      <PropertyRow label="Media type">
                        <select value={form.mediaTypeId} onChange={updateField("mediaTypeId")}>
                          {MEDIA_TYPE_OPTIONS.map((option) => (
                            <option value={option.id} key={option.id}>
                              {option.label}
                            </option>
                          ))}
                        </select>
                      </PropertyRow>,
                    )}
                    {renderFilteredProperty(
                      "Artwork type",
                      <PropertyRow label="Artwork type">
                        <select value={form.artTypeId} onChange={updateField("artTypeId")}>
                          {ART_TYPE_OPTIONS.map((option) => (
                            <option value={option.id} key={option.id}>
                              {option.label}
                            </option>
                          ))}
                        </select>
                      </PropertyRow>,
                    )}
                    {renderFilteredProperty(
                      "Publication status",
                      <PropertyRow label="Publication status">
                        <select
                          value={form.publicationStatusId}
                          onChange={updateField("publicationStatusId")}
                        >
                          {PUBLICATION_STATUS_OPTIONS.map((option) => (
                            <option value={option.id} key={option.id}>
                              {option.label}
                            </option>
                          ))}
                        </select>
                      </PropertyRow>,
                    )}
                    {renderFilteredProperty(
                      "Artist credits",
                      <div
                        className="artist-credit-editor property-block"
                        aria-label="Artist credits"
                      >
                        <span
                          className="property-block-label"
                          title={propertyHelpForLabel("Artist credits")}
                        >
                          Artist credits
                        </span>
                        {form.artistCredits.map((credit, index) => (
                          <div className="artist-credit-row" key={index}>
                            {showProperty("Artist first name") ? (
                              <label title={propertyHelpForLabel("Artist first name")}>
                                {`Artist ${index + 1} first name`}
                                <input
                                  value={credit.firstName}
                                  onChange={updateArtistCredit(index, "firstName")}
                                />
                              </label>
                            ) : null}
                            {showProperty("Artist last name") ? (
                              <label title={propertyHelpForLabel("Artist last name")}>
                                {`Artist ${index + 1} last name`}
                                <input
                                  value={credit.lastName}
                                  onChange={updateArtistCredit(index, "lastName")}
                                />
                              </label>
                            ) : null}
                            {showProperty("Artist role") ? (
                              <label title={propertyHelpForLabel("Artist role")}>
                                {`Role ${index + 1}`}
                                <select
                                  value={credit.roleId}
                                  onChange={updateArtistCredit(index, "roleId")}
                                >
                                  <option value="">Select Role</option>
                                  {ARTIST_ROLE_OPTIONS.map((role) => (
                                    <option value={role.id} key={role.id}>
                                      {role.label}
                                    </option>
                                  ))}
                                </select>
                              </label>
                            ) : null}
                            <button
                              type="button"
                              onClick={() => removeArtistCredit(index)}
                              disabled={
                                form.artistCredits.length === 1 && !artistCreditHasValue(credit)
                              }
                            >
                              Remove
                            </button>
                          </div>
                        ))}
                        <button type="button" onClick={addArtistCredit}>
                          Add artist
                        </button>
                      </div>,
                    )}
                    {renderFilteredProperty(
                      "Flags",
                      <div className="property-block property-flags" aria-label="Artwork flags">
                        <span
                          className="property-block-label"
                          title={propertyHelpForLabel("Flags")}
                        >
                          Flags
                        </span>
                        <div className="flag-row">
                          {showProperty("Active") ? (
                            <label
                              className="checkbox-field"
                              title={propertyHelpForLabel("Active")}
                            >
                              <input
                                type="checkbox"
                                checked={form.active}
                                onChange={updateCheckbox("active")}
                              />
                              Active
                            </label>
                          ) : null}
                          {showProperty("Illustration Exchange") ? (
                            <label
                              className="checkbox-field"
                              title={propertyHelpForLabel("Illustration Exchange")}
                            >
                              <input
                                type="checkbox"
                                checked={form.illustrationExchange}
                                onChange={updateCheckbox("illustrationExchange")}
                              />
                              Illustration Exchange
                            </label>
                          ) : null}
                          {showProperty("IX for sale") ? (
                            <label
                              className="checkbox-field"
                              title={propertyHelpForLabel("IX for sale")}
                            >
                              <input
                                type="checkbox"
                                checked={form.ixForSale}
                                onChange={updateCheckbox("ixForSale")}
                              />
                              IX for sale
                            </label>
                          ) : null}
                        </div>
                      </div>,
                    )}
                    {showSniktUploadGroup ? renderSniktMetadataGroup() : null}
                    {showPrivateDataGroup ? (
                      <div
                        className="property-block private-data-group"
                        role="group"
                        aria-label="Private Data"
                      >
                        <span
                          className="property-block-label"
                          title={propertyHelpForLabel("Private Data")}
                        >
                          Private Data
                        </span>
                        {renderFilteredProperty(
                          "Purchase price",
                          <PropertyRow label="Purchase price">
                            <input
                              value={form.purchasePrice}
                              onChange={updateField("purchasePrice")}
                            />
                          </PropertyRow>,
                        )}
                        {renderFilteredProperty(
                          "Estimated value",
                          <PropertyRow label="Estimated value">
                            <input
                              value={form.estimatedValue}
                              onChange={updateField("estimatedValue")}
                            />
                          </PropertyRow>,
                        )}
                        {renderFilteredProperty(
                          "Purchase date",
                          <PropertyRow label="Purchase date">
                            <input
                              type="date"
                              value={form.purchaseDate}
                              onChange={updateField("purchaseDate")}
                            />
                          </PropertyRow>,
                        )}
                        {renderFilteredProperty(
                          "Provenance",
                          <PropertyBlock label="Provenance">
                            <textarea
                              value={form.provenance}
                              onChange={updateField("provenance")}
                            />
                          </PropertyBlock>,
                        )}
                        {renderFilteredProperty(
                          "Personal notes",
                          <PropertyBlock label="Personal notes">
                            <textarea
                              value={form.personalNotes}
                              onChange={updateField("personalNotes")}
                            />
                          </PropertyBlock>,
                        )}
                      </div>
                    ) : null}
                  </div>
                </>
              ) : (
                <div className="empty-state">
                  <h2>No item selected</h2>
                  <p>Collection, Gallery, or Artwork details appear here after selection.</p>
                </div>
              )}
            </section>
          </Allotment.Pane>
        </Allotment>
      </div>

      {renderExplorerContextMenu()}
      {renderGalleryMergeDialog()}
      {renderArtworkMergeDialog()}

      <StatusBar status={status} />
    </main>
  );

  function renderCollectionInspector(collection: CollectionSummary) {
    return (
      <div className="property-grid collection-inspector">
        {renderFilteredProperty(
          "Collection name",
          <PropertyRow label="Collection name">
            <input
              value={collection.name}
              onChange={(event) => updateCollectionNameField(collection, event.currentTarget.value)}
              onBlur={(event) => void saveCollectionName(collection, event.currentTarget.value)}
            />
          </PropertyRow>,
        )}
        {renderFilteredProperty(
          "Manifest path",
          <PropertyRow label="Manifest path">
            <input value={collection.manifest_path} readOnly />
          </PropertyRow>,
        )}
        {renderFilteredProperty(
          "CAF Collection ID",
          <PropertyRow label="CAF Collection ID">
            <input
              value={collection.caf_collection_id ?? ""}
              onChange={(event) =>
                updateCollectionProviderField(
                  collection,
                  "caf_collection_id",
                  event.currentTarget.value,
                )
              }
              onBlur={(event) =>
                void saveCollectionProviderIds(collection, {
                  caf_collection_id: event.currentTarget.value,
                })
              }
            />
          </PropertyRow>,
        )}
        {renderFilteredProperty(
          "SNIKT Collection ID",
          <PropertyRow label="SNIKT Collection ID">
            <input
              value={collection.snikt_collection_id ?? ""}
              onChange={(event) =>
                updateCollectionProviderField(
                  collection,
                  "snikt_collection_id",
                  event.currentTarget.value,
                )
              }
              onBlur={(event) =>
                void saveCollectionProviderIds(collection, {
                  snikt_collection_id: event.currentTarget.value,
                })
              }
            />
          </PropertyRow>,
        )}
        {renderFilteredProperty(
          "Raremarq Collection ID",
          <PropertyRow label="Raremarq Collection ID">
            <input
              value={collection.raremarq_collection_id ?? ""}
              onChange={(event) =>
                updateCollectionProviderField(
                  collection,
                  "raremarq_collection_id",
                  event.currentTarget.value,
                )
              }
              onBlur={(event) =>
                void saveCollectionProviderIds(collection, {
                  raremarq_collection_id: event.currentTarget.value,
                })
              }
            />
          </PropertyRow>,
        )}
        {renderFilteredProperty(
          "Galleries",
          <PropertyRow label="Galleries">
            <input value={String(galleries.length)} readOnly />
          </PropertyRow>,
        )}
        {renderFilteredProperty(
          "Artworks",
          <PropertyRow label="Artworks">
            <input value={String(artworks.length)} readOnly />
          </PropertyRow>,
        )}
      </div>
    );
  }

  function renderGalleryInspector(gallery: GallerySummary) {
    const sniktGalleryValue = gallery.snikt_gallery_inherits_collection
      ? (workspace?.collection?.snikt_collection_id ?? "")
      : (gallery.snikt_gallery_id ?? "");

    return (
      <div className="property-grid gallery-inspector">
        {renderFilteredProperty(
          "Gallery name",
          <PropertyRow label="Gallery name">
            <input
              value={gallery.name}
              onChange={(event) => updateGalleryNameField(gallery, event.currentTarget.value)}
              onBlur={(event) => void saveGalleryName(gallery, event.currentTarget.value)}
            />
          </PropertyRow>,
        )}
        {renderFilteredProperty(
          "Manifest path",
          <PropertyRow label="Manifest path">
            <input value={gallery.manifest_path} readOnly />
          </PropertyRow>,
        )}
        {renderFilteredProperty(
          "CAF Gallery Room ID",
          <PropertyRow label="CAF Gallery Room ID">
            <input
              value={gallery.caf_gallery_room_id ?? ""}
              onChange={(event) =>
                updateGalleryProviderField(
                  gallery,
                  "caf_gallery_room_id",
                  event.currentTarget.value,
                )
              }
              onBlur={(event) =>
                void saveGalleryProviderIds(gallery, {
                  caf_gallery_room_id: event.currentTarget.value,
                })
              }
            />
          </PropertyRow>,
        )}
        {renderFilteredProperty(
          "SNIKT Gallery ID",
          <PropertyRow label="SNIKT Gallery ID">
            <input aria-label="SNIKT Gallery ID" value={sniktGalleryValue} readOnly />
            {gallery.snikt_gallery_inherits_collection && sniktGalleryValue ? (
              <span className="property-note">Inherited from Collection</span>
            ) : null}
          </PropertyRow>,
        )}
        {renderFilteredProperty(
          "Inherit SNIKT Collection ID",
          <PropertyRow label="Inherit SNIKT Collection ID">
            <input
              aria-label="Inherit SNIKT Collection ID"
              type="checkbox"
              checked={gallery.snikt_gallery_inherits_collection}
              onChange={(event) => {
                const inherits = event.currentTarget.checked;
                updateGalleryProviderField(gallery, "snikt_gallery_inherits_collection", inherits);
                void saveGalleryProviderIds(gallery, {
                  snikt_gallery_inherits_collection: inherits,
                });
              }}
            />
          </PropertyRow>,
        )}
        {renderFilteredProperty(
          "Raremarq Gallery ID",
          <PropertyRow label="Raremarq Gallery ID">
            <input
              value={gallery.raremarq_gallery_id ?? ""}
              onChange={(event) =>
                updateGalleryProviderField(
                  gallery,
                  "raremarq_gallery_id",
                  event.currentTarget.value,
                )
              }
              onBlur={(event) =>
                void saveGalleryProviderIds(gallery, {
                  raremarq_gallery_id: event.currentTarget.value,
                })
              }
            />
          </PropertyRow>,
        )}
        {renderFilteredProperty(
          "Artworks",
          <PropertyRow label="Artworks">
            <input value={String(artworksForGallery(gallery).length)} readOnly />
          </PropertyRow>,
        )}
      </div>
    );
  }

  function renderCollectionTree() {
    const collectionName =
      workspace?.collection?.name ??
      (workspace?.mode === "loose" ? "Loose Galleries" : "No Collection");
    const collectionKey = "collection";
    const collectionExpanded = isTreeNodeExpanded(collectionKey);
    const collectionSelected =
      inspectorTarget?.type === "collection" &&
      workspace?.collection?.id === inspectorTarget.collectionId;
    const searchActive = isCollectionSearchActive();
    const visibleGalleries = searchActive
      ? galleries.filter((gallery) => artworksForGallery(gallery).length > 0)
      : galleries;

    return (
      <div
        className="collection-tree"
        role="tree"
        aria-label="Collection contents"
        ref={collectionTreeRef}
        onScroll={handleCollectionTreeScroll}
      >
        <div
          className={`tree-row depth-0 collection-node ${collectionSelected ? "selected-row" : ""}`}
          role="treeitem"
          aria-expanded={collectionExpanded}
          aria-selected={collectionSelected}
          aria-label={`Collection ${collectionName}`}
          tabIndex={0}
          onClick={(event) => {
            if (!workspace?.collection) return;
            if ((event.target as HTMLElement).closest("button,input,textarea,select")) return;
            selectCollectionFromTree(workspace.collection);
          }}
          onKeyDown={(event) => {
            if (workspace?.collection) {
              handleExplorerItemKeyDown(event, {
                type: "collection",
                collection: workspace.collection,
              });
            }
          }}
          onContextMenu={(event) => {
            if (workspace?.collection) {
              openExplorerContextMenu(event, {
                type: "collection",
                collection: workspace.collection,
              });
            }
          }}
        >
          {renderTreeToggle(collectionKey, collectionExpanded, `collection ${collectionName}`)}
          <span className="tree-row-static">
            <span className="tree-icon" aria-hidden="true" />
            {workspace?.collection &&
            isRenamingExplorerItem({ type: "collection", collection: workspace.collection }) ? (
              renderExplorerRenameInput("Rename Collection")
            ) : (
              <span className="tree-label">{collectionName}</span>
            )}
          </span>
          {workspace?.collection ? (
            renderTreeActionButton(
              "Add gallery",
              `Add gallery to ${collectionName}`,
              "gallery-new",
              () => void beginWorkspaceCommand("new_gallery"),
            )
          ) : (
            <span className="tree-action-gutter" aria-hidden="true" />
          )}
        </div>

        {collectionExpanded && searchActive && artworks.length === 0 ? (
          <div className="empty-browser">
            <p>No Artworks match the current search.</p>
          </div>
        ) : collectionExpanded && visibleGalleries.length > 0 ? (
          visibleGalleries.map((gallery, index) =>
            renderGalleryTreeNode(gallery, visibleGalleries, index),
          )
        ) : collectionExpanded ? (
          <div className="empty-browser">
            <p>Create or open a Gallery to start adding Artworks.</p>
          </div>
        ) : null}
      </div>
    );
  }

  function isCollectionSearchActive() {
    return collectionSearchQuery.trim().length > 0;
  }

  function renderCollectionTreeControls() {
    return (
      <div
        className="collection-tree-toolbar"
        role="toolbar"
        aria-label="Collection Explorer tree controls"
      >
        <button
          type="button"
          aria-label="Collapse all"
          title="Collapse all"
          onClick={collapseAllTreeNodes}
        >
          <ToolbarIcon name="tree-collapse-all" />
        </button>
        <button
          type="button"
          aria-label="Expand all"
          title="Expand all"
          onClick={expandAllTreeNodes}
        >
          <ToolbarIcon name="tree-expand-all" />
        </button>
        <input
          type="search"
          aria-label="Search collection"
          className="collection-tree-search"
          placeholder="Search"
          value={collectionSearchQuery}
          onChange={(event) => setCollectionSearchQuery(event.currentTarget.value)}
        />
      </div>
    );
  }

  function renderCollectionExplorerStatus() {
    const count = artworks.length;
    const suffix = count === 1 ? "artwork" : "artworks";
    const text = isCollectionSearchActive()
      ? `Viewing ${count} filtered ${suffix}`
      : `Viewing ${count} ${suffix}`;

    return (
      <div className="collection-explorer-status" role="status">
        {text}
      </div>
    );
  }

  function renderGalleryTreeNode(
    gallery: GallerySummary,
    visibleGalleries: GallerySummary[],
    galleryIndex: number,
  ) {
    const galleryArtworks = artworksForGallery(gallery);
    const selected =
      inspectorTarget?.type === "gallery" && inspectorTarget.galleryId === gallery.id;
    const galleryKey = treeKeyForGallery(gallery.id);
    const galleryExpanded = isTreeNodeExpanded(galleryKey);

    return (
      <div className="tree-branch" role="group" key={gallery.id}>
        <div
          className={`tree-row depth-1 collection-node ${selected ? "selected-row" : ""}`}
          role="treeitem"
          aria-expanded={galleryExpanded}
          aria-selected={selected}
          aria-label={`Gallery ${gallery.name}`}
          tabIndex={0}
          onKeyDown={(event) =>
            handleExplorerItemKeyDown(event, {
              type: "gallery",
              gallery,
              collectionId: workspace?.collection?.id ?? null,
            })
          }
          onContextMenu={(event) =>
            openExplorerContextMenu(event, {
              type: "gallery",
              gallery,
              collectionId: workspace?.collection?.id ?? null,
            })
          }
        >
          {renderTreeToggle(galleryKey, galleryExpanded, `gallery ${gallery.name}`)}
          {isRenamingExplorerItem({
            type: "gallery",
            gallery,
            collectionId: workspace?.collection?.id ?? null,
          }) ? (
            <span className="tree-row-static">
              <span className="tree-icon" aria-hidden="true" />
              {renderExplorerRenameInput("Rename Gallery")}
            </span>
          ) : (
            <button
              type="button"
              className="tree-row-button"
              onClick={() => void selectGalleryFromTree(gallery)}
            >
              <span className="tree-icon" aria-hidden="true" />
              <span className="tree-label">{gallery.name}</span>
            </button>
          )}
          {renderTreeActionButton(
            "Add artwork",
            `Add artwork to ${gallery.name}`,
            "artwork-new",
            () => void createArtworkForGallery(gallery.id),
          )}
        </div>

        {galleryExpanded && galleryArtworks.length > 0 ? (
          renderGalleryArtworkRows(galleryArtworks, gallery, visibleGalleries, galleryIndex)
        ) : galleryExpanded ? (
          <div className="tree-row depth-2 muted-node" role="treeitem" aria-label="No Artworks">
            <span className="tree-disclosure" />
            <span className="tree-row-static">
              <span className="tree-icon" aria-hidden="true" />
              <span className="tree-label">No Artworks</span>
            </span>
            <span className="tree-action-gutter" aria-hidden="true" />
          </div>
        ) : null}
      </div>
    );
  }

  function renderGalleryArtworkRows(
    galleryArtworks: ArtworkSummary[],
    gallery: GallerySummary,
    visibleGalleries: GallerySummary[],
    galleryIndex: number,
  ) {
    const canVirtualize =
      galleryArtworks.length > EXPLORER_VIRTUALIZATION_THRESHOLD &&
      expandedFileTreeNodes.size === 0;
    if (!canVirtualize) {
      return galleryArtworks.map((artwork) => renderArtworkTreeNode(artwork, gallery));
    }

    const rowsBeforeGallery = visibleGalleries.slice(0, galleryIndex).reduce((count, item) => {
      const itemExpanded = isTreeNodeExpanded(treeKeyForGallery(item.id));
      if (!itemExpanded) return count + 1;
      return count + 1 + Math.max(artworksForGallery(item).length, 1);
    }, 1);
    const artworkRowsTop = (rowsBeforeGallery + 1) * EXPLORER_TREE_ROW_ESTIMATE_PX;
    const viewportTop = Math.max(0, collectionTreeViewport.scrollTop - artworkRowsTop);
    const viewportHeight = collectionTreeViewport.height || EXPLORER_TREE_ROW_ESTIMATE_PX * 20;
    const visibleStart = Math.max(
      0,
      Math.floor(viewportTop / EXPLORER_TREE_ROW_ESTIMATE_PX) - EXPLORER_TREE_OVERSCAN_ROWS,
    );
    const visibleCount =
      Math.ceil(viewportHeight / EXPLORER_TREE_ROW_ESTIMATE_PX) + EXPLORER_TREE_OVERSCAN_ROWS * 2;
    const visibleEnd = Math.min(galleryArtworks.length, visibleStart + visibleCount);
    const topSpacerHeight = visibleStart * EXPLORER_TREE_ROW_ESTIMATE_PX;
    const bottomSpacerHeight =
      (galleryArtworks.length - visibleEnd) * EXPLORER_TREE_ROW_ESTIMATE_PX;

    return (
      <div className="tree-virtual-artworks" role="group">
        {topSpacerHeight > 0 ? (
          <div className="tree-virtual-spacer" style={{ height: topSpacerHeight }} />
        ) : null}
        {galleryArtworks
          .slice(visibleStart, visibleEnd)
          .map((artwork) => renderArtworkTreeNode(artwork, gallery))}
        {bottomSpacerHeight > 0 ? (
          <div className="tree-virtual-spacer" style={{ height: bottomSpacerHeight }} />
        ) : null}
      </div>
    );
  }

  function renderArtworkTreeNode(artwork: ArtworkSummary, gallery: GallerySummary) {
    const artworkLabelId = artwork.display_id || artwork.canonical_id;
    const selected = inspectorTarget?.type === "artwork" && selectedArtworkId === artwork.id;
    const artworkKey = treeKeyForArtwork(artwork.id);
    const artworkExpanded = isTreeNodeExpanded(artworkKey);
    return (
      <div className="tree-branch" role="group" key={artwork.id}>
        <div
          className={`tree-row depth-2 artwork-node ${selected ? "selected-row" : ""}`}
          role="treeitem"
          aria-expanded={artworkExpanded}
          aria-selected={selected}
          aria-label={`Artwork ${artworkLabelId} ${artwork.title} ${artwork.file_count} files`}
          tabIndex={0}
          onKeyDown={(event) =>
            handleExplorerItemKeyDown(event, { type: "artwork", artwork, galleryId: gallery.id })
          }
          onContextMenu={(event) =>
            openExplorerContextMenu(event, { type: "artwork", artwork, galleryId: gallery.id })
          }
        >
          {renderTreeToggle(artworkKey, artworkExpanded, `artwork ${artworkLabelId}`)}
          {isRenamingExplorerItem({ type: "artwork", artwork, galleryId: gallery.id }) ? (
            <span className="tree-row-static">
              <span className="tree-icon tree-icon-artwork" aria-hidden="true">
                <ToolbarIcon name="artwork-open" />
              </span>
              <span className="tree-label">
                <span>{artworkLabelId}</span>
                {renderExplorerRenameInput("Rename Artwork Title")}
              </span>
            </span>
          ) : (
            <button
              type="button"
              className="tree-row-button"
              onClick={() => void loadArtworkFromTree(artwork, gallery.id)}
              aria-label={`Open artwork ${artworkLabelId}`}
            >
              <span className="tree-icon tree-icon-artwork" aria-hidden="true">
                <ToolbarIcon name="artwork-open" />
              </span>
              <span className="tree-label">
                <span>{artworkLabelId}</span>
                <strong>{artwork.title}</strong>
              </span>
            </button>
          )}
          {renderTreeActionButton(
            "Add file",
            `Add file to ${artworkLabelId}`,
            "file-new",
            () => void pickFilesForArtwork(artwork.id, "copy"),
          )}
        </div>
        {artworkExpanded && renderArtworkFilesTree(artwork, gallery)}
      </div>
    );
  }

  function renderArtworkFilesTree(artwork: ArtworkSummary, gallery: GallerySummary) {
    const selected = detail?.id === artwork.id;
    const fileItems = selected ? carouselItems : [];
    const fileCount = selected ? fileItems.length : artwork.file_count;
    const filesKey = treeKeyForFiles(artwork.id);
    const filesExpanded = isFilesTreeExpanded(artwork.id, detail?.id);

    return (
      <div className="tree-branch" role="group">
        <div
          className="tree-row depth-3 files-node"
          role="treeitem"
          aria-expanded={filesExpanded}
          aria-label={`Files for ${artwork.display_id || artwork.canonical_id} ${fileCount}`}
          onClick={() => void toggleFilesTreeNode(artwork, gallery.id)}
        >
          {renderTreeToggle(
            filesKey,
            filesExpanded,
            `files for ${artwork.display_id || artwork.canonical_id}`,
            () => void toggleFilesTreeNode(artwork, gallery.id),
          )}
          <span className="tree-row-static">
            <span className="tree-icon" aria-hidden="true" />
            <span className="tree-label">Files</span>
            <small>{fileCount}</small>
          </span>
          <span className="tree-action-gutter" aria-hidden="true" />
        </div>
        {filesExpanded && fileItems.map((item) => renderFileTreeNode(artwork.id, item))}
      </div>
    );
  }

  function renderTreeToggle(key: string, expanded: boolean, label: string, onToggle?: () => void) {
    return (
      <button
        type="button"
        className="tree-disclosure-button"
        aria-label={`${expanded ? "Collapse" : "Expand"} ${label}`}
        onClick={(event) => {
          event.stopPropagation();
          (onToggle ?? (() => toggleTreeNode(key)))();
        }}
      >
        {expanded ? "v" : ">"}
      </button>
    );
  }

  function renderFileTreeNode(artworkId: number, item: CarouselImageItem) {
    const selected = selectedCarouselItem?.key === item.key;
    return (
      <div
        className="tree-row depth-4 file-node"
        role="treeitem"
        aria-label={`File ${item.name}`}
        key={item.key}
        tabIndex={0}
        onKeyDown={(event) =>
          handleExplorerItemKeyDown(event, { type: "file", file: item, artworkId })
        }
        onContextMenu={(event) =>
          openExplorerContextMenu(event, { type: "file", file: item, artworkId })
        }
      >
        <span className="tree-disclosure" />
        {isRenamingExplorerItem({ type: "file", file: item, artworkId }) ? (
          <span className={`tree-row-static ${selected ? "active-file" : ""}`}>
            <span className="tree-icon" aria-hidden="true" />
            {renderExplorerRenameInput("Rename File")}
            <small>{item.status}</small>
          </span>
        ) : (
          <button
            type="button"
            className={`tree-row-button ${selected ? "active-file" : ""}`}
            aria-label={`Select file ${item.name}`}
            onClick={() => setSelectedCarouselItemKey(item.key)}
          >
            <span className="tree-icon" aria-hidden="true" />
            <span className="tree-label">{item.name}</span>
            <small>{item.status}</small>
          </button>
        )}
        <span className="tree-action-gutter" aria-hidden="true" />
      </div>
    );
  }

  function renderTreeActionButton(
    label: string,
    ariaLabel: string,
    icon: string,
    onClick: () => void,
  ) {
    return (
      <span className="tree-action-gutter">
        <button
          type="button"
          className="tree-action-button"
          aria-label={ariaLabel}
          title={label}
          onClick={onClick}
        >
          <ToolbarIcon name={icon} />
        </button>
      </span>
    );
  }

  function renderExplorerRenameInput(ariaLabel: string) {
    return (
      <input
        className="tree-rename-input"
        aria-label={ariaLabel}
        value={pendingRename?.value ?? ""}
        disabled={pendingRename?.isSaving}
        autoFocus
        onFocus={(event) => event.currentTarget.select()}
        onClick={(event) => event.stopPropagation()}
        onMouseDown={(event) => event.stopPropagation()}
        onChange={(event) => {
          const nextValue = event.currentTarget.value;
          setPendingRename((current) => (current ? { ...current, value: nextValue } : current));
        }}
        onKeyDown={(event) => {
          if (event.key === "Enter") {
            event.preventDefault();
            event.stopPropagation();
            void commitPendingRename(event.currentTarget.value);
          } else if (event.key === "Escape") {
            event.preventDefault();
            event.stopPropagation();
            setPendingRename(null);
          }
        }}
      />
    );
  }

  function renderGalleryMergeDialog() {
    if (!galleryMerge || !workspace?.collection) return null;
    const source = galleryMerge.source;
    const targetOptions = galleries.filter((gallery) => gallery.id !== source.id);
    const target = targetOptions.find((gallery) => String(gallery.id) === galleryMerge.targetId);
    const sourceArtworkCount = artworksForGallery(source).length;
    const targetArtworkCount = target ? artworksForGallery(target).length : 0;
    const mergedArtworkCount = target
      ? new Set([
          ...artworksForGallery(source).map((artwork) => artwork.id),
          ...artworksForGallery(target).map((artwork) => artwork.id),
        ]).size
      : 0;

    return (
      <div className="workspace-command-backdrop">
        <section
          className="workspace-command workspace-command-modal gallery-merge-modal"
          role="dialog"
          aria-modal="true"
          aria-labelledby="gallery-merge-title"
        >
          <form onSubmit={(event) => void executeGalleryMerge(event)}>
            <h3 id="gallery-merge-title">Merge Gallery</h3>
            <div className="gallery-merge-grid">
              <section className="gallery-merge-column" role="region" aria-label="Source Gallery">
                <h4>Source</h4>
                {renderGalleryMergeReadOnlyProperties(source, sourceArtworkCount)}
              </section>

              <section className="gallery-merge-column" role="region" aria-label="Merged Gallery">
                <h4>Merged</h4>
                {target ? (
                  <div className="property-grid gallery-merge-properties">
                    <PropertyRow label="Gallery name">
                      <input
                        value={galleryMerge.name}
                        onChange={(event) =>
                          setGalleryMerge({
                            ...galleryMerge,
                            name: event.currentTarget.value,
                          })
                        }
                      />
                    </PropertyRow>
                    <PropertyRow label="Manifest path">
                      <input value={target.manifest_path} readOnly />
                    </PropertyRow>
                    <PropertyRow label="CAF Gallery Room ID">
                      <input
                        value={galleryMerge.cafGalleryRoomId}
                        onChange={(event) =>
                          setGalleryMerge({
                            ...galleryMerge,
                            cafGalleryRoomId: event.currentTarget.value,
                          })
                        }
                      />
                    </PropertyRow>
                    <PropertyRow label="SNIKT Gallery ID">
                      <input value={galleryMergeSniktValue(target, galleryMerge)} readOnly />
                    </PropertyRow>
                    <PropertyRow label="Inherit SNIKT Collection ID">
                      <input
                        aria-label="Inherit SNIKT Collection ID"
                        type="checkbox"
                        checked={galleryMerge.sniktGalleryInheritsCollection}
                        onChange={(event) =>
                          setGalleryMerge({
                            ...galleryMerge,
                            sniktGalleryInheritsCollection: event.currentTarget.checked,
                          })
                        }
                      />
                    </PropertyRow>
                    <PropertyRow label="Raremarq Gallery ID">
                      <input
                        value={galleryMerge.raremarqGalleryId}
                        onChange={(event) =>
                          setGalleryMerge({
                            ...galleryMerge,
                            raremarqGalleryId: event.currentTarget.value,
                          })
                        }
                      />
                    </PropertyRow>
                    <PropertyRow label="Artworks">
                      <input value={String(mergedArtworkCount)} readOnly />
                    </PropertyRow>
                  </div>
                ) : (
                  <p className="muted-node">Select a target Gallery to preview merged values.</p>
                )}
              </section>

              <section className="gallery-merge-column" role="region" aria-label="Target Gallery">
                <h4>Target</h4>
                <label className="property-row gallery-merge-target-row">
                  <span className="property-key">Target gallery</span>
                  <span className="property-value">
                    <select
                      aria-label="Target gallery"
                      value={galleryMerge.targetId}
                      onChange={(event) => updateGalleryMergeTarget(event.currentTarget.value)}
                      disabled={galleryMerge.isMerging}
                    >
                      <option value="">Select a Gallery</option>
                      {targetOptions.map((option) => (
                        <option key={option.id} value={option.id}>
                          {option.name}
                        </option>
                      ))}
                    </select>
                  </span>
                </label>
                {target ? (
                  renderGalleryMergeReadOnlyProperties(target, targetArtworkCount)
                ) : (
                  <p className="muted-node">No target Gallery selected.</p>
                )}
              </section>
            </div>

            <div className="dialog-actions">
              <button
                type="submit"
                className="primary"
                disabled={!target || galleryMerge.isMerging}
              >
                Merge Gallery
              </button>
              <button
                type="button"
                disabled={galleryMerge.isMerging}
                onClick={() => setGalleryMerge(null)}
              >
                Cancel
              </button>
            </div>
          </form>
        </section>
      </div>
    );
  }

  function renderGalleryMergeReadOnlyProperties(gallery: GallerySummary, artworkCount: number) {
    return (
      <div className="property-grid gallery-merge-properties">
        <PropertyRow label="Gallery name">
          <input value={gallery.name} readOnly />
        </PropertyRow>
        <PropertyRow label="Manifest path">
          <input value={gallery.manifest_path} readOnly />
        </PropertyRow>
        <PropertyRow label="CAF Gallery Room ID">
          <input value={gallery.caf_gallery_room_id ?? ""} readOnly />
        </PropertyRow>
        <PropertyRow label="SNIKT Gallery ID">
          <input value={galleryMergeSniktValue(gallery)} readOnly />
        </PropertyRow>
        <PropertyRow label="Inherit SNIKT Collection ID">
          <input
            aria-label="Inherit SNIKT Collection ID"
            type="checkbox"
            checked={gallery.snikt_gallery_inherits_collection}
            readOnly
          />
        </PropertyRow>
        <PropertyRow label="Raremarq Gallery ID">
          <input value={gallery.raremarq_gallery_id ?? ""} readOnly />
        </PropertyRow>
        <PropertyRow label="Artworks">
          <input value={String(artworkCount)} readOnly />
        </PropertyRow>
      </div>
    );
  }

  function galleryMergeSniktValue(gallery: GallerySummary, draft?: GalleryMergeDraft) {
    const inherits =
      draft?.sniktGalleryInheritsCollection ?? gallery.snikt_gallery_inherits_collection;
    return inherits
      ? (workspace?.collection?.snikt_collection_id ?? "")
      : (gallery.snikt_gallery_id ?? "");
  }

  function renderArtworkMergeDialog() {
    if (!artworkMerge || !workspace?.collection) return null;
    const source = artworkMerge.source;
    const targetOptions = artworks.filter((artwork) => artwork.id !== source.id);
    const target = targetOptions.find((artwork) => String(artwork.id) === artworkMerge.targetId);
    const mergedFileCount =
      artworkMerge.sourceDetail && artworkMerge.targetDetail
        ? artworkFileCountFromDetail(artworkMerge.sourceDetail) +
          artworkFileCountFromDetail(artworkMerge.targetDetail)
        : target
          ? source.file_count + target.file_count
          : 0;

    return (
      <div className="workspace-command-backdrop">
        <section
          className="workspace-command workspace-command-modal gallery-merge-modal artwork-merge-modal"
          role="dialog"
          aria-modal="true"
          aria-labelledby="artwork-merge-title"
        >
          <form onSubmit={(event) => void executeArtworkMerge(event)}>
            <h3 id="artwork-merge-title">Merge Artwork</h3>
            <div className="gallery-merge-grid artwork-merge-grid">
              <section className="gallery-merge-column" role="region" aria-label="Source Artwork">
                <h4>Source</h4>
                {artworkMerge.isLoadingSource && !artworkMerge.sourceDetail ? (
                  <p className="muted-node">Loading source Artwork...</p>
                ) : null}
                {renderArtworkMergeReadOnlyProperties(
                  source,
                  artworkMerge.sourceDetail,
                  "Source artwork primary file thumbnail",
                )}
              </section>

              <section className="gallery-merge-column" role="region" aria-label="Merged Artwork">
                <h4>Merged</h4>
                {target ? (
                  <div className="property-grid gallery-merge-properties artwork-merge-properties">
                    {renderArtworkMergeThumbnail(target, "Merged artwork primary file thumbnail")}
                    <PropertyRow label="Artwork ID">
                      <input
                        value={
                          artworkMerge.targetDetail?.display_id ??
                          target.display_id ??
                          target.canonical_id
                        }
                        readOnly
                      />
                    </PropertyRow>
                    <PropertyRow label="Gallery">
                      <input
                        value={mergedArtworkGalleryNames(source, target).join(", ")}
                        readOnly
                      />
                    </PropertyRow>
                    <PropertyRow label="File count">
                      <input value={String(mergedFileCount)} readOnly />
                    </PropertyRow>
                    {renderArtworkMergeEditableProperties(artworkMerge)}
                  </div>
                ) : (
                  <p className="muted-node">Select a target Artwork to preview merged values.</p>
                )}
              </section>

              <section className="gallery-merge-column" role="region" aria-label="Target Artwork">
                <h4>Target</h4>
                <label className="property-row gallery-merge-target-row">
                  <span className="property-key">Target artwork</span>
                  <span className="property-value">
                    <select
                      aria-label="Target artwork"
                      value={artworkMerge.targetId}
                      onChange={(event) => updateArtworkMergeTarget(event.currentTarget.value)}
                      disabled={artworkMerge.isMerging}
                    >
                      <option value="">Select an Artwork</option>
                      {targetOptions.map((option) => (
                        <option key={option.id} value={option.id}>
                          {option.display_id ?? option.canonical_id} - {option.title}
                        </option>
                      ))}
                    </select>
                  </span>
                </label>
                {artworkMerge.isLoadingTarget ? (
                  <p className="muted-node">Loading target Artwork...</p>
                ) : target ? (
                  renderArtworkMergeReadOnlyProperties(
                    target,
                    artworkMerge.targetDetail,
                    "Target artwork primary file thumbnail",
                  )
                ) : (
                  <p className="muted-node">No target Artwork selected.</p>
                )}
              </section>
            </div>

            <div className="dialog-actions">
              <button
                type="submit"
                className="primary"
                disabled={!target || !artworkMerge.targetDetail || artworkMerge.isMerging}
              >
                Merge Artwork
              </button>
              <button
                type="button"
                disabled={artworkMerge.isMerging}
                onClick={() => setArtworkMerge(null)}
              >
                Cancel
              </button>
            </div>
          </form>
        </section>
      </div>
    );
  }

  function renderArtworkMergeReadOnlyProperties(
    artwork: ArtworkSummary,
    artworkDetail: ArtworkDetail | null,
    thumbnailAlt: string,
  ) {
    return (
      <div className="property-grid gallery-merge-properties artwork-merge-properties">
        {renderArtworkMergeThumbnail(artwork, thumbnailAlt)}
        <PropertyRow label="Artwork ID">
          <input
            value={artworkDetail?.display_id ?? artwork.display_id ?? artwork.canonical_id}
            readOnly
          />
        </PropertyRow>
        <PropertyRow label="Gallery">
          <input value={artwork.gallery_names.join(", ")} readOnly />
        </PropertyRow>
        <PropertyRow label="Title">
          <input value={artworkDetail?.title ?? artwork.title} readOnly />
        </PropertyRow>
        <PropertyBlock label="Description">
          <textarea value={artworkDetail?.description ?? ""} readOnly />
        </PropertyBlock>
        <PropertyRow label="For sale status">
          <input value={artworkDetail?.for_sale_status ?? ""} readOnly />
        </PropertyRow>
        <PropertyRow label="Media type">
          <input value={artworkDetail?.media ?? artwork.media ?? ""} readOnly />
        </PropertyRow>
        <PropertyRow label="Artwork type">
          <input value={artworkDetail?.format ?? artwork.format ?? ""} readOnly />
        </PropertyRow>
        <PropertyRow label="Publication status">
          <input
            value={publicationStatusLabel(artworkDetail?.publication_status_id ?? "")}
            readOnly
          />
        </PropertyRow>
        <PropertyRow label="Artist credits">
          <input
            value={formatArtistCreditList(artworkDetail?.artist_credits ?? artwork.artist_credits)}
            readOnly
          />
        </PropertyRow>
        <PropertyRow label="File count">
          <input
            value={String(
              artworkDetail ? artworkFileCountFromDetail(artworkDetail) : artwork.file_count,
            )}
            readOnly
          />
        </PropertyRow>
        <PropertyRow label="CAF URL">
          <input value={artworkDetail?.caf_url ?? ""} readOnly />
        </PropertyRow>
        <PropertyRow label="SNIKT URL">
          <input value={artworkDetail?.snikt_url ?? ""} readOnly />
        </PropertyRow>
        <PropertyRow label="Raremarq URL">
          <input value={artworkDetail?.raremarq_url ?? ""} readOnly />
        </PropertyRow>
        <PropertyRow label="Generic URL">
          <input value={artworkDetail?.generic_url ?? ""} readOnly />
        </PropertyRow>
      </div>
    );
  }

  function renderArtworkMergeEditableProperties(draft: ArtworkMergeDraft) {
    const mergeShowPrivateDataGroup = PRIVATE_DATA_PROPERTY_LABELS.some(showProperty);
    const form = draft.form;
    return (
      <>
        {renderFilteredProperty(
          "CAF URL",
          <PropertyRow label="CAF URL">
            <input value={form.cafUrl} onChange={updateArtworkMergeField("cafUrl")} />
          </PropertyRow>,
        )}
        {renderFilteredProperty(
          "SNIKT URL",
          <PropertyRow label="SNIKT URL">
            <input value={form.sniktUrl} onChange={updateArtworkMergeField("sniktUrl")} />
          </PropertyRow>,
        )}
        {renderFilteredProperty(
          "Raremarq URL",
          <PropertyRow label="Raremarq URL">
            <input value={form.raremarqUrl} onChange={updateArtworkMergeField("raremarqUrl")} />
          </PropertyRow>,
        )}
        {renderFilteredProperty(
          "Generic URL",
          <PropertyRow label="Generic URL">
            <input value={form.genericUrl} onChange={updateArtworkMergeField("genericUrl")} />
          </PropertyRow>,
        )}
        {renderFilteredProperty(
          "Title",
          <PropertyRow label="Title">
            <input value={form.title} onChange={updateArtworkMergeField("title")} />
          </PropertyRow>,
        )}
        {renderFilteredProperty(
          "Description",
          <PropertyBlock label="Description">
            <textarea value={form.description} onChange={updateArtworkMergeField("description")} />
          </PropertyBlock>,
        )}
        {renderFilteredProperty(
          "For sale status",
          <PropertyRow label="For sale status">
            <input value={form.forSaleStatus} onChange={updateArtworkMergeField("forSaleStatus")} />
          </PropertyRow>,
        )}
        {renderFilteredProperty(
          "Media type",
          <PropertyRow label="Media type">
            <select value={form.mediaTypeId} onChange={updateArtworkMergeField("mediaTypeId")}>
              {MEDIA_TYPE_OPTIONS.map((option) => (
                <option value={option.id} key={option.id}>
                  {option.label}
                </option>
              ))}
            </select>
          </PropertyRow>,
        )}
        {renderFilteredProperty(
          "Artwork type",
          <PropertyRow label="Artwork type">
            <select value={form.artTypeId} onChange={updateArtworkMergeField("artTypeId")}>
              {ART_TYPE_OPTIONS.map((option) => (
                <option value={option.id} key={option.id}>
                  {option.label}
                </option>
              ))}
            </select>
          </PropertyRow>,
        )}
        {renderFilteredProperty(
          "Publication status",
          <PropertyRow label="Publication status">
            <select
              value={form.publicationStatusId}
              onChange={updateArtworkMergeField("publicationStatusId")}
            >
              {PUBLICATION_STATUS_OPTIONS.map((option) => (
                <option value={option.id} key={option.id}>
                  {option.label}
                </option>
              ))}
            </select>
          </PropertyRow>,
        )}
        {renderFilteredProperty(
          "Artist credits",
          <div className="artist-credit-editor property-block" aria-label="Merged artist credits">
            <span className="property-block-label" title={propertyHelpForLabel("Artist credits")}>
              Artist credits
            </span>
            {form.artistCredits.map((credit, index) => (
              <div className="artist-credit-row" key={index}>
                {showProperty("Artist first name") ? (
                  <label title={propertyHelpForLabel("Artist first name")}>
                    {`Artist ${index + 1} first name`}
                    <input
                      value={credit.firstName}
                      onChange={updateArtworkMergeArtistCredit(index, "firstName")}
                    />
                  </label>
                ) : null}
                {showProperty("Artist last name") ? (
                  <label title={propertyHelpForLabel("Artist last name")}>
                    {`Artist ${index + 1} last name`}
                    <input
                      value={credit.lastName}
                      onChange={updateArtworkMergeArtistCredit(index, "lastName")}
                    />
                  </label>
                ) : null}
                {showProperty("Artist role") ? (
                  <label title={propertyHelpForLabel("Artist role")}>
                    {`Role ${index + 1}`}
                    <select
                      value={credit.roleId}
                      onChange={updateArtworkMergeArtistCredit(index, "roleId")}
                    >
                      <option value="">Select Role</option>
                      {ARTIST_ROLE_OPTIONS.map((role) => (
                        <option value={role.id} key={role.id}>
                          {role.label}
                        </option>
                      ))}
                    </select>
                  </label>
                ) : null}
                <button
                  type="button"
                  onClick={() => removeArtworkMergeArtistCredit(index)}
                  disabled={form.artistCredits.length === 1 && !artistCreditHasValue(credit)}
                >
                  Remove
                </button>
              </div>
            ))}
            <button type="button" onClick={addArtworkMergeArtistCredit}>
              Add artist
            </button>
          </div>,
        )}
        {renderFilteredProperty(
          "Flags",
          <div className="property-block property-flags" aria-label="Merged artwork flags">
            <span className="property-block-label" title={propertyHelpForLabel("Flags")}>
              Flags
            </span>
            <div className="flag-row">
              {showProperty("Active") ? (
                <label className="checkbox-field" title={propertyHelpForLabel("Active")}>
                  <input
                    type="checkbox"
                    checked={form.active}
                    onChange={updateArtworkMergeCheckbox("active")}
                  />
                  Active
                </label>
              ) : null}
              {showProperty("Illustration Exchange") ? (
                <label
                  className="checkbox-field"
                  title={propertyHelpForLabel("Illustration Exchange")}
                >
                  <input
                    type="checkbox"
                    checked={form.illustrationExchange}
                    onChange={updateArtworkMergeCheckbox("illustrationExchange")}
                  />
                  Illustration Exchange
                </label>
              ) : null}
              {showProperty("IX for sale") ? (
                <label className="checkbox-field" title={propertyHelpForLabel("IX for sale")}>
                  <input
                    type="checkbox"
                    checked={form.ixForSale}
                    onChange={updateArtworkMergeCheckbox("ixForSale")}
                  />
                  IX for sale
                </label>
              ) : null}
            </div>
          </div>,
        )}
        {showProperty("SNIKT extension fields") ? renderArtworkMergeSniktMetadataGroup(form) : null}
        {mergeShowPrivateDataGroup ? (
          <div className="property-block private-data-group" role="group" aria-label="Private Data">
            <span className="property-block-label" title={propertyHelpForLabel("Private Data")}>
              Private Data
            </span>
            {renderFilteredProperty(
              "Purchase price",
              <PropertyRow label="Purchase price">
                <input
                  value={form.purchasePrice}
                  onChange={updateArtworkMergeField("purchasePrice")}
                />
              </PropertyRow>,
            )}
            {renderFilteredProperty(
              "Estimated value",
              <PropertyRow label="Estimated value">
                <input
                  value={form.estimatedValue}
                  onChange={updateArtworkMergeField("estimatedValue")}
                />
              </PropertyRow>,
            )}
            {renderFilteredProperty(
              "Purchase date",
              <PropertyRow label="Purchase date">
                <input
                  type="date"
                  value={form.purchaseDate}
                  onChange={updateArtworkMergeField("purchaseDate")}
                />
              </PropertyRow>,
            )}
            {renderFilteredProperty(
              "Provenance",
              <PropertyBlock label="Provenance">
                <textarea
                  value={form.provenance}
                  onChange={updateArtworkMergeField("provenance")}
                />
              </PropertyBlock>,
            )}
            {renderFilteredProperty(
              "Personal notes",
              <PropertyBlock label="Personal notes">
                <textarea
                  value={form.personalNotes}
                  onChange={updateArtworkMergeField("personalNotes")}
                />
              </PropertyBlock>,
            )}
          </div>
        ) : null}
      </>
    );
  }

  function renderArtworkMergeSniktMetadataGroup(form: DetailForm) {
    const effectiveArtType = effectiveSniktArtType(form.sniktMetadata.artType, form.artTypeId);
    const renderSniktProperty = (label: SniktExtensionFieldLabel, node: ReactNode) =>
      sniktExtensionFieldVisible(label, effectiveArtType, {
        isForSale: form.sniktMetadata.isForSale,
      })
        ? renderFilteredProperty(label, node)
        : null;

    return (
      <div
        className="property-block snikt-upload-group"
        role="group"
        aria-label="SNIKT extension fields"
      >
        <span
          className="property-block-label"
          title={propertyHelpForLabel("SNIKT extension fields")}
        >
          SNIKT extension fields
        </span>
        <div className="snikt-upload-grid">
          {renderSniktProperty(
            "Art type",
            <PropertyRow label="Art type">
              <select
                value={form.sniktMetadata.artType}
                onChange={updateArtworkMergeSniktField("artType")}
              >
                <option value="">Use OAC artwork type</option>
                {SNIKT_ART_TYPE_OPTIONS.map((option) => (
                  <option value={option} key={option}>
                    {option}
                  </option>
                ))}
              </select>
            </PropertyRow>,
          )}
          {renderSniktProperty(
            "Publisher",
            <PropertyRow label="Publisher">
              <input
                value={form.sniktMetadata.comicPublisher}
                onChange={updateArtworkMergeSniktField("comicPublisher")}
              />
            </PropertyRow>,
          )}
          {renderSniktProperty(
            "Series title",
            <PropertyRow label="Series title">
              <input
                value={form.sniktMetadata.seriesTitle}
                onChange={updateArtworkMergeSniktField("seriesTitle")}
              />
            </PropertyRow>,
          )}
          {renderSniktProperty(
            "Issue number",
            <PropertyRow label="Issue number">
              <input
                value={form.sniktMetadata.issueNumber}
                onChange={updateArtworkMergeSniktField("issueNumber")}
              />
            </PropertyRow>,
          )}
          {renderSniktProperty(
            "Page number",
            <PropertyRow label="Page number">
              <input
                value={form.sniktMetadata.seriesPageNumber}
                onChange={updateArtworkMergeSniktField("seriesPageNumber")}
              />
            </PropertyRow>,
          )}
          {renderSniktProperty(
            "Year",
            <PropertyRow label="Year">
              <input
                value={form.sniktMetadata.year}
                onChange={updateArtworkMergeSniktField("year")}
              />
            </PropertyRow>,
          )}
          {renderSniktProperty(
            "Character",
            <PropertyRow label="Character">
              <input
                value={form.sniktMetadata.character}
                onChange={updateArtworkMergeSniktField("character")}
              />
            </PropertyRow>,
          )}
          {renderSniktProperty(
            "Animation subcategory",
            <PropertyRow label="Animation subcategory">
              <select
                value={form.sniktMetadata.subcategory}
                onChange={updateArtworkMergeSniktField("subcategory")}
              >
                <option value="">Select subcategory</option>
                {SNIKT_ANIMATION_SUBCATEGORY_OPTIONS.map((option) => (
                  <option value={option} key={option}>
                    {option}
                  </option>
                ))}
              </select>
            </PropertyRow>,
          )}
          {renderSniktProperty(
            "Animation studio",
            <PropertyRow label="Animation studio">
              <input
                value={form.sniktMetadata.animationStudio}
                onChange={updateArtworkMergeSniktField("animationStudio")}
              />
            </PropertyRow>,
          )}
          {renderSniktProperty(
            "Episode number",
            <PropertyRow label="Episode number">
              <input
                value={form.sniktMetadata.episodeNumber}
                onChange={updateArtworkMergeSniktField("episodeNumber")}
              />
            </PropertyRow>,
          )}
          {renderSniktProperty(
            "Episode title",
            <PropertyRow label="Episode title">
              <input
                value={form.sniktMetadata.episodeTitle}
                onChange={updateArtworkMergeSniktField("episodeTitle")}
              />
            </PropertyRow>,
          )}
          {renderSniktProperty(
            "Published date",
            <PropertyRow label="Published date">
              <input
                type="date"
                value={form.sniktMetadata.publishedDate}
                onChange={updateArtworkMergeSniktField("publishedDate")}
              />
            </PropertyRow>,
          )}
          {renderSniktProperty(
            "Strip title",
            <PropertyRow label="Strip title">
              <input
                value={form.sniktMetadata.stripTitle}
                onChange={updateArtworkMergeSniktField("stripTitle")}
              />
            </PropertyRow>,
          )}
          {renderSniktProperty(
            "Sunday strip",
            <PropertyRow label="Sunday strip">
              <input
                type="checkbox"
                checked={form.sniktMetadata.isSundayStrip}
                onChange={updateArtworkMergeSniktCheckbox("isSundayStrip")}
              />
            </PropertyRow>,
          )}
          {renderSniktProperty(
            "Other",
            <PropertyRow label="Other">
              <input
                value={form.sniktMetadata.other}
                onChange={updateArtworkMergeSniktField("other")}
              />
            </PropertyRow>,
          )}
          {renderSniktProperty(
            "Tags",
            <PropertyRow label="Tags">
              <input
                value={form.sniktMetadata.tags}
                onChange={updateArtworkMergeSniktField("tags")}
              />
            </PropertyRow>,
          )}
          {renderSniktProperty(
            "NSFW",
            <PropertyRow label="NSFW">
              <input
                type="checkbox"
                checked={form.sniktMetadata.isNsfw}
                onChange={updateArtworkMergeSniktCheckbox("isNsfw")}
              />
            </PropertyRow>,
          )}
          {renderSniktProperty(
            "For sale",
            <PropertyRow label="For sale">
              <input
                type="checkbox"
                checked={form.sniktMetadata.isForSale}
                onChange={updateArtworkMergeSniktCheckbox("isForSale")}
              />
            </PropertyRow>,
          )}
          {renderSniktProperty(
            "Sale price",
            <PropertyRow label="Sale price">
              <input
                value={form.sniktMetadata.price}
                onChange={updateArtworkMergeSniktField("price")}
              />
            </PropertyRow>,
          )}
          {renderSniktProperty(
            "Open to offers",
            <PropertyRow label="Open to offers">
              <input
                type="checkbox"
                checked={form.sniktMetadata.isOpenToOffers}
                onChange={updateArtworkMergeSniktCheckbox("isOpenToOffers")}
              />
            </PropertyRow>,
          )}
        </div>
      </div>
    );
  }

  function renderArtworkMergeThumbnail(artwork: ArtworkSummary, alt: string) {
    const url = thumbnailUrls[artwork.canonical_id] ?? "";
    return (
      <div className="artwork-merge-thumbnail">
        {url ? (
          <img src={url} alt={alt} />
        ) : (
          <span>{artwork.thumbnail_path ? "Loading thumbnail" : "No thumbnail"}</span>
        )}
      </div>
    );
  }

  function updateArtworkMergeForm(updater: (current: DetailForm) => DetailForm) {
    setArtworkMerge((current) => (current ? { ...current, form: updater(current.form) } : current));
  }

  function updateArtworkMergeField(
    field: keyof Omit<DetailForm, "artistCredits" | "sniktMetadata">,
  ) {
    return (
      event: React.ChangeEvent<HTMLInputElement | HTMLTextAreaElement | HTMLSelectElement>,
    ) => {
      const { value } = event.currentTarget;
      updateArtworkMergeForm((current) => ({ ...current, [field]: value }));
    };
  }

  function updateArtworkMergeSniktField(field: SniktMetadataTextField) {
    return (
      event: React.ChangeEvent<HTMLInputElement | HTMLTextAreaElement | HTMLSelectElement>,
    ) => {
      const { value } = event.currentTarget;
      updateArtworkMergeForm((current) => ({
        ...current,
        sniktMetadata: { ...current.sniktMetadata, [field]: value },
      }));
    };
  }

  function updateArtworkMergeCheckbox(field: "active" | "illustrationExchange" | "ixForSale") {
    return (event: React.ChangeEvent<HTMLInputElement>) => {
      const { checked } = event.currentTarget;
      updateArtworkMergeForm((current) => ({ ...current, [field]: checked }));
    };
  }

  function updateArtworkMergeSniktCheckbox(field: SniktMetadataBooleanField) {
    return (event: React.ChangeEvent<HTMLInputElement>) => {
      const { checked } = event.currentTarget;
      updateArtworkMergeForm((current) => ({
        ...current,
        sniktMetadata: { ...current.sniktMetadata, [field]: checked },
      }));
    };
  }

  function updateArtworkMergeArtistCredit(index: number, field: keyof ArtistCreditForm) {
    return (event: React.ChangeEvent<HTMLInputElement | HTMLSelectElement>) => {
      const { value } = event.currentTarget;
      updateArtworkMergeForm((current) => ({
        ...current,
        artistCredits: current.artistCredits.map((credit, creditIndex) =>
          creditIndex === index ? { ...credit, [field]: value } : credit,
        ),
      }));
    };
  }

  function addArtworkMergeArtistCredit() {
    updateArtworkMergeForm((current) => ({
      ...current,
      artistCredits: [...current.artistCredits, emptyArtistCredit()],
    }));
  }

  function removeArtworkMergeArtistCredit(index: number) {
    updateArtworkMergeForm((current) => {
      const selectedCredit = current.artistCredits[index];
      if (current.artistCredits.length === 1) {
        if (!artistCreditHasValue(selectedCredit)) return current;
        return { ...current, artistCredits: [emptyArtistCredit()] };
      }
      return {
        ...current,
        artistCredits: current.artistCredits.filter((_, creditIndex) => creditIndex !== index),
      };
    });
  }

  function renderExplorerContextMenu() {
    if (!explorerContextMenu) return null;
    const menuLabel = `${explorerItemTypeLabel(explorerContextMenu.item)} actions`;
    return (
      <div
        className="explorer-context-menu"
        role="menu"
        aria-label={menuLabel}
        style={{ left: explorerContextMenu.x, top: explorerContextMenu.y }}
        onMouseDown={(event) => event.stopPropagation()}
        onClick={(event) => event.stopPropagation()}
      >
        <button
          type="button"
          role="menuitem"
          onClick={() => startExplorerRename(explorerContextMenu.item)}
        >
          {renameExplorerItemLabel(explorerContextMenu.item)}
        </button>
        {canMergeGallery(explorerContextMenu.item) ? (
          <button
            type="button"
            role="menuitem"
            onClick={() => startGalleryMerge(explorerContextMenu.item)}
          >
            Merge Gallery with ...
          </button>
        ) : null}
        {canMergeArtwork(explorerContextMenu.item) ? (
          <button
            type="button"
            role="menuitem"
            onClick={() => void startArtworkMerge(explorerContextMenu.item)}
          >
            Merge Artwork with ...
          </button>
        ) : null}
        <button
          type="button"
          role="menuitem"
          className="danger-menuitem"
          onClick={() => void deleteExplorerItem(explorerContextMenu.item)}
        >
          {deleteExplorerItemLabel(explorerContextMenu.item)}
        </button>
      </div>
    );
  }

  function handleExplorerItemKeyDown(
    event: ReactKeyboardEvent<HTMLElement>,
    item: ExplorerContextItem,
  ) {
    if (event.key !== "F2") return;
    event.preventDefault();
    event.stopPropagation();
    startExplorerRename(item);
  }

  function openExplorerContextMenu(event: MouseEvent<HTMLElement>, item: ExplorerContextItem) {
    event.preventDefault();
    event.stopPropagation();
    setExplorerContextMenu({ x: event.clientX, y: event.clientY, item });
  }

  function startExplorerRename(item: ExplorerContextItem) {
    setExplorerContextMenu(null);
    setPendingRename({
      item,
      value: explorerRenameValue(item),
      isSaving: false,
    });
  }

  function startGalleryMerge(item: ExplorerContextItem) {
    if (item.type !== "gallery" || !workspace?.collection) return;
    setExplorerContextMenu(null);
    setGalleryMerge({
      source: item.gallery,
      targetId: "",
      name: "",
      cafGalleryRoomId: "",
      raremarqGalleryId: "",
      sniktGalleryInheritsCollection: true,
      isMerging: false,
    });
  }

  function canMergeGallery(item: ExplorerContextItem) {
    return (
      item.type === "gallery" &&
      Boolean(workspace?.collection) &&
      galleries.some((gallery) => gallery.id !== item.gallery.id)
    );
  }

  function canMergeArtwork(item: ExplorerContextItem) {
    return (
      item.type === "artwork" &&
      Boolean(workspace?.collection) &&
      artworks.some((artwork) => artwork.id !== item.artwork.id)
    );
  }

  async function startArtworkMerge(item: ExplorerContextItem) {
    if (item.type !== "artwork" || !workspace?.collection) return;
    setExplorerContextMenu(null);
    setArtworkMerge({
      source: item.artwork,
      sourceGalleryId: item.galleryId,
      sourceDetail: null,
      targetId: "",
      targetDetail: null,
      form: emptyForm,
      isLoadingSource: true,
      isLoadingTarget: false,
      isMerging: false,
    });
    try {
      const sourceDetail = await invoke<ArtworkDetail>("artwork_detail_command", {
        artworkId: item.artwork.id,
      });
      setArtworkMerge((current) => {
        if (!current || current.source.id !== item.artwork.id) return current;
        const form =
          current.targetDetail !== null
            ? formForArtworkMerge(sourceDetail, current.targetDetail)
            : current.form;
        return { ...current, sourceDetail, form, isLoadingSource: false };
      });
    } catch (caught) {
      setError(errorMessage(caught));
      setArtworkMerge((current) =>
        current && current.source.id === item.artwork.id
          ? { ...current, isLoadingSource: false }
          : current,
      );
    }
  }

  function updateGalleryMergeTarget(targetId: string) {
    setGalleryMerge((current) => {
      if (!current) return current;
      const target = galleries.find((gallery) => String(gallery.id) === targetId);
      if (!target) {
        return {
          ...current,
          targetId: "",
          name: "",
          cafGalleryRoomId: "",
          raremarqGalleryId: "",
          sniktGalleryInheritsCollection: true,
        };
      }
      return {
        ...current,
        targetId,
        name: target.name,
        cafGalleryRoomId: target.caf_gallery_room_id ?? current.source.caf_gallery_room_id ?? "",
        raremarqGalleryId: target.raremarq_gallery_id ?? current.source.raremarq_gallery_id ?? "",
        sniktGalleryInheritsCollection: target.snikt_gallery_inherits_collection,
      };
    });
  }

  function updateArtworkMergeTarget(targetId: string) {
    setArtworkMerge((current) => {
      if (!current) return current;
      return {
        ...current,
        targetId,
        targetDetail: null,
        form: targetId ? current.form : emptyForm,
        isLoadingTarget: Boolean(targetId),
      };
    });
    const target = artworks.find((artwork) => String(artwork.id) === targetId);
    if (!target) return;
    void loadArtworkMergeTarget(target.id);
  }

  async function loadArtworkMergeTarget(targetArtworkId: number) {
    try {
      const targetDetail = await invoke<ArtworkDetail>("artwork_detail_command", {
        artworkId: targetArtworkId,
      });
      setArtworkMerge((current) => {
        if (!current || current.targetId !== String(targetArtworkId)) return current;
        const form =
          current.sourceDetail !== null
            ? formForArtworkMerge(current.sourceDetail, targetDetail)
            : formFromDetail(targetDetail);
        return { ...current, targetDetail, form, isLoadingTarget: false };
      });
    } catch (caught) {
      setError(errorMessage(caught));
      setArtworkMerge((current) =>
        current && current.targetId === String(targetArtworkId)
          ? { ...current, isLoadingTarget: false }
          : current,
      );
    }
  }

  async function executeGalleryMerge(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!galleryMerge || !workspace?.collection || galleryMerge.isMerging) return;
    const targetGalleryId = Number(galleryMerge.targetId);
    const targetGallery = galleries.find((gallery) => gallery.id === targetGalleryId);
    if (!targetGallery) {
      setError("Choose a target Gallery before merging.");
      return;
    }
    const name = galleryMerge.name.trim();
    if (!name) {
      setError("Gallery name is required");
      return;
    }
    const request: MergeGalleryRequest = {
      collection_id: workspace.collection.id,
      source_gallery_id: galleryMerge.source.id,
      target_gallery_id: targetGallery.id,
      name,
      caf_gallery_room_id: blankToNull(galleryMerge.cafGalleryRoomId),
      raremarq_gallery_id: blankToNull(galleryMerge.raremarqGalleryId),
      snikt_gallery_inherits_collection: galleryMerge.sniktGalleryInheritsCollection,
    };
    setGalleryMerge({ ...galleryMerge, isMerging: true });
    try {
      setError("");
      const nextWorkspace = await invoke<WorkspaceState>("merge_gallery_command", { request });
      setWorkspace(nextWorkspace);
      setSelectedGalleryId(nextWorkspace.selected_gallery_id ?? targetGallery.id);
      setInspectorTarget({ type: "gallery", galleryId: targetGallery.id });
      clearSelectedArtwork();
      expandTreeNodes(["collection", treeKeyForGallery(targetGallery.id)]);
      setGalleryMerge(null);
      setStatus("Gallery merged");
    } catch (caught) {
      setError(errorMessage(caught));
      setGalleryMerge((current) => (current ? { ...current, isMerging: false } : current));
    }
  }

  async function executeArtworkMerge(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!artworkMerge || !workspace?.collection || artworkMerge.isMerging) return;
    const targetArtworkId = Number(artworkMerge.targetId);
    const targetArtwork = artworks.find((artwork) => artwork.id === targetArtworkId);
    if (!targetArtwork || !artworkMerge.targetDetail) {
      setError("Choose a target Artwork before merging.");
      return;
    }
    const title = artworkMerge.form.title.trim();
    if (!title) {
      setError("Artwork title is required");
      return;
    }
    const request: MergeArtworkRequest = {
      collection_id: workspace.collection.id,
      source_gallery_id: artworkMerge.sourceGalleryId,
      source_artwork_id: artworkMerge.source.id,
      target_artwork_id: targetArtwork.id,
      metadata: metadataRequestForForm(targetArtwork.id, { ...artworkMerge.form, title }),
    };
    setArtworkMerge({ ...artworkMerge, form: { ...artworkMerge.form, title }, isMerging: true });
    try {
      setError("");
      const nextWorkspace = await invoke<WorkspaceState>("merge_artwork_command", { request });
      setWorkspace(nextWorkspace);
      setSelectedGalleryId(artworkMerge.sourceGalleryId);
      setSelectedArtworkId(targetArtwork.id);
      setInspectorTarget({ type: "artwork", artworkId: targetArtwork.id });
      expandTreeNodes([
        "collection",
        treeKeyForGallery(artworkMerge.sourceGalleryId),
        treeKeyForArtwork(targetArtwork.id),
      ]);
      const nextDetail = await invoke<ArtworkDetail>("artwork_detail_command", {
        artworkId: targetArtwork.id,
      });
      const nextForm = setArtworkDetailFromSnapshot(nextDetail);
      markMetadataAutosaveBaseline(nextDetail.id, nextForm);
      setArtworkMerge(null);
      setStatus("Artwork merged");
    } catch (caught) {
      setError(errorMessage(caught));
      setArtworkMerge((current) => (current ? { ...current, isMerging: false } : current));
    }
  }

  async function commitPendingRename(committedValue?: string) {
    const rename = pendingRename;
    if (!rename || rename.isSaving || renameCommitInFlightRef.current) return;
    const value = (committedValue ?? rename.value).trim();
    if (!value) {
      setError(`${explorerItemTypeLabel(rename.item)} name is required`);
      return;
    }
    renameCommitInFlightRef.current = true;
    setPendingRename({ ...rename, value, isSaving: true });
    try {
      setError("");
      const outcome = await executeRenameItem(rename.item, value);
      if (outcome === "renamed_reload_workspace") {
        await loadWorkspace();
      }
      setPendingRename(null);
      if (outcome === "canceled") {
        setStatus(`${explorerItemTypeLabel(rename.item)} rename canceled`);
        return;
      }
      setStatus(`${explorerItemTypeLabel(rename.item)} renamed`);
    } catch (caught) {
      setError(errorMessage(caught));
      setPendingRename((current) => (current ? { ...current, isSaving: false } : current));
    } finally {
      renameCommitInFlightRef.current = false;
    }
  }

  async function executeRenameItem(
    item: ExplorerContextItem,
    value: string,
  ): Promise<RenameOutcome> {
    if (item.type === "collection") {
      await invoke("rename_collection_command", {
        collectionId: item.collection.id,
        name: value,
      });
      return "renamed_reload_workspace";
    }
    if (item.type === "gallery") {
      await invoke("rename_gallery_command", {
        galleryId: item.gallery.id,
        name: value,
      });
      return "renamed_reload_workspace";
    }
    if (item.type === "artwork") {
      const nextDetail = await invoke<ArtworkDetail>("rename_artwork_command", {
        artworkId: item.artwork.id,
        title: value,
      });
      if (detail?.id === item.artwork.id) {
        const nextForm = setArtworkDetailFromSnapshot(nextDetail);
        markMetadataAutosaveBaseline(nextDetail.id, nextForm);
      }
      return "renamed_reload_workspace";
    }
    const plan = await invoke<FileRenameResult["plan"]>("preview_rename_artwork_file_command", {
      request: { asset_kind: item.file.kind, asset_id: item.file.id, name: value },
    });
    if (plan.physical_file_rename) {
      const confirmed = await confirmDialog(
        `Rename this file on disk?\n\nFrom:\n${plan.current_path}\n\nTo:\n${plan.new_path}`,
        {
          title: "Rename File?",
          kind: "warning",
        },
      );
      if (!confirmed) {
        return "canceled";
      }
    }
    const renameRequest: FileRenameExecution = {
      plan,
      confirmed_physical_file_rename: plan.physical_file_rename,
    };
    const renameResult = await invoke<FileRenameResult>("execute_file_rename_command", {
      request: renameRequest,
    });
    if (detail?.id === item.artworkId) {
      applyArtworkDetailUpdate(renameResult.detail);
    }
    return "renamed";
  }

  function isRenamingExplorerItem(item: ExplorerContextItem) {
    return pendingRename ? explorerItemKey(pendingRename.item) === explorerItemKey(item) : false;
  }

  async function deleteExplorerItem(item: ExplorerContextItem) {
    setExplorerContextMenu(null);
    try {
      setError("");
      setStatus(`Preparing ${explorerItemTypeLabel(item).toLowerCase()} delete`);
      const preview = await loadDeletePreview(item);
      setPendingDelete({ item, preview, isDeleting: false });
      setStatus("Ready");
    } catch (caught) {
      setError(errorMessage(caught));
      setStatus("Ready");
    }
  }

  async function loadDeletePreview(item: ExplorerContextItem): Promise<DeletePreview> {
    if (item.type === "collection") {
      return invoke<DeletePreview>("preview_delete_collection_command", {
        collectionId: item.collection.id,
      });
    }
    if (item.type === "gallery") {
      return invoke<DeletePreview>("preview_delete_gallery_command", {
        request: { gallery_id: item.gallery.id, collection_id: item.collectionId },
      });
    }
    if (item.type === "artwork") {
      return invoke<DeletePreview>("preview_delete_artwork_command", {
        request: { artwork_id: item.artwork.id, gallery_id: item.galleryId },
      });
    }
    return invoke<DeletePreview>("preview_delete_artwork_file_command", {
      request: { asset_kind: item.file.kind, asset_id: item.file.id },
    });
  }

  async function confirmPendingDelete() {
    if (!pendingDelete) return;
    const item = pendingDelete.item;
    setPendingDelete({ ...pendingDelete, isDeleting: true });

    try {
      setError("");
      setStatus(`Deleting ${explorerItemTypeLabel(item).toLowerCase()}`);
      const outcome = await executeDeleteItem(item);
      if (outcome.result.trash_failures.length > 0) {
        if (outcome.detail && detail?.id === outcome.detail.id) {
          setArtworkDetailFromSnapshot(outcome.detail);
        }
        setPendingDelete(null);
        setTrashFailureReport({
          trashedFiles: outcome.result.trashed_files,
          failures: outcome.result.trash_failures,
        });
        setStatus(`${explorerItemTypeLabel(item)} delete blocked`);
        return;
      }
      const nextDetail = outcome.detail;
      if (nextDetail) {
        setWorkspace((current) => updateWorkspaceArtworkSummary(current, nextDetail));
        if (detail?.id === nextDetail.id) {
          setArtworkDetailFromSnapshot(nextDetail);
          setSelectedCarouselItemKey(null);
        }
      }
      if (item.type === "collection") {
        clearSelectedArtwork();
        setSelectedGalleryId(null);
        if (
          inspectorTarget?.type === "collection" &&
          inspectorTarget.collectionId === item.collection.id
        ) {
          setInspectorTarget(null);
        }
      } else if (item.type === "gallery") {
        if (selectedGalleryId === item.gallery.id) {
          setSelectedGalleryId(null);
          clearSelectedArtwork();
        }
        if (inspectorTarget?.type === "gallery" && inspectorTarget.galleryId === item.gallery.id) {
          setInspectorTarget(null);
        }
      } else if (item.type === "artwork") {
        if (selectedArtworkId === item.artwork.id) {
          clearSelectedArtwork();
        }
        if (inspectorTarget?.type === "artwork" && inspectorTarget.artworkId === item.artwork.id) {
          setInspectorTarget(null);
        }
      }
      if (item.type !== "file") {
        await loadWorkspace();
      }
      setPendingDelete(null);
      setStatus(`${explorerItemTypeLabel(item)} deleted`);
    } catch (caught) {
      setError(errorMessage(caught));
      setStatus("Ready");
      setPendingDelete((current) => (current ? { ...current, isDeleting: false } : current));
    }
  }

  async function executeDeleteItem(item: ExplorerContextItem): Promise<DeleteExecutionOutcome> {
    if (item.type === "collection") {
      return {
        result: await invoke<DeleteResult>("delete_collection_command", {
          collectionId: item.collection.id,
        }),
      };
    }
    if (item.type === "gallery") {
      return {
        result: await invoke<DeleteResult>("delete_gallery_command", {
          request: { gallery_id: item.gallery.id, collection_id: item.collectionId },
        }),
      };
    }
    if (item.type === "artwork") {
      return {
        result: await invoke<DeleteResult>("delete_artwork_command", {
          request: { artwork_id: item.artwork.id, gallery_id: item.galleryId },
        }),
      };
    }
    const response = await invoke<DeleteArtworkFileResult>("delete_artwork_file_command", {
      request: { asset_kind: item.file.kind, asset_id: item.file.id },
    });
    return { result: response.result, detail: response.detail };
  }

  function clearSelectedArtwork() {
    setSelectedArtworkId(null);
    setSelectedCarouselItemKey(null);
    clearArtworkDetail();
  }

  function artworksForGallery(gallery: GallerySummary) {
    return artworksByGalleryId.get(gallery.id) ?? [];
  }

  function renderSelectedImageDetails() {
    return (
      <FileDetailsPanel
        selectedCarouselItem={selectedCarouselItem}
        pngExportVariant={pngExportVariant}
        exportDestination={exportDestination}
        showPngExportControls={Boolean(detail && selectedRenderableSourceForPngExport)}
        canCreatePngExport={Boolean(
          detail &&
          selectedRenderableSourceForPngExport &&
          exportDestination.trim() &&
          !pngExportRunning,
        )}
        onImageRoleChange={saveSelectedImageRole}
        onCopyPath={copySelectedFilePath}
        onShowInExplorer={showSelectedImageInExplorer}
        onPngExportVariantChange={setPngExportVariant}
        onExportDestinationChange={(value) => {
          setExportDestinationIsAuto(false);
          setExportDestination(value);
        }}
        onCreatePngExport={createPngExport}
      />
    );
  }

  function renderSniktMetadataGroup() {
    const effectiveArtType = effectiveSniktArtType(form.sniktMetadata.artType, form.artTypeId);
    const renderSniktProperty = (label: SniktExtensionFieldLabel, node: ReactNode) =>
      sniktExtensionFieldVisible(label, effectiveArtType, {
        isForSale: form.sniktMetadata.isForSale,
      })
        ? renderFilteredProperty(label, node)
        : null;

    return (
      <div
        className="property-block snikt-upload-group"
        role="group"
        aria-label="SNIKT extension fields"
      >
        <span
          className="property-block-label"
          title={propertyHelpForLabel("SNIKT extension fields")}
        >
          SNIKT extension fields
        </span>
        <div className="snikt-upload-grid">
          {renderSniktProperty(
            "Art type",
            <PropertyRow label="Art type">
              <select value={form.sniktMetadata.artType} onChange={updateSniktField("artType")}>
                <option value="">Use OAC artwork type</option>
                {SNIKT_ART_TYPE_OPTIONS.map((option) => (
                  <option value={option} key={option}>
                    {option}
                  </option>
                ))}
              </select>
            </PropertyRow>,
          )}
          {renderSniktProperty(
            "Publisher",
            <PropertyRow label="Publisher">
              <input
                value={form.sniktMetadata.comicPublisher}
                onChange={updateSniktField("comicPublisher")}
              />
            </PropertyRow>,
          )}
          {renderSniktProperty(
            "Series title",
            <PropertyRow label="Series title">
              <input
                value={form.sniktMetadata.seriesTitle}
                onChange={updateSniktField("seriesTitle")}
              />
            </PropertyRow>,
          )}
          {renderSniktProperty(
            "Issue number",
            <PropertyRow label="Issue number">
              <input
                value={form.sniktMetadata.issueNumber}
                onChange={updateSniktField("issueNumber")}
              />
            </PropertyRow>,
          )}
          {renderSniktProperty(
            "Page number",
            <PropertyRow label="Page number">
              <input
                value={form.sniktMetadata.seriesPageNumber}
                onChange={updateSniktField("seriesPageNumber")}
              />
            </PropertyRow>,
          )}
          {renderSniktProperty(
            "Year",
            <PropertyRow label="Year">
              <input value={form.sniktMetadata.year} onChange={updateSniktField("year")} />
            </PropertyRow>,
          )}
          {renderSniktProperty(
            "Character",
            <PropertyRow label="Character">
              <input
                value={form.sniktMetadata.character}
                onChange={updateSniktField("character")}
              />
            </PropertyRow>,
          )}
          {renderSniktProperty(
            "Animation subcategory",
            <PropertyRow label="Animation subcategory">
              <select
                value={form.sniktMetadata.subcategory}
                onChange={updateSniktField("subcategory")}
              >
                <option value="">Select subcategory</option>
                {SNIKT_ANIMATION_SUBCATEGORY_OPTIONS.map((option) => (
                  <option value={option} key={option}>
                    {option}
                  </option>
                ))}
              </select>
            </PropertyRow>,
          )}
          {renderSniktProperty(
            "Animation studio",
            <PropertyRow label="Animation studio">
              <input
                value={form.sniktMetadata.animationStudio}
                onChange={updateSniktField("animationStudio")}
              />
            </PropertyRow>,
          )}
          {renderSniktProperty(
            "Episode number",
            <PropertyRow label="Episode number">
              <input
                value={form.sniktMetadata.episodeNumber}
                onChange={updateSniktField("episodeNumber")}
              />
            </PropertyRow>,
          )}
          {renderSniktProperty(
            "Episode title",
            <PropertyRow label="Episode title">
              <input
                value={form.sniktMetadata.episodeTitle}
                onChange={updateSniktField("episodeTitle")}
              />
            </PropertyRow>,
          )}
          {renderSniktProperty(
            "Published date",
            <PropertyRow label="Published date">
              <input
                type="date"
                value={form.sniktMetadata.publishedDate}
                onChange={updateSniktField("publishedDate")}
              />
            </PropertyRow>,
          )}
          {renderSniktProperty(
            "Strip title",
            <PropertyRow label="Strip title">
              <input
                value={form.sniktMetadata.stripTitle}
                onChange={updateSniktField("stripTitle")}
              />
            </PropertyRow>,
          )}
          {renderSniktProperty(
            "Sunday strip",
            <PropertyRow label="Sunday strip">
              <input
                type="checkbox"
                checked={form.sniktMetadata.isSundayStrip}
                onChange={updateSniktCheckbox("isSundayStrip")}
              />
            </PropertyRow>,
          )}
          {renderSniktProperty(
            "Other",
            <PropertyRow label="Other">
              <input value={form.sniktMetadata.other} onChange={updateSniktField("other")} />
            </PropertyRow>,
          )}
          {renderSniktProperty(
            "Tags",
            <PropertyRow label="Tags">
              <input value={form.sniktMetadata.tags} onChange={updateSniktField("tags")} />
            </PropertyRow>,
          )}
          {renderSniktProperty(
            "NSFW",
            <PropertyRow label="NSFW">
              <input
                type="checkbox"
                checked={form.sniktMetadata.isNsfw}
                onChange={updateSniktCheckbox("isNsfw")}
              />
            </PropertyRow>,
          )}
          {renderSniktProperty(
            "For sale",
            <PropertyRow label="For sale">
              <input
                type="checkbox"
                checked={form.sniktMetadata.isForSale}
                onChange={updateSniktCheckbox("isForSale")}
              />
            </PropertyRow>,
          )}
          {renderSniktProperty(
            "Sale price",
            <PropertyRow label="Sale price">
              <input value={form.sniktMetadata.price} onChange={updateSniktField("price")} />
            </PropertyRow>,
          )}
          {renderSniktProperty(
            "Open to offers",
            <PropertyRow label="Open to offers">
              <input
                type="checkbox"
                checked={form.sniktMetadata.isOpenToOffers}
                onChange={updateSniktCheckbox("isOpenToOffers")}
              />
            </PropertyRow>,
          )}
        </div>
      </div>
    );
  }

  function updateField(field: keyof Omit<DetailForm, "artistCredits" | "sniktMetadata">) {
    return (
      event: React.ChangeEvent<HTMLInputElement | HTMLTextAreaElement | HTMLSelectElement>,
    ) => {
      const { value } = event.currentTarget;
      setForm((current) => ({ ...current, [field]: value }));
    };
  }

  function updateSniktField(field: SniktMetadataTextField) {
    return (
      event: React.ChangeEvent<HTMLInputElement | HTMLTextAreaElement | HTMLSelectElement>,
    ) => {
      const { value } = event.currentTarget;
      setForm((current) => ({
        ...current,
        sniktMetadata: { ...current.sniktMetadata, [field]: value },
      }));
    };
  }

  function updateCheckbox(field: "active" | "illustrationExchange" | "ixForSale") {
    return (event: React.ChangeEvent<HTMLInputElement>) => {
      const { checked } = event.currentTarget;
      setForm((current) => ({ ...current, [field]: checked }));
    };
  }

  function updateSniktCheckbox(field: SniktMetadataBooleanField) {
    return (event: React.ChangeEvent<HTMLInputElement>) => {
      const { checked } = event.currentTarget;
      setForm((current) => ({
        ...current,
        sniktMetadata: { ...current.sniktMetadata, [field]: checked },
      }));
    };
  }

  function updateArtistCredit(index: number, field: keyof ArtistCreditForm) {
    return (event: React.ChangeEvent<HTMLInputElement | HTMLSelectElement>) => {
      const { value } = event.currentTarget;
      setForm((current) => ({
        ...current,
        artistCredits: current.artistCredits.map((credit, creditIndex) =>
          creditIndex === index ? { ...credit, [field]: value } : credit,
        ),
      }));
    };
  }

  function addArtistCredit() {
    setForm((current) => ({
      ...current,
      artistCredits: [...current.artistCredits, emptyArtistCredit()],
    }));
  }

  function removeArtistCredit(index: number) {
    setForm((current) => {
      const selectedCredit = current.artistCredits[index];
      if (current.artistCredits.length === 1) {
        if (!artistCreditHasValue(selectedCredit)) return current;
        return {
          ...current,
          artistCredits: [emptyArtistCredit()],
        };
      }
      return {
        ...current,
        artistCredits: current.artistCredits.filter((_, creditIndex) => creditIndex !== index),
      };
    });
  }
}

function emptyArtistCredit(): ArtistCreditForm {
  return { firstName: "", lastName: "", roleId: "" };
}

function formForArtworkMerge(sourceDetail: ArtworkDetail, targetDetail: ArtworkDetail): DetailForm {
  const sourceForm = formFromDetail(sourceDetail);
  const targetForm = formFromDetail(targetDetail);
  return {
    ...targetForm,
    artistCredits: mergeArtistCreditForms(targetForm.artistCredits, sourceForm.artistCredits),
  };
}

function mergeArtistCreditForms(
  targetCredits: ArtistCreditForm[],
  sourceCredits: ArtistCreditForm[],
) {
  const merged: ArtistCreditForm[] = [];
  const seen = new Set<string>();
  for (const credit of [...targetCredits, ...sourceCredits]) {
    if (!artistCreditHasValue(credit)) continue;
    const key = `${credit.firstName.trim().toLowerCase()}\n${credit.lastName
      .trim()
      .toLowerCase()}\n${credit.roleId.trim().toLowerCase()}`;
    if (seen.has(key)) continue;
    seen.add(key);
    merged.push(credit);
  }
  return merged.length > 0 ? merged : [emptyArtistCredit()];
}

function artworkFileCountFromDetail(detail: ArtworkDetail): number {
  return (
    detail.file_assets.length +
    detail.derived_assets.filter((asset) => asset.derivative_type === "png_export").length
  );
}

function mergedArtworkGalleryNames(source: ArtworkSummary, target: ArtworkSummary): string[] {
  const names: string[] = [];
  for (const name of [...target.gallery_names, ...source.gallery_names]) {
    if (!names.includes(name)) names.push(name);
  }
  return names;
}

function formatArtistCreditList(credits: ArtworkSummary["artist_credits"]) {
  if (credits.length === 0) return "";
  return credits.map((credit) => [credit.name, credit.role].filter(Boolean).join(" - ")).join(", ");
}

function publicationStatusLabel(publicationStatusId: string) {
  if (!publicationStatusId.trim()) return "";
  return (
    PUBLICATION_STATUS_OPTIONS.find((option) => option.id === publicationStatusId)?.label ??
    publicationStatusId
  );
}

function artistCreditHasValue(credit?: ArtistCreditForm): boolean {
  return Boolean(credit?.firstName.trim() || credit?.lastName.trim() || credit?.roleId.trim());
}

function explorerItemTypeLabel(item: ExplorerContextItem): string {
  if (item.type === "collection") return "Collection";
  if (item.type === "gallery") return "Gallery";
  if (item.type === "artwork") return "Artwork";
  return "File";
}

function explorerItemKey(item: ExplorerContextItem): string {
  if (item.type === "collection") return `collection:${item.collection.id}`;
  if (item.type === "gallery") return `gallery:${item.gallery.id}`;
  if (item.type === "artwork") return `artwork:${item.artwork.id}`;
  return `${item.file.kind}:${item.file.id}`;
}

function explorerRenameValue(item: ExplorerContextItem): string {
  if (item.type === "collection") return item.collection.name;
  if (item.type === "gallery") return item.gallery.name;
  if (item.type === "artwork") return item.artwork.title;
  return item.file.name;
}

function renameExplorerItemLabel(item: ExplorerContextItem): string {
  return `Rename ${explorerItemTypeLabel(item)}`;
}

function deleteExplorerItemLabel(item: ExplorerContextItem): string {
  return `Delete ${explorerItemTypeLabel(item)}`;
}

function isArtworkIdLabelPreference(value: unknown): value is ArtworkIdLabelPreference {
  return value === "oac" || value === "caf" || value === "snikt" || value === "raremarq";
}

function normalizeAppPreferences(value: unknown, fallbackRoot: string): AppPreferences {
  const candidate = isRecord(value) ? value : {};
  return {
    default_attach_mode: isAttachMode(candidate["default_attach_mode"])
      ? candidate["default_attach_mode"]
      : DEFAULT_APP_PREFERENCES.default_attach_mode,
    default_png_export_variant:
      normalizePngExportVariant(candidate["default_png_export_variant"]) ??
      DEFAULT_APP_PREFERENCES.default_png_export_variant,
    default_provider_focus: isDefaultProviderFocus(candidate["default_provider_focus"])
      ? candidate["default_provider_focus"]
      : DEFAULT_APP_PREFERENCES.default_provider_focus,
    artwork_id_label_preference: isArtworkIdLabelPreference(
      candidate["artwork_id_label_preference"],
    )
      ? candidate["artwork_id_label_preference"]
      : DEFAULT_APP_PREFERENCES.artwork_id_label_preference,
    theme: isThemePreference(candidate["theme"])
      ? candidate["theme"]
      : DEFAULT_APP_PREFERENCES.theme,
    startup_behavior: isStartupBehaviorPreference(candidate["startup_behavior"])
      ? candidate["startup_behavior"]
      : DEFAULT_APP_PREFERENCES.startup_behavior,
    default_workspace_root:
      typeof candidate["default_workspace_root"] === "string" &&
      candidate["default_workspace_root"].trim()
        ? candidate["default_workspace_root"]
        : fallbackRoot,
    raremarq_csv_export_scope: isRaremarqCsvExportScope(candidate["raremarq_csv_export_scope"])
      ? candidate["raremarq_csv_export_scope"]
      : DEFAULT_APP_PREFERENCES.raremarq_csv_export_scope,
    raremarq_csv_url_mode: isRaremarqCsvUrlMode(candidate["raremarq_csv_url_mode"])
      ? candidate["raremarq_csv_url_mode"]
      : DEFAULT_APP_PREFERENCES.raremarq_csv_url_mode,
  };
}

function propertySourceFiltersForFocus(focus: DefaultProviderFocus): PropertySourceFilters {
  if (focus === "all") return DEFAULT_PROPERTY_SOURCE_FILTERS;
  return {
    caf: focus === "caf",
    snikt: focus === "snikt",
    raremarq: focus === "raremarq",
  };
}

function themeStateFromPreference(preference: ThemePreference): "dark" | "light" {
  return preference === "alucard" ? "light" : "dark";
}

function startupVisibleWorkspace(
  workspace: WorkspaceState,
  startupBehavior?: StartupBehaviorPreference,
): WorkspaceState {
  if (startupBehavior === "show_start_window" || startupBehavior === "start_empty") {
    return emptyWorkspaceState();
  }
  return workspace;
}

function isWorkspaceState(value: unknown): value is WorkspaceState {
  return (
    isRecord(value) &&
    Array.isArray(value["galleries"]) &&
    Array.isArray(value["artworks"]) &&
    typeof value["mode"] === "string"
  );
}

function isAttachMode(value: unknown): value is AttachMode {
  return value === "copy" || value === "link";
}

function normalizePngExportVariant(value: unknown): PngExportVariant | null {
  if (value === "basic" || value === "premium") return value;
  if (value === "caf_basic") return "basic";
  if (value === "caf_premium") return "premium";
  return null;
}

function isDefaultProviderFocus(value: unknown): value is DefaultProviderFocus {
  return value === "all" || value === "caf" || value === "snikt" || value === "raremarq";
}

function isThemePreference(value: unknown): value is ThemePreference {
  return value === "dracula" || value === "alucard";
}

function isStartupBehaviorPreference(value: unknown): value is StartupBehaviorPreference {
  return value === "reopen_last" || value === "show_start_window" || value === "start_empty";
}

function isRaremarqCsvExportScope(value: unknown): value is RaremarqCsvExportScope {
  return value === "all" || value === "untracked";
}

function isRaremarqCsvUrlMode(value: unknown): value is RaremarqCsvUrlMode {
  return value === "generic_url" || value === "blank" || value === "tmpfiles";
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

function getOptionalCurrentWebview(): CurrentWebview | null {
  try {
    return getCurrentWebview();
  } catch {
    return null;
  }
}

function errorMessage(caught: unknown): string {
  if (isTauriIpcUnavailable(caught)) {
    return "Desktop app integration is unavailable in this browser preview. Run OA Curator through the Tauri desktop app to use this command.";
  }
  return String(caught);
}

function isCafCollectionIdMismatchError(message: string): boolean {
  return message.includes("is already linked to CAF Collection");
}

function isTauriIpcUnavailable(caught: unknown): boolean {
  const message = String(caught);
  return (
    message.includes("Cannot read properties of undefined (reading 'invoke')") ||
    message.includes("__TAURI_INTERNALS__")
  );
}

function allowUiUpdate(): Promise<void> {
  return new Promise((resolve) => {
    window.setTimeout(resolve, 0);
  });
}

function scrollElementToTop(element: HTMLElement | null) {
  if (!element) return;
  element.scrollTop = 0;
  if (typeof element.scrollTo === "function") {
    element.scrollTo({ behavior: "auto", left: 0, top: 0 });
  }
}

function updateWorkspaceArtworkSummary(
  workspace: WorkspaceState | null,
  detail: ArtworkDetail,
): WorkspaceState | null {
  if (!workspace) return workspace;
  return {
    ...workspace,
    artworks: workspace.artworks.map((artwork) =>
      artwork.id === detail.id ? updateArtworkSummaryFromDetail(artwork, detail) : artwork,
    ),
  };
}

function updateArtworkSummaryFromDetail(
  artwork: ArtworkSummary,
  detail: ArtworkDetail,
): ArtworkSummary {
  return {
    ...artwork,
    canonical_id: detail.canonical_id,
    display_id: detail.display_id ?? detail.canonical_id,
    caf_artwork_id: detail.caf_artwork_id ?? null,
    snikt_artwork_id: detail.snikt_artwork_id ?? null,
    raremarq_artwork_id: detail.raremarq_artwork_id ?? null,
    title: detail.title,
    media: detail.media ?? null,
    format: detail.format ?? null,
    source_folder: detail.source_folder,
    thumbnail_path: thumbnailPathFromDetail(detail),
    file_count:
      detail.file_assets.length +
      detail.derived_assets.filter((asset) => asset.derivative_type === "png_export").length,
    artist_credits: detail.artist_credits,
  };
}

function thumbnailPathFromDetail(detail: ArtworkDetail): string | null {
  for (const fileAsset of detail.file_assets) {
    const thumbnail = detail.derived_assets.find(
      (asset) =>
        asset.derivative_type === "thumbnail" && asset.source_file_asset_id === fileAsset.id,
    );
    if (thumbnail) return thumbnail.path;
  }
  return detail.derived_assets.find((asset) => asset.derivative_type === "thumbnail")?.path ?? null;
}

function emptyWorkspaceState(): WorkspaceState {
  return {
    mode: "empty",
    collection: null,
    galleries: [],
    artworks: [],
    selected_gallery_id: null,
  };
}

function formatRecentCollectionDate(value: string) {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return "";
  return new Intl.DateTimeFormat(undefined, {
    month: "short",
    day: "numeric",
    year: "numeric",
    hour: "numeric",
    minute: "2-digit",
  }).format(date);
}

function updateDialogTitle(dialog: UpdateDialogState) {
  if (dialog.state === "checking") return "Checking for Updates";
  if (dialog.state === "none") return "OA Curator Is Up To Date";
  if (dialog.state === "error") return "Update Check Failed";
  return dialog.state === "installing" ? "Installing Update" : "Update Available";
}

function formatUpdateDate(value: string) {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return new Intl.DateTimeFormat(undefined, {
    month: "short",
    day: "numeric",
    year: "numeric",
  }).format(date);
}

function formatUpdateProgress(progress: AppUpdateProgress | null) {
  if (!progress) return "Preparing download";
  if (!progress.total) return `${formatByteCount(progress.downloaded)} downloaded`;
  return `${formatByteCount(progress.downloaded)} of ${formatByteCount(progress.total)} downloaded`;
}

function formatByteCount(value: number) {
  if (value < 1024) return `${value} B`;
  const kib = value / 1024;
  if (kib < 1024) return `${kib.toFixed(1)} KB`;
  return `${(kib / 1024).toFixed(1)} MB`;
}

function HelpPage({ page, onClose }: { page: "about" | "licensing"; onClose: () => void }) {
  const title = page === "about" ? "About OA Curator" : "Licensing";
  const closeButtonRef = useRef<HTMLButtonElement | null>(null);

  useEffect(() => {
    closeButtonRef.current?.focus();
  }, []);

  return (
    <div className="help-page-backdrop">
      <section
        className="help-page"
        role="dialog"
        aria-modal="true"
        aria-label={title}
        onKeyDown={(event) => {
          if (event.key === "Escape") {
            event.preventDefault();
            onClose();
          }
        }}
      >
        <header>
          <h2>{title}</h2>
          <button type="button" aria-label="Close help page" ref={closeButtonRef} onClick={onClose}>
            Close
          </button>
        </header>
        {page === "about" ? (
          <div className="help-page-body">
            <div className="about-lede">
              <img className="about-logo" src="/oac-logo-app.svg" alt="OA Curator logo" />
              <p>
                OA Curator is Original Art Curator, a local-first desktop app for collectors who
                manage original art scans, metadata, galleries, and web-ready export prep on their
                own computer.
              </p>
            </div>
            <dl>
              <div>
                <dt>Version</dt>
                <dd>0.1.0 public beta</dd>
              </div>
              <div>
                <dt>Publisher</dt>
                <dd>Remgrandt Works</dd>
              </div>
            </dl>
          </div>
        ) : (
          <div className="help-page-body">
            <p>
              Project attributions for bundled visual, theme, and UI resources are tracked in
              ATTRIBUTIONS.md.
            </p>
            <dl>
              <div>
                <dt>Dracula Theme</dt>
                <dd>Dracula and Alucard palette references, MIT License.</dd>
              </div>
              <div>
                <dt>Allotment</dt>
                <dd>Workbench split pane layout dependency, MIT License.</dd>
              </div>
              <div>
                <dt>Lucide icons</dt>
                <dd>
                  Collection, Gallery, Artwork, file, external-link, and upload-prefill command
                  icons, ISC License.
                </dd>
              </div>
            </dl>
          </div>
        )}
      </section>
    </div>
  );
}

function PropertySourceFilterBar({
  filters,
  onToggle,
}: {
  filters: PropertySourceFilters;
  onToggle: (source: PropertySource) => void;
}) {
  return (
    <div className="property-source-filter" aria-label="Property source filters">
      {PROPERTY_SOURCE_OPTIONS.map((source) => (
        <button
          type="button"
          key={source.key}
          className={filters[source.key] ? "active" : ""}
          aria-pressed={filters[source.key]}
          title={`Show fields compatible with ${source.label}`}
          onClick={() => onToggle(source.key)}
        >
          {source.label}
        </button>
      ))}
    </div>
  );
}

function PropertyRow({ label, children }: { label: string; children: ReactNode }) {
  const helpText = propertyHelpForLabel(label);

  return (
    <label className="property-row">
      <span className="property-key" title={helpText}>
        {label}
      </span>
      <span className="property-value">{children}</span>
    </label>
  );
}

function UrlPropertyRow({
  label,
  value,
  onChange,
  onOpen,
  extraAction,
}: {
  label: string;
  value: string;
  onChange: (event: React.ChangeEvent<HTMLInputElement>) => void;
  onOpen: () => void;
  extraAction?: {
    iconName: string;
    label: string;
    title: string;
    disabled?: boolean;
    onClick: () => void;
  };
}) {
  const inputId = propertyInputId(label);
  const hasUrl = value.trim().length > 0;
  const helpText = propertyHelpForLabel(label);

  return (
    <div className="property-row">
      <label className="property-key" htmlFor={inputId} title={helpText}>
        {label}
      </label>
      <span className="property-value url-field-row">
        <input id={inputId} value={value} onChange={onChange} />
        <button
          type="button"
          className="property-icon-button"
          aria-label={`Open ${label} in browser`}
          title={`Open ${label} in browser`}
          disabled={!hasUrl}
          onClick={onOpen}
        >
          <ToolbarIcon name="external-link" />
        </button>
        {extraAction ? (
          <button
            type="button"
            className="property-icon-button property-export-button"
            aria-label={extraAction.label}
            title={extraAction.title}
            disabled={extraAction.disabled}
            onClick={extraAction.onClick}
          >
            <ToolbarIcon name={extraAction.iconName} />
            <span>{extraAction.label}</span>
          </button>
        ) : null}
      </span>
    </div>
  );
}

function PropertyBlock({ label, children }: { label: string; children: ReactNode }) {
  const helpText = propertyHelpForLabel(label);

  return (
    <label className="property-block">
      <span className="property-block-label" title={helpText}>
        {label}
      </span>
      {children}
    </label>
  );
}

function propertyInputId(label: string) {
  return `property-${label
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-|-$/g, "")}`;
}

export default WorkbenchApp;
