// Copyright (c) 2026 Remgrandt Works. All rights reserved.

import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

const remoteHost = process.env["TAURI_DEV_HOST"];
const host = remoteHost || "127.0.0.1";

export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host,
    ...(remoteHost
      ? {
          hmr: {
            protocol: "ws",
            host,
            port: 1421,
          },
        }
      : {}),
    watch: {
      ignored: ["**/src-tauri/**"],
    },
  },
});
