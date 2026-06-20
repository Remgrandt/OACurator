// Copyright (c) 2026 Remgrandt Works. All rights reserved.

import { useEffect, useState, type MouseEvent } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";

type CommandBarProps = {
  theme: "dark" | "light";
  onNewCollection: () => void;
  onOpenCollection: () => void;
  onCloseCollection: () => void;
  onImportCafCollection: () => void;
  onImportOaaArchive: () => void;
  onExportOaaArchive: () => void;
  onExportRaremarqCsv: () => void;
  onImportSniktCollection: () => void;
  importCafCollectionLabel?: string;
  importSniktCollectionLabel?: string;
  onNewGallery: () => void;
  onNewArtwork: () => void;
  onShowUserGuide: () => void;
  onShowAbout: () => void;
  onShowLicensing: () => void;
  onShowPreferences: () => void;
  onCheckForUpdates?: () => void;
  onToggleTheme: () => void;
  canCloseCollection: boolean;
  canCreateGallery: boolean;
  canCreateArtwork: boolean;
  canExportOaaArchive: boolean;
  canExportRaremarqCsv: boolean;
};

type OpenMenu = "file" | "help" | null;
type WindowAction = "minimize" | "toggleMaximize" | "close";
type CurrentWindow = ReturnType<typeof getCurrentWindow>;

const assetToolbarIcons = new Set([
  "collection-new",
  "collection-open",
  "gallery-new",
  "gallery-open",
  "artwork-new",
  "artwork-open",
  "file-new",
  "external-link",
  "cloud-upload",
  "move-left",
  "move-right",
  "layers-plus",
  "layers-minus",
]);

