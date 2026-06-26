import type {
  CafImportReport,
  OaaExportReport,
  OaaImportReport,
  RaremarqCsvExportPlan,
  RaremarqCsvExportPlanScope,
  RaremarqCsvExportReport,
  RaremarqCsvExportScope,
  SniktImportReport,
} from "./types";

export function cafImportReportSummary(report: CafImportReport) {
  return `Imported CAF Collection ${report.caf_collection_id}: ${report.galleries_imported} galleries, ${report.artworks_imported} artworks`;
}

export function oaaImportReportSummary(report: OaaImportReport) {
  return `Imported OAA archive: ${report.galleries_imported} galleries, ${report.artworks_imported} artworks, ${report.files_imported} files`;
}

export function oaaExportReportSummary(report: OaaExportReport) {
  return `Exported OAA archive: ${report.galleries_exported} galleries, ${report.artworks_exported} artworks, ${report.files_exported} files`;
}

export function cafImportScreenMessages(report: CafImportReport) {
  const detailMessages = report.messages.filter(isUserFacingCafImportWarning);
  if (detailMessages.length === 0) return [];
  if (report.debug_log_path) {
    return [
      `CAF CSV import finished with ${pluralize(detailMessages.length, "warning")}. See the detailed import log: ${report.debug_log_path}`,
    ];
  }
  return [
    `CAF CSV import finished with ${pluralize(detailMessages.length, "warning")}. The detailed import log was not available.`,
  ];
}

function isUserFacingCafImportWarning(message: string) {
  if (message.startsWith("CAF CSV import log: ")) return false;
  if (message.startsWith("CAF CSV does not include a Collection name;")) return false;
  if (message.startsWith("CAF CSV does not include Gallery Room names;")) return false;
  return true;
}

export function raremarqCsvExportReportSummary(report: RaremarqCsvExportReport) {
  const missing =
    report.rows_missing_primary_image_url === 1
      ? "1 missing URL"
      : `${report.rows_missing_primary_image_url} missing URLs`;
  return `Exported Raremarq CSV: ${report.rows_exported} artworks, ${missing}`;
}

export function raremarqExportPlanScope(wizard: {
  plan: RaremarqCsvExportPlan;
  scope: RaremarqCsvExportScope;
}): RaremarqCsvExportPlanScope {
  return wizard.scope === "all" ? wizard.plan.all : wizard.plan.untracked;
}

export function pluralize(count: number, singular: string, plural = `${singular}s`) {
  return `${count} ${count === 1 ? singular : plural}`;
}

export function sniktImportReportSummary(report: SniktImportReport) {
  const base = `Imported SNIKT.com Collection ${report.snikt_collection_id}: ${report.galleries_imported} galleries, ${report.artworks_imported} artworks, ${report.images_downloaded} images`;
  if (report.image_download_failures <= 0) return base;
  const failureText =
    report.image_download_failures === 1
      ? "1 image failed"
      : `${report.image_download_failures} image failures`;
  return `${base}, ${failureText}`;
}
