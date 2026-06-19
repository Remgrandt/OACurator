// Copyright (c) 2026 Remgrandt Works. All rights reserved.

import { lazy, Suspense } from "react";

const WorkbenchApp = lazy(() => import("./WorkbenchApp"));

function StartupShell() {
  return (
    <div
      className="startup-shell"
      role="status"
      aria-live="polite"
      style={{
        minHeight: "100vh",
        display: "grid",
        placeItems: "center",
        background: "#282A36",
        color: "#F8F8F2",
        fontFamily:
          'Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif',
      }}
    >
      <div
        className="startup-card"
        style={{
          display: "grid",
          gap: 12,
          justifyItems: "center",
          color: "#6272A4",
        }}
      >
        <img src="/oac-logo-app.svg" alt="" style={{ width: 120, height: 120 }} />
        <div className="startup-title" style={{ color: "#F8F8F2", fontWeight: 700 }}>
          OA Curator
        </div>
        <div>Starting workbench...</div>
      </div>
    </div>
  );
}

function App() {
  return (
    <Suspense fallback={<StartupShell />}>
      <WorkbenchApp />
    </Suspense>
  );
}

export default App;