export function CommandBar({
  theme,
  onNewCollection,
  onOpenCollection,
  onCloseCollection,
  onImportCafCollection,
  onImportOaaArchive,
  onExportOaaArchive,
  onExportRaremarqCsv,
  onImportSniktCollection,
  importCafCollectionLabel = "Import CAF Collection",
  importSniktCollectionLabel = "Import SNIKT.com Collection",
  onNewGallery,
  onNewArtwork,
  onShowUserGuide,
  onShowAbout,
  onShowLicensing,
  onShowPreferences,
  onCheckForUpdates = () => {},
  onToggleTheme,
  canCloseCollection,
  canCreateGallery,
  canCreateArtwork,
  canExportOaaArchive,
  canExportRaremarqCsv,
}: CommandBarProps) {
  const [openMenu, setOpenMenu] = useState<OpenMenu>(null);
  const appWindow = getOptionalCurrentWindow();

  useEffect(() => {
    if (!openMenu) return;

    function closeMenuOnEscape(event: KeyboardEvent) {
      if (event.key === "Escape") {
        setOpenMenu(null);
      }
    }

    window.addEventListener("keydown", closeMenuOnEscape);
    return () => window.removeEventListener("keydown", closeMenuOnEscape);
  }, [openMenu]);

  function runMenuAction(action: () => void) {
    setOpenMenu(null);
    action();
  }

  function runWindowAction(action: WindowAction) {
    if (!appWindow) return;
    void Promise.resolve(appWindow[action]()).catch(() => {});
  }

  function handleTitlebarMouseDown(event: MouseEvent<HTMLElement>) {
    if (event.button !== 0) return;
    if (event.detail > 1) {
      event.preventDefault();
      if (event.detail === 2) {
        runWindowAction("toggleMaximize");
      }
      return;
    }
    if (!appWindow) return;
    void Promise.resolve(appWindow.startDragging()).catch(() => {});
  }

  return (
    <>
      <header className="app-titlebar" role="banner" aria-label="Application Title Bar">
        <div className="titlebar-brand" onMouseDown={handleTitlebarMouseDown}>
          <img
            className="menu-logo"
            src={theme === "dark" ? "/oac-logo-dark-mode.svg" : "/oac-logo-light-mode.svg"}
            alt=""
          />
        </div>
        <nav className="menu-bar" role="menubar" aria-label="Application Menu">
          <div className="menu-group">
            <button
              type="button"
              role="menuitem"
              aria-haspopup="menu"
              aria-expanded={openMenu === "file"}
              onClick={() => setOpenMenu((current) => (current === "file" ? null : "file"))}
            >
              File
            </button>
            {openMenu === "file" && (
              <div className="menu-popover" role="menu" aria-label="File menu">
                <button
                  type="button"
                  role="menuitem"
                  onClick={() => runMenuAction(onNewCollection)}
                >
                  New Collection
                </button>
                <button
                  type="button"
                  role="menuitem"
                  onClick={() => runMenuAction(onOpenCollection)}
                >
                  Open Collection
                </button>
                {canCloseCollection && (
                  <button
                    type="button"
                    role="menuitem"
                    onClick={() => runMenuAction(onCloseCollection)}
                  >
                    Close Collection
                  </button>
                )}
                <div className="menu-separator" role="separator" />
                <button
                  type="button"
                  role="menuitem"
                  onClick={() => runMenuAction(onImportCafCollection)}
                >
                  {importCafCollectionLabel}
                </button>
                <button
                  type="button"
                  role="menuitem"
                  onClick={() => runMenuAction(onImportSniktCollection)}
                >
                  {importSniktCollectionLabel}
                </button>
                <button
                  type="button"
                  role="menuitem"
                  onClick={() => runMenuAction(onImportOaaArchive)}
                >
                  Import OAA Archive
                </button>
                <div className="menu-separator" role="separator" />
                <button
                  type="button"
                  role="menuitem"
                  disabled={!canExportRaremarqCsv}
                  onClick={() => runMenuAction(onExportRaremarqCsv)}
                >
                  Export to Raremarq
                </button>
                <button
                  type="button"
                  role="menuitem"
                  disabled={!canExportOaaArchive}
                  onClick={() => runMenuAction(onExportOaaArchive)}
                >
                  Export OAA Archive
                </button>
                <div className="menu-separator" role="separator" />
                <button
                  type="button"
                  role="menuitem"
                  disabled={!canCreateGallery}
                  onClick={() => runMenuAction(onNewGallery)}
                >
                  New Gallery
                </button>
                <button
                  type="button"
                  role="menuitem"
                  disabled={!canCreateArtwork}
                  onClick={() => runMenuAction(onNewArtwork)}
                >
                  New Artwork
                </button>
              </div>
            )}
          </div>
          <div className="menu-group">
            <button type="button" role="menuitem" onClick={() => runMenuAction(onShowPreferences)}>
              Preferences
            </button>
          </div>
          <div className="menu-group">
            <button
              type="button"
              role="menuitem"
              aria-haspopup="menu"
              aria-expanded={openMenu === "help"}
              onClick={() => setOpenMenu((current) => (current === "help" ? null : "help"))}
            >
              Help
            </button>
            {openMenu === "help" && (
              <div className="menu-popover" role="menu" aria-label="Help menu">
                <button
                  type="button"
                  role="menuitem"
                  onClick={() => runMenuAction(onShowUserGuide)}
                >
                  User Guide
                </button>
                <button
                  type="button"
                  role="menuitem"
                  onClick={() => runMenuAction(onCheckForUpdates)}
                >
                  Check for Updates
                </button>
                <button type="button" role="menuitem" onClick={() => runMenuAction(onShowAbout)}>
                  About OA Curator
                </button>
                <button
                  type="button"
                  role="menuitem"
                  onClick={() => runMenuAction(onShowLicensing)}
                >
                  Licensing
                </button>
              </div>
            )}
          </div>
        </nav>
        <div className="titlebar-drag-region" onMouseDown={handleTitlebarMouseDown}>
          <span className="titlebar-title">OA Curator</span>
        </div>
        <div className="window-controls" aria-label="Window controls">
          <WindowControlButton
            label="Minimize window"
            icon="minimize"
            onClick={() => runWindowAction("minimize")}
          />
          <WindowControlButton
            label="Maximize or restore window"
            icon="maximize"
            onClick={() => runWindowAction("toggleMaximize")}
          />
          <WindowControlButton
            label="Close window"
            icon="close"
            danger
            onClick={() => runWindowAction("close")}
          />
        </div>
      </header>
      <div className="command-bar" role="toolbar" aria-label="Application Toolbar">
        <IconButton label="New Collection" icon="collection-new" onClick={onNewCollection} />
        <IconButton label="Open Collection" icon="collection-open" onClick={onOpenCollection} />
        <span className="toolbar-separator" aria-hidden="true" />
        <IconButton
          label="New Gallery"
          icon="gallery-new"
          disabled={!canCreateGallery}
          onClick={onNewGallery}
        />
        <IconButton
          label="New Artwork"
          icon="artwork-new"
          disabled={!canCreateArtwork}
          onClick={onNewArtwork}
        />
        <span className="toolbar-spacer" aria-hidden="true" />
        <IconButton
          label={theme === "dark" ? "Switch to light mode" : "Switch to dark mode"}
          icon={theme === "dark" ? "theme-dark" : "theme-light"}
          pressed={theme === "light"}
          extraClassName="theme-toggle"
          onClick={onToggleTheme}
        />
      </div>
    </>
  );
}

