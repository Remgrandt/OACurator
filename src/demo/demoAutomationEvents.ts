import type { AttachMode } from "../domain/types";

export const DEMO_ATTACH_FILE_EVENT = "oac-demo-attach-file";
export const DEMO_ATTACH_VISUAL_EVENT = "oac-demo-attach-visual";
export const DEMO_CAPTION_EVENT = "oac-demo-caption";
export const DEMO_CURSOR_EVENT = "oac-demo-cursor";
export const DEMO_SELECT_VISUAL_EVENT = "oac-demo-select-visual";

export type DemoAttachFileRequest = {
  paths: string[];
  mode: AttachMode;
};

export type DemoCursorEventDetail = {
  x: number;
  y: number;
  visible?: boolean;
  pressed?: boolean;
};

export type DemoAttachVisualEventDetail = {
  paths: string[];
  mode: AttachMode;
  phase?: "choosing" | "attaching" | "complete";
  visible?: boolean;
};

export type DemoSelectVisualEventDetail = {
  x: number;
  y: number;
  width?: number;
  label?: string;
  options: string[];
  selected: string;
  visible?: boolean;
};

export function demoAttachFileEventDetail(event: Event): DemoAttachFileRequest | null {
  const detail = (event as CustomEvent<unknown>).detail;
  if (!isRecord(detail)) return null;

  const rawPaths = detail["paths"];
  if (!Array.isArray(rawPaths)) return null;
  const paths = rawPaths.filter(
    (candidate): candidate is string =>
      typeof candidate === "string" && candidate.trim().length > 0,
  );
  if (paths.length !== rawPaths.length || paths.length === 0) return null;

  const mode = detail["mode"] ?? "copy";
  if (mode !== "copy" && mode !== "link") return null;

  return { paths, mode };
}

export function demoAutomationEnabled() {
  return import.meta.env.VITE_OAC_DEMO_AUTOMATION === "1";
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
