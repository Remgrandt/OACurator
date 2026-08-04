import { WebviewWindow } from "@tauri-apps/api/webviewWindow";

export const USER_GUIDE_URL = "/help/index.html";
export const CAF_IMPORT_GUIDE_URL = "/help/importing-caf.html#save-the-caf-report-as-csv";
const USER_GUIDE_WINDOW_LABEL = "oa-curator-user-guide";
const CAF_IMPORT_GUIDE_WINDOW_LABEL = "oa-curator-caf-import-guide";
const MATERIAL_DESKTOP_NAV_MIN_WIDTH = 1220;

export async function openUserGuideWindow() {
  await openHelpWindow(USER_GUIDE_WINDOW_LABEL, USER_GUIDE_URL);
}

export async function openCafImportGuideWindow() {
  await openHelpWindow(CAF_IMPORT_GUIDE_WINDOW_LABEL, CAF_IMPORT_GUIDE_URL);
}

async function openHelpWindow(windowLabel: string, url: string) {
  if (!isTauriRuntime()) {
    openBrowserHelpWindow(url);
    return;
  }

  try {
    const existingWindow = await WebviewWindow.getByLabel(windowLabel);
    if (existingWindow) {
      await Promise.allSettled([
        existingWindow.unminimize(),
        existingWindow.show(),
        existingWindow.setFocus(),
      ]);
      return;
    }

    const helpWindow = new WebviewWindow(windowLabel, {
      center: true,
      decorations: true,
      height: 760,
      minHeight: 520,
      minWidth: MATERIAL_DESKTOP_NAV_MIN_WIDTH,
      resizable: true,
      title: "OA Curator User Guide",
      url,
      width: 1280,
    });

    void helpWindow
      .once("tauri://error", () => openBrowserHelpWindow(url))
      .catch(() => openBrowserHelpWindow(url));
  } catch {
    openBrowserHelpWindow(url);
  }
}

function isTauriRuntime() {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

function openBrowserHelpWindow(url: string) {
  const helpLink = document.createElement("a");
  helpLink.href = url;
  helpLink.target = "_blank";
  helpLink.rel = "noopener";
  helpLink.click();
}
