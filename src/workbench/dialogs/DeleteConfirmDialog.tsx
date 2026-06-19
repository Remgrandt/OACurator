// Copyright (c) 2026 Remgrandt Works. All rights reserved.

import type { KeyboardEvent } from "react";
import type { DeleteFilePreview, DeletePreview, DeleteTrashFailure } from "../../domain/types";

type DeleteConfirmDialogProps = {
  itemLabel: string;
  preview: DeletePreview;
  isDeleting: boolean;
  onConfirm: () => void;
  onCancel: () => void;
};

type TrashFailureDialogProps = {
  failures: DeleteTrashFailure[];
  trashedFiles: DeleteFilePreview[];
  onClose: () => void;
};

export function DeleteConfirmDialog({
  itemLabel,
  preview,
  isDeleting,
  onConfirm,
  onCancel,
}: DeleteConfirmDialogProps) {
  const files = preview.files_to_trash;
  const fileCountLabel = files.length === 1 ? "file" : "files";

  function handleKeyDown(event: KeyboardEvent<HTMLElement>) {
    if (event.key === "Escape" && !isDeleting) {
      event.preventDefault();
      onCancel();
    }
  }

  return (
    <div className="workspace-command-backdrop">
      <section
        className="workspace-command workspace-command-modal delete-confirm-modal"
        role="dialog"
        aria-modal="true"
        aria-label={`Delete ${itemLabel}`}
        onKeyDown={handleKeyDown}
      >
        <h3>Delete {itemLabel}</h3>
        <p>This will unlink the {itemLabel} from OA Curator.</p>
        {files.length > 0 ? (
          <>
            <p>
              The following OAC-managed {fileCountLabel} will be moved to the Recycle Bin or Trash:
            </p>
            <ul className="delete-file-list">
              {files.map((file) => (
                <li key={file.path}>
                  <strong>{file.label}</strong>
                  <span>{file.reason}</span>
                  <code>{file.path}</code>
                </li>
              ))}
            </ul>
          </>
        ) : (
          <p>
            Linked source files stay on disk. No OAC-managed image files are scheduled for disk
            removal.
          </p>
        )}
        <div className="button-row">
          <button
            type="button"
            className="primary danger-button"
            disabled={isDeleting}
            onClick={onConfirm}
          >
            Move to Recycle Bin and Delete
          </button>
          <button type="button" disabled={isDeleting} onClick={onCancel}>
            Cancel
          </button>
        </div>
      </section>
    </div>
  );
}

export function TrashFailureDialog({ failures, trashedFiles, onClose }: TrashFailureDialogProps) {
  function handleKeyDown(event: KeyboardEvent<HTMLElement>) {
    if (event.key === "Escape") {
      event.preventDefault();
      onClose();
    }
  }

  return (
    <div className="workspace-command-backdrop">
      <section
        className="workspace-command workspace-command-modal delete-confirm-modal"
        role="dialog"
        aria-modal="true"
        aria-label="Files Still On Disk"
        onKeyDown={handleKeyDown}
      >
        <h3>Files Still On Disk</h3>
        <p>
          OA Curator kept the catalog records because these files could not be moved to the Recycle
          Bin or Trash.
        </p>
        <ul className="delete-file-list">
          {failures.map((failure) => (
            <li key={failure.path}>
              <code>{failure.path}</code>
              <span>{failure.error}</span>
            </li>
          ))}
        </ul>
        {trashedFiles.length > 0 && (
          <>
            <p>The following files were moved before the failure was reported:</p>
            <ul className="delete-file-list">
              {trashedFiles.map((file) => (
                <li key={file.path}>
                  <strong>{file.label}</strong>
                  <span>{file.path}</span>
                </li>
              ))}
            </ul>
          </>
        )}
        <div className="button-row">
          <button type="button" className="primary" onClick={onClose}>
            OK
          </button>
        </div>
      </section>
    </div>
  );
}