function getOptionalCurrentWindow(): CurrentWindow | null {
  try {
    return getCurrentWindow();
  } catch {
    return null;
  }
}

function WindowControlButton({
  label,
  icon,
  danger,
  onClick,
}: {
  label: string;
  icon: "minimize" | "maximize" | "close";
  danger?: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      className={danger ? "window-control window-control-danger" : "window-control"}
      aria-label={label}
      title={label}
      onClick={onClick}
    >
      <WindowControlIcon name={icon} />
    </button>
  );
}

function WindowControlIcon({ name }: { name: "minimize" | "maximize" | "close" }) {
  const common = {
    fill: "none",
    stroke: "currentColor",
    strokeWidth: 1.8,
    strokeLinecap: "round" as const,
  };
  if (name === "minimize") {
    return (
      <svg viewBox="0 0 24 24" aria-hidden="true">
        <path {...common} d="M6 15h12" />
      </svg>
    );
  }
  if (name === "maximize") {
    return (
      <svg viewBox="0 0 24 24" aria-hidden="true">
        <path {...common} d="M7 7h10v10H7z" />
      </svg>
    );
  }
  return (
    <svg viewBox="0 0 24 24" aria-hidden="true">
      <path {...common} d="M7 7l10 10M17 7 7 17" />
    </svg>
  );
}

function IconButton({
  label,
  icon,
  disabled,
  pressed,
  extraClassName,
  onClick,
}: {
  label: string;
  icon: string;
  disabled?: boolean;
  pressed?: boolean;
  extraClassName?: string;
  onClick: () => void;
}) {
  const className = extraClassName ? `icon-button ${extraClassName}` : "icon-button";

  return (
    <button
      type="button"
      className={className}
      aria-label={label}
      aria-pressed={pressed}
      title={label}
      disabled={disabled}
      onClick={onClick}
    >
      <ToolbarIcon name={icon} />
    </button>
  );
}

export function ToolbarIcon({ name }: { name: string }) {
  const common = {
    fill: "none",
    stroke: "currentColor",
    strokeWidth: 1.8,
    strokeLinecap: "round" as const,
  };
  if (assetToolbarIcons.has(name)) {
    return <span className={`svg-icon icon-${name}`} aria-hidden="true" />;
  }

  if (name.endsWith("open")) {
    return (
      <svg viewBox="0 0 24 24" aria-hidden="true">
        <path {...common} d="M3.5 7.5h6l2 2h9v8.5a2 2 0 0 1-2 2h-13a2 2 0 0 1-2-2z" />
        <path {...common} d="M3.5 7.5v-2a2 2 0 0 1 2-2h3l2 2h4" />
      </svg>
    );
  }
  if (name === "theme-light") {
    return (
      <svg viewBox="0 0 24 24" aria-hidden="true">
        <circle {...common} cx="12" cy="12" r="4" />
        <path
          {...common}
          d="M12 2.5v2M12 19.5v2M4.6 4.6 6 6M18 18l1.4 1.4M2.5 12h2M19.5 12h2M4.6 19.4 6 18M18 6l1.4-1.4"
        />
      </svg>
    );
  }
  if (name === "theme-dark") {
    return (
      <svg viewBox="0 0 24 24" aria-hidden="true">
        <path {...common} d="M20 14.4A7.2 7.2 0 0 1 9.6 4a8.2 8.2 0 1 0 10.4 10.4z" />
      </svg>
    );
  }
  if (name === "tree-collapse-all") {
    return (
      <svg viewBox="0 0 24 24" aria-hidden="true">
        <path {...common} d="M5 7h14M5 17h14" />
        <path {...common} d="m8 14 4-4 4 4" />
      </svg>
    );
  }
  if (name === "tree-expand-all") {
    return (
      <svg viewBox="0 0 24 24" aria-hidden="true">
        <path {...common} d="M5 7h14M5 17h14" />
        <path {...common} d="m8 10 4 4 4-4" />
      </svg>
    );
  }
  if (name === "artwork-new") {
    return (
      <svg viewBox="0 0 24 24" aria-hidden="true">
        <path {...common} d="M5 6h10v10H5z" />
        <path {...common} d="m7 14 3-4 2 3 1-1 2 2" />
        <path {...common} d="M18 12v7M14.5 15.5h7" />
      </svg>
    );
  }
  return (
    <svg viewBox="0 0 24 24" aria-hidden="true">
      <path {...common} d="M4 6h13v12H4z" />
      <path {...common} d="M17 10h3M18.5 8.5v3" />
    </svg>
  );
}
