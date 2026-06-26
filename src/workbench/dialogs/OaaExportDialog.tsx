import type { OaaExportProgress, OaaExportReport } from "../../domain/types";

export type OaaExportWizardState = {
  archivePath: string;
  includeImages: boolean;
  includePrivateMetadata: boolean;
  isRunning: boolean;
  progress: OaaExportProgress | null;
  report: OaaExportReport | null;
};

type OaaExportDialogProps = {
  wizard: OaaExportWizardState;
  onChange: (wizard: OaaExportWizardState) => void;
  onBrowse: () => void;
  onSubmit: () => void;
  onClose: () => void;
};

export function OaaExportDialog({
  wizard,
  onChange,
  onBrowse,
  onSubmit,
  onClose,
}: OaaExportDialogProps) {
  const canClose = !wizard.isRunning;
  const exportDisabled = wizard.isRunning || !wizard.archivePath.trim();

  return (
    <div className="workspace-command-backdrop">
      <section
        className="workspace-command workspace-command-modal oaa-export-modal"
        role="dialog"
        aria-modal="true"
        aria-labelledby="oaa-export-title"
        onKeyDown={(event) => {
          if (event.key === "Escape" && canClose) {
            event.preventDefault();
            onClose();
          }
        }}
      >
        <h3 id="oaa-export-title">Export OAA Archive</h3>
        <form
          onSubmit={(event) => {
            event.preventDefault();
            onSubmit();
          }}
        >
          <label>
            OAA archive path
            <span className="workspace-command-file-row">
              <input
                value={wizard.archivePath}
                disabled={wizard.isRunning}
                onChange={(event) =>
                  onChange({
                    ...wizard,
                    archivePath: event.currentTarget.value,
                    report: null,
                  })
                }
              />
              <button type="button" disabled={wizard.isRunning} onClick={onBrowse}>
                Browse
              </button>
            </span>
          </label>

          <fieldset className="raremarq-export-fieldset">
            <legend>Archive contents</legend>
            <label className="radio-row">
              <input
                type="checkbox"
                checked={wizard.includeImages}
                disabled={wizard.isRunning}
                onChange={(event) =>
                  onChange({
                    ...wizard,
                    includeImages: event.currentTarget.checked,
                    report: null,
                  })
                }
              />
              Include artwork files in the archive
            </label>
            <p className="workspace-command-note">
              When enabled, linked files are copied into the OAA archive without changing the local
              Collection. When disabled, OAC exports metadata and Gallery membership only.
            </p>
            <label className="radio-row">
              <input
                type="checkbox"
                checked={wizard.includePrivateMetadata}
                disabled={wizard.isRunning}
                onChange={(event) =>
                  onChange({
                    ...wizard,
                    includePrivateMetadata: event.currentTarget.checked,
                    report: null,
                  })
                }
              />
              Include private collector metadata
            </label>
            <p className="workspace-command-note">
              Turn this on only for private backups. Private metadata includes purchase, value,
              provenance, and personal note fields.
            </p>
          </fieldset>

          {wizard.progress && (
            <div className="raremarq-export-progress">
              <span>{wizard.progress.message}</span>
              <progress
                aria-label="OAA export progress"
                max={Math.max(wizard.progress.total, 1)}
                value={wizard.progress.current}
              />
            </div>
          )}

          {wizard.report && (
            <div className="workspace-command-note">
              <p>OAA archive finished writing.</p>
            </div>
          )}

          <div className="button-row">
            <button type="submit" className="primary" disabled={exportDisabled}>
              Export OAA
            </button>
            <button type="button" disabled={wizard.isRunning} onClick={onClose}>
              {wizard.report ? "Close" : "Cancel"}
            </button>
          </div>
        </form>
      </section>
    </div>
  );
}
