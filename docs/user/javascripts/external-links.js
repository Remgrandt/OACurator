// Copyright (c) 2026 Remgrandt Works. All rights reserved.

(function () {
  "use strict";

  const openerCommand = "plugin:opener|open_url";

  function externalHttpUrl(rawUrl) {
    try {
      const url = new URL(rawUrl, window.location.href);
      const isHttp = url.protocol === "https:" || url.protocol === "http:";
      return isHttp && url.origin !== window.location.origin ? url.href : null;
    } catch {
      return null;
    }
  }

  function fallbackOpen(url) {
    const opened = window.open(url, "_blank", "noopener,noreferrer");
    if (!opened) {
      window.location.href = url;
    }
  }

  async function openExternalUrl(url) {
    const invoke = window.__TAURI__?.core?.invoke;
    if (typeof invoke !== "function") {
      fallbackOpen(url);
      return;
    }

    try {
      await invoke(openerCommand, { url });
    } catch {
      fallbackOpen(url);
    }
  }

  function isPlainLeftClick(event) {
    return (
      event.button === 0 && !event.altKey && !event.ctrlKey && !event.metaKey && !event.shiftKey
    );
  }

  function handleDocumentClick(event) {
    if (event.defaultPrevented || !isPlainLeftClick(event)) return;

    const anchor = event.target?.closest?.("a[href]");
    if (!anchor) return;

    const url = externalHttpUrl(anchor.href);
    if (!url) return;

    event.preventDefault();
    void openExternalUrl(url);
  }

  window.OACHelpExternalLinks = {
    externalHttpUrl,
    handleDocumentClick,
  };

  document.addEventListener("click", handleDocumentClick);
})();
