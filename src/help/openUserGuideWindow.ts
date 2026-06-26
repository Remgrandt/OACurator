import { WebviewWindow } from "@tauri-apps/api/webviewWindow";

export const USER_GUIDE_URL = "/help/index.html";
const USER_GUIDE_WINDOW_LABEL = "oa-curator-user-guide";
const MATERIAL_DESKTOP_NAV_MIN_WIDTH = 1220;

export async function openUserGuideWindow() {
  if (!isTauriRuntime()) {
    openBrowserHelpWindow();
    return;
  }

  try {
    const existingWindow = await WebviewWindow.getByLabel(USER_GUIDE_WINDOW_LABEL);
    if (existingWindow) {
      await Promise.allSettled([
        existingWindow.unminimize(),
        existingWindow.show(),
        existingWindow.setFocus(),
      ]);
      return;
    }

    const helpWindow = new WebviewWindow(USER_GUIDE_WINDOW_LABEL, {
      center: true,
      decorations: true,
      height: 760,
      minHeight: 520,
      minWidth: MATERIAL_DESKTOP_NAV_MIN_WIDTH,
      resizable: true,
      title: "OA Curator User Guide",
      url: USER_GUIDE_URL,
      width: 1280,
    });

    void helpWindow.once("tauri://error", openBrowserHelpWindow).catch(openBrowserHelpWindow);
  } catch {
    openBrowserHelpWindow();
  }
}

function isTauriRuntime() {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

function openBrowserHelpWindow() {
  const helpLink = document.createElement("a");
  helpLink.href = USER_GUIDE_URL;
  helpLink.target = "_blank";
  helpLink.rel = "noopener";
  helpLink.click();
}
