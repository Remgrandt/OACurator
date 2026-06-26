import { raremarqExportPlanScope } from "../../domain/reportSummaries";
import type {
  RaremarqCsvExportPlan,
  RaremarqCsvExportProgress,
  RaremarqCsvExportReport,
  RaremarqCsvExportScope,
  RaremarqCsvUrlMode,
} from "../../domain/types";

export type RaremarqExportWizardState = {
  plan: RaremarqCsvExportPlan;
  csvPath: string;
  scope: RaremarqCsvExportScope;
  urlMode: RaremarqCsvUrlMode;
  isRunning: boolean;
  progress: RaremarqCsvExportProgress | null;
  report: RaremarqCsvExportReport | null;
};

type RaremarqExportDialogProps = {
  wizard: RaremarqExportWizardState;
  onChange: (wizard: RaremarqExportWizardState) => void;
  onBrowse: () => void;
  onSubmit: () => void;
  onClose: () => void;
};

export function RaremarqExportDialog({
  wizard,
  onChange,
  onBrowse,
  onSubmit,
  onClose,
}: RaremarqExportDialogProps) {
  const selectedPlan = raremarqExportPlanScope(wizard);
  const duplicateWarning = wizard.plan.raremarq_tracked_artworks;
  const genericBlankCount = selectedPlan.generic_url_blank_count;
  const blankCount = selectedPlan.blank_url_count;
  const tmpfilesLargeCount = selectedPlan.tmpfiles_large_file_count;
  const tmpfilesMissingCount = selectedPlan.tmpfiles_missing_file_count;
  const tmpfilesUnrenderableCount = selectedPlan.tmpfiles_unrenderable_file_count;
  const tmpfilesBlockedCount = tmpfilesMissingCount + tmpfilesUnrenderableCount;
  const canClose = !wizard.isRunning;
  const exportDisabled =
    wizard.isRunning ||
    !wizard.csvPath.trim() ||
    (wizard.urlMode === "tmpfiles" && tmpfilesBlockedCount > 0);

  return (
    <div className="workspace-command-backdrop">
      <section
        className="workspace-command workspace-command-modal raremarq-export-modal"
        role="dialog"
        aria-modal="true"
        aria-labelledby="raremarq-export-title"
        onKeyDown={(event) => {
          if (event.key === "Escape" && canClose) {
            event.preventDefault();
            onClose();
          }
        }}
      >
        <h3 id="raremarq-export-title">Export to Raremarq</h3>
        <form
          onSubmit={(event) => {
            event.preventDefault();
            onSubmit();
          }}
        >
          <label>
            CSV path
            <span className="workspace-command-file-row">
              <input
                value={wizard.csvPath}
                disabled={wizard.isRunning}
                onChange={(event) =>
                  onChange({ ...wizard, csvPath: event.currentTarget.value, report: null })
                }
              />
              <button type="button" disabled={wizard.isRunning} onClick={onBrowse}>
                Browse
              </button>
            </span>
          </label>

          <fieldset className="raremarq-export-fieldset">
            <legend>Items</legend>
            <label className="radio-row">
              <input
                type="radio"
                name="raremarq-export-scope"
                checked={wizard.scope === "all"}
                disabled={wizard.isRunning}
                onChange={() => onChange({ ...wizard, scope: "all", report: null })}
              />
              Export all items in the Collection
            </label>
            {duplicateWarning > 0 && (
              <p className="workspace-command-note">
                {pluralize(duplicateWarning, "item")} already has a Raremarq URL and can create a
                duplicate if exported again.
              </p>
            )}
            <label className="radio-row">
              <input
                type="radio"
                name="raremarq-export-scope"
                checked={wizard.scope === "untracked"}
                disabled={wizard.isRunning}
                onChange={() => onChange({ ...wizard, scope: "untracked", report: null })}
              />
              Export only items without a Raremarq URL
            </label>
          </fieldset>

          <fieldset className="raremarq-export-fieldset">
            <legend>URL field</legend>
            <label className="radio-row">
              <input
                type="radio"
                name="raremarq-export-url-mode"
                checked={wizard.urlMode === "generic_url"}
                disabled={wizard.isRunning}
                onChange={() => onChange({ ...wizard, urlMode: "generic_url", report: null })}
              />
              Use Generic URL
            </label>
            <p className="workspace-command-note">
              {pluralize(genericBlankCount, "entry", "entries")} will still have a blank URL field.
              Raremarq bulk upload will fail unless those rows are fixed manually before upload.
            </p>
            <label className="radio-row">
              <input
                type="radio"
                name="raremarq-export-url-mode"
                checked={wizard.urlMode === "blank"}
                disabled={wizard.isRunning}
                onChange={() => onChange({ ...wizard, urlMode: "blank", report: null })}
              />
              Leave URL fields blank
            </label>
            <p className="workspace-command-note">
              {pluralize(blankCount, "entry", "entries")} will have blank URL fields. Every row must
              be fixed manually for Raremarq bulk upload to work.
            </p>
            <label className="radio-row">
              <input
                type="radio"
                name="raremarq-export-url-mode"
                checked={wizard.urlMode === "tmpfiles"}
                disabled={wizard.isRunning}
                onChange={() => onChange({ ...wizard, urlMode: "tmpfiles", report: null })}
              />
              Upload temporary image copies
            </label>
            <p className="workspace-command-note">
              OAC will upload obfuscated copies to tmpfiles.org with a 24 hour expiry, verify each
              URL is live, and then write those URLs into the CSV.
            </p>
            {tmpfilesLargeCount > 0 && (
              <p className="workspace-command-note">
                {pluralize(tmpfilesLargeCount, "image")} over 20 MB will be downsized before upload.
              </p>
            )}
            {tmpfilesMissingCount > 0 && (
              <p className="workspace-command-note">
                {pluralize(tmpfilesMissingCount, "entry", "entries")} cannot be uploaded because no
                primary file is attached.
              </p>
            )}
            {tmpfilesUnrenderableCount > 0 && (
              <p className="workspace-command-note">
                {pluralize(tmpfilesUnrenderableCount, "entry", "entries")} cannot be uploaded
                because the primary file is not a supported image.
              </p>
            )}
          </fieldset>

          {wizard.progress && (
            <div className="raremarq-export-progress">
              <span>{wizard.progress.message}</span>
              <progress
                aria-label="Raremarq export progress"
                max={Math.max(wizard.progress.total, 1)}
                value={wizard.progress.current}
              />
            </div>
          )}

          {wizard.report && (
            <div className="workspace-command-note">
              <p>CSV finished writing.</p>
              {wizard.report.tmpfiles_uploaded > 0 && (
                <p>{pluralize(wizard.report.tmpfiles_uploaded, "temporary URL")} uploaded.</p>
              )}
            </div>
          )}

          <div className="button-row">
            <button type="submit" className="primary" disabled={exportDisabled}>
              Export CSV
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

function pluralize(count: number, singular: string, plural = `${singular}s`) {
  return `${count} ${count === 1 ? singular : plural}`;
}
