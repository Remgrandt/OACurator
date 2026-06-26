import { relaunch } from "@tauri-apps/plugin-process";
import { check, type DownloadEvent, type Update } from "@tauri-apps/plugin-updater";

export type AppUpdateInfo = {
  currentVersion: string;
  version: string;
  date?: string;
  body?: string;
  update: Update;
};

export type AppUpdateProgress = {
  downloaded: number;
  total?: number;
};

export async function checkForAppUpdate(): Promise<AppUpdateInfo | null> {
  const update = await check();
  if (!update) return null;

  const info: AppUpdateInfo = {
    currentVersion: update.currentVersion,
    version: update.version,
    update,
  };
  if (update.date) info.date = update.date;
  if (update.body) info.body = update.body;
  return info;
}

export async function installAppUpdate(
  update: AppUpdateInfo,
  onProgress: (progress: AppUpdateProgress) => void,
): Promise<void> {
  let downloaded = 0;
  let total: number | undefined;

  await update.update.downloadAndInstall((event: DownloadEvent) => {
    if (event.event === "Started") {
      downloaded = 0;
      total = event.data.contentLength;
      onProgress(progressPayload(downloaded, total));
      return;
    }
    if (event.event === "Progress") {
      downloaded += event.data.chunkLength;
      onProgress(progressPayload(downloaded, total));
    }
  });

  await relaunch();
}

function progressPayload(downloaded: number, total: number | undefined): AppUpdateProgress {
  return total === undefined ? { downloaded } : { downloaded, total };
}
