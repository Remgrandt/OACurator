// Copyright (c) 2026 Remgrandt Works. All rights reserved.

import { existsSync, readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";

const assetsDir = join("dist", "assets");

if (!existsSync(assetsDir)) {
  throw new Error("dist/assets does not exist; run the frontend build first.");
}

const scripts = readdirSync(assetsDir).filter((name) => name.endsWith(".js"));
const workbenchChunks = scripts.filter((name) => name.startsWith("WorkbenchApp-"));
const lazyWorkbenchImporters = scripts.filter((name) =>
  readFileSync(join(assetsDir, name), "utf8").includes('import("./WorkbenchApp'),
);

if (workbenchChunks.length > 0 || lazyWorkbenchImporters.length > 0) {
  throw new Error(
    `Release bundle lazy-loads WorkbenchApp: ${[
      ...workbenchChunks,
      ...lazyWorkbenchImporters,
    ].join(", ")}`,
  );
}

console.log("Release bundle embeds WorkbenchApp in the startup chunk.");
