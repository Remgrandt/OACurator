// Copyright (c) 2026 Remgrandt Works. All rights reserved.

import { invoke } from "@tauri-apps/api/core";

export interface StartupTraceEvent {
  name: string;
  category: string;
  timestampMs: number;
  source: string;
  detail?: Record<string, unknown>;
}

declare global {
  interface Window {
    __OAC_STARTUP_TRACE_EVENTS?: StartupTraceEvent[];
  }
}

const traceEvents = window.__OAC_STARTUP_TRACE_EVENTS ?? [];
window.__OAC_STARTUP_TRACE_EVENTS = traceEvents;

export function markStartupTrace(
  name: string,
  category = "frontend",
  detail?: Record<string, unknown>,
) {
  const timestampMs = performance.timeOrigin + performance.now();
  performance.mark(`oac:${name}`);
  const event: StartupTraceEvent = {
    name,
    category,
    timestampMs,
    source: "webview",
  };
  if (detail !== undefined) {
    event.detail = detail;
  }
  traceEvents.push(event);
}

export async function finishStartupTrace(detail?: Record<string, unknown>): Promise<string | null> {
  markStartupTrace("frontend_trace_collect_begin", "frontend", detail);
  const frontendEvents = [
    ...traceEvents,
    ...performanceMarks(),
    ...paintEvents(),
    navigationEvent(),
  ].filter(Boolean) as StartupTraceEvent[];

  return invoke<string | null>("finish_startup_trace_command", { frontendEvents });
}

function performanceMarks(): StartupTraceEvent[] {
  return performance.getEntriesByType("mark").map((entry) => ({
    name: `mark:${entry.name}`,
    category: "performance-mark",
    timestampMs: performance.timeOrigin + entry.startTime,
    source: "webview",
    detail: {
      durationMs: entry.duration,
    },
  }));
}

function paintEvents(): StartupTraceEvent[] {
  return performance.getEntriesByType("paint").map((entry) => ({
    name: `paint:${entry.name}`,
    category: "paint",
    timestampMs: performance.timeOrigin + entry.startTime,
    source: "webview",
    detail: {
      durationMs: entry.duration,
    },
  }));
}

function navigationEvent(): StartupTraceEvent | null {
  const navigation = performance.getEntriesByType("navigation")[0] as
    | PerformanceNavigationTiming
    | undefined;
  if (!navigation) {
    return null;
  }
  return {
    name: "navigation",
    category: "navigation",
    timestampMs: performance.timeOrigin + navigation.startTime,
    source: "webview",
    detail: {
      domInteractiveMs: navigation.domInteractive,
      domContentLoadedMs: navigation.domContentLoadedEventEnd,
      loadEventEndMs: navigation.loadEventEnd,
    },
  };
}
