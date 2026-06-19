// Copyright (c) 2026 Remgrandt Works. All rights reserved.

import type { ChangeEvent, FocusEvent, RefObject } from "react";
import {
  workspaceCommandPlaceholder,
  workspaceCommandSourceFilePlaceholder,
} from "../../domain/formatters";
import type { WorkspaceCommandMode } from "../../domain/types";

type WorkspaceCommandDialogProps = {
  command: WorkspaceCommandMode;
  commandLabel: string;
  importMessage: string;
  needsPath: boolean;
  initialFocusRef: RefObject<HTMLInputElement | null>;
  name: string;
  path: string;
  cafId: string;
  sniktId: string;
  raremarqId: string;
  sniktGalleryInheritsCollection: boolean;
  sourceFilePath: string;
  submitDisabled: boolean;
  submitLabel: string;
  onSubmit: () => void;
  onCancel: () => void;
  onNameChange: (event: ChangeEvent<HTMLInputElement>) => void;
  onPathChange: (event: ChangeEvent<HTMLInputElement>) => void;
  onPathFocus: (event: FocusEvent<HTMLInputElement>) => void;
  onCafIdChange: (value: string) => void;
  onSniktIdChange: (value: string) => void;
  onRaremarqIdChange: (value: string) => void;
  onSniktGalleryInheritsCollectionChange: (value: boolean) => void;
  onSourceFilePathChange: (value: string) => void;
  onBrowseSourceFile: () => void;
};

export function WorkspaceCommandDialog({
  command,
  commandLabel,
  importMessage,
  needsPath,
  initialFocusRef,
  name,
  path,
  cafId,
  sniktId,
  raremarqId,
  sniktGalleryInheritsCollection,
  sourceFilePath,
  submitDisabled,
  submitLabel,
  onSubmit,
  onCancel,
  onNameChange,
  onPathChange,
  onPathFocus,
  onCafIdChange,
  onSniktIdChange,
  onRaremarqIdChange,
  onSniktGalleryInheritsCollectionChange,
  onSourceFilePathChange,
  onBrowseSourceFile,
}: WorkspaceCommandDialogProps) {
  const isImportCommand = isImportWorkspaceCommand(command);
  const isSourceFileCommand = isSourceFileWorkspaceCommand(command);

  return (
    <div className="workspace-command-backdrop">
      <section
        className="workspace-command workspace-command-modal"
        role="dialog"
        aria-modal="true"
        aria-labelledby="workspace-command-title"
        onKeyDown={(event) => {
          if (event.key === "Escape") {
            event.preventDefault();
            onCancel();
          }
        }}
      >
        <h3 id="workspace-command-title">{commandLabel}</h3>
        <form
          onSubmit={(event) => {
            event.preventDefault();
            onSubmit();
          }}
        >
          {(command === "new_collection" || command === "new_gallery") && (
            <label>
              Name
              <input ref={initialFocusRef} value={name} onChange={onNameChange} />
            </label>
          )}
          {importMessage && <p className="workspace-command-note">{importMessage}</p>}
          {needsPath && (
            <label>
              {isImportCommand ? "Destination folder" : "Manifest path"}
              <input
                ref={isImportCommand ? initialFocusRef : undefined}
                value={path}
                onChange={onPathChange}
                onFocus={onPathFocus}
                placeholder={workspaceCommandPlaceholder(command)}
              />
            </label>
          )}
          {command === "new_collection" && (
            <>
              <label>
                CAF Collection ID (GCat)
                <input
                  value={cafId}
                  onChange={(event) => onCafIdChange(event.currentTarget.value)}
                />
              </label>
              <label>
                SNIKT Collection ID
                <input
                  value={sniktId}
                  onChange={(event) => onSniktIdChange(event.currentTarget.value)}
                />
              </label>
              <label>
                Raremarq Collection ID
                <input
                  value={raremarqId}
                  onChange={(event) => onRaremarqIdChange(event.currentTarget.value)}
                />
              </label>
            </>
          )}
          {isSourceFileCommand && (
            <label>
              {workspaceCommandSourceFileLabel(command)}
              <span className="workspace-command-file-row">
                <input
                  ref={needsPath ? undefined : initialFocusRef}
                  value={sourceFilePath}
                  onChange={(event) => onSourceFilePathChange(event.currentTarget.value)}
                  placeholder={workspaceCommandSourceFilePlaceholder(command)}
                />
                <button type="button" onClick={onBrowseSourceFile}>
                  Browse
                </button>
              </span>
            </label>
          )}
          {command === "new_gallery" && (
            <>
              <label>
                CAF Gallery Room ID (GSub)
                <input
                  value={cafId}
                  onChange={(event) => onCafIdChange(event.currentTarget.value)}
                />
              </label>
              <label>
                Raremarq Gallery ID
                <input
                  value={raremarqId}
                  onChange={(event) => onRaremarqIdChange(event.currentTarget.value)}
                />
              </label>
              <label className="checkbox-field">
                <input
                  type="checkbox"
                  checked={sniktGalleryInheritsCollection}
                  onChange={(event) =>
                    onSniktGalleryInheritsCollectionChange(event.currentTarget.checked)
                  }
                />
                Inherit SNIKT Collection ID
              </label>
            </>
          )}
          <div className="button-row">
            <button type="submit" className="primary" disabled={submitDisabled}>
              {submitLabel}
            </button>
            <button type="button" onClick={onCancel}>
              Cancel
            </button>
          </div>
        </form>
      </section>
    </div>
  );
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

function isSourceFileWorkspaceCommand(
  command: WorkspaceCommandMode | null,
): command is "import_caf_collection" | "import_oaa_archive" | "import_snikt_collection" {
  return isImportWorkspaceCommand(command);
}

function workspaceCommandSourceFileLabel(command: WorkspaceCommandMode) {
  if (command === "import_oaa_archive") return "OAA archive";
  if (command === "import_snikt_collection") return "SNIKT.com CSV file";
  return "CAF CSV file";
}
