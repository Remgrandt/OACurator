// Copyright (c) 2026 Remgrandt Works. All rights reserved.

/// <reference types="vite/client" />

interface ImportMetaEnv {
  readonly VITE_OAC_DEMO_AUTOMATION?: string;
}

interface Window {
  __oacDemoCaptionText?: string;
  __oacDemoCursorPosition?: { x: number; y: number };
}
