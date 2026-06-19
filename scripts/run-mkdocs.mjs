// Copyright (c) 2026 Remgrandt Works. All rights reserved.

import { existsSync } from "node:fs";
import { spawnSync } from "node:child_process";
import path from "node:path";

const args = process.argv.slice(2);

const candidates = [
  process.env.OAC_MKDOCS_PYTHON,
  process.platform === "win32"
    ? path.join(".venv", "Scripts", "python.exe")
    : path.join(".venv", "bin", "python"),
  "python",
].filter(Boolean);

const python = candidates.find((candidate) => {
  return candidate === "python" || existsSync(candidate);
});

const result = spawnSync(python, ["-m", "mkdocs", ...args], {
  stdio: "inherit",
  shell: false,
});

if (result.error) {
  console.error(result.error.message);
  process.exit(1);
}

process.exit(result.status ?? 1);
