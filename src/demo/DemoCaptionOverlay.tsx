import { useEffect, useState } from "react";
import { createPortal } from "react-dom";
import {
  DEMO_ATTACH_VISUAL_EVENT,
  DEMO_CAPTION_EVENT,
  DEMO_CURSOR_EVENT,
  DEMO_SELECT_VISUAL_EVENT,
  demoAutomationEnabled,
  type DemoAttachVisualEventDetail,
  type DemoCursorEventDetail,
  type DemoSelectVisualEventDetail,
} from "./demoAutomationEvents";
import { demoSelectVisibleWindow } from "./demoSelectVisual";

type DemoCaptionOverlayProps = {
  enabled?: boolean;
};

type DemoCaptionEventDetail = {
  text?: string;
};

type DemoCursorState = {
  x: number;
  y: number;
  visible: boolean;
  pressed: boolean;
};

type DemoAttachVisualState = {
  paths: string[];
  mode: "copy" | "link";
  phase: "choosing" | "attaching" | "complete";
  visible: boolean;
};

type DemoSelectVisualState = {
  x: number;
  y: number;
  width: number;
  label: string;
  options: string[];
  selected: string;
  visible: boolean;
};

export function DemoCaptionOverlay({ enabled = demoAutomationEnabled() }: DemoCaptionOverlayProps) {
  const [caption, setCaption] = useState(() =>
    enabled ? (window.__oacDemoCaptionText ?? "").trim() : "",
  );
  const [cursor, setCursor] = useState<DemoCursorState | null>(null);
  const [attachVisual, setAttachVisual] = useState<DemoAttachVisualState | null>(null);
  const [selectVisual, setSelectVisual] = useState<DemoSelectVisualState | null>(null);

  useEffect(() => {
    if (!enabled) {
      setCaption("");
      setCursor(null);
      setAttachVisual(null);
      setSelectVisual(null);
      return;
    }

    const handleCaption = (event: Event) => {
      const detail = (event as CustomEvent<DemoCaptionEventDetail>).detail;
      const text = typeof detail?.text === "string" ? detail.text.trim() : "";
      window.__oacDemoCaptionText = text;
      setCaption(text);
    };

    const handleCursor = (event: Event) => {
      const detail = (event as CustomEvent<DemoCursorEventDetail>).detail;
      if (!Number.isFinite(detail?.x) || !Number.isFinite(detail?.y)) return;
      setCursor({
        x: detail.x,
        y: detail.y,
        visible: detail.visible ?? true,
        pressed: detail.pressed ?? false,
      });
    };

    const handleAttachVisual = (event: Event) => {
      const detail = (event as CustomEvent<DemoAttachVisualEventDetail>).detail;
      const paths = Array.isArray(detail?.paths)
        ? detail.paths.filter((candidate) => typeof candidate === "string" && candidate.trim())
        : [];
      if (paths.length === 0 || (detail.mode !== "copy" && detail.mode !== "link")) return;
      setAttachVisual({
        paths,
        mode: detail.mode,
        phase: detail.phase ?? "choosing",
        visible: detail.visible ?? true,
      });
    };

    const handleSelectVisual = (event: Event) => {
      const detail = (event as CustomEvent<DemoSelectVisualEventDetail>).detail;
      const options = Array.isArray(detail?.options)
        ? detail.options.filter((candidate) => typeof candidate === "string" && candidate.trim())
        : [];
      if (
        !Number.isFinite(detail?.x) ||
        !Number.isFinite(detail?.y) ||
        options.length === 0 ||
        typeof detail.selected !== "string"
      ) {
        return;
      }
      const visibleWindow = demoSelectVisibleWindow(options, detail.selected);
      setSelectVisual({
        x: detail.x,
        y: detail.y,
        width: Math.max(180, detail.width ?? 220),
        label: typeof detail.label === "string" ? detail.label.trim() : "Dropdown",
        options: visibleWindow.options,
        selected: detail.selected,
        visible: detail.visible ?? true,
      });
    };

    window.addEventListener(DEMO_ATTACH_VISUAL_EVENT, handleAttachVisual);
    window.addEventListener(DEMO_CAPTION_EVENT, handleCaption);
    window.addEventListener(DEMO_CURSOR_EVENT, handleCursor);
    window.addEventListener(DEMO_SELECT_VISUAL_EVENT, handleSelectVisual);
    setCaption((window.__oacDemoCaptionText ?? "").trim());
    return () => {
      window.removeEventListener(DEMO_ATTACH_VISUAL_EVENT, handleAttachVisual);
      window.removeEventListener(DEMO_CAPTION_EVENT, handleCaption);
      window.removeEventListener(DEMO_CURSOR_EVENT, handleCursor);
      window.removeEventListener(DEMO_SELECT_VISUAL_EVENT, handleSelectVisual);
    };
  }, [enabled]);

  if (!enabled) return null;

  return createPortal(
    <>
      {caption ? (
        <div
          className="demo-caption-overlay"
          role="status"
          aria-live="polite"
          aria-label="Demo caption"
        >
          {caption}
        </div>
      ) : null}
      {cursor?.visible ? (
        <div
          className={`demo-cursor-overlay${cursor.pressed ? " pressed" : ""}`}
          data-testid="demo-cursor"
          aria-hidden="true"
          style={{ transform: `translate3d(${cursor.x}px, ${cursor.y}px, 0)` }}
        />
      ) : null}
      {attachVisual?.visible ? (
        <div
          className="demo-attach-overlay"
          role="status"
          aria-live="polite"
          aria-label="Demo file attach"
        >
          <div className="demo-attach-heading">{attachPhaseLabel(attachVisual)}</div>
          <div className="demo-attach-file">{fileNameFromPath(attachVisual.paths[0] ?? "")}</div>
          <div className="demo-attach-path">{attachVisual.paths[0]}</div>
          <div className="demo-attach-mode">{attachModeLabel(attachVisual.mode)}</div>
        </div>
      ) : null}
      {selectVisual?.visible ? (
        <div
          className="demo-select-overlay"
          role="status"
          aria-live="polite"
          aria-label="Demo dropdown"
          style={{
            left: `${selectVisual.x}px`,
            top: `${selectVisual.y}px`,
            width: `${selectVisual.width}px`,
          }}
        >
          <div className="demo-select-heading">{selectVisual.label}</div>
          <div className="demo-select-options">
            {selectVisual.options.map((option) => (
              <div
                className={`demo-select-option${option === selectVisual.selected ? " selected" : ""}`}
                key={option}
              >
                {option}
              </div>
            ))}
          </div>
        </div>
      ) : null}
    </>,
    document.body,
  );
}

function attachPhaseLabel(attachVisual: DemoAttachVisualState) {
  if (attachVisual.phase === "attaching") {
    return attachVisual.mode === "copy" ? "Copying file into Artwork" : "Linking file to Artwork";
  }
  if (attachVisual.phase === "complete") return "File attached";
  return "Choose file to attach";
}

function attachModeLabel(mode: DemoAttachVisualState["mode"]) {
  return mode === "copy" ? "Copy into Artwork" : "Link from current location";
}

function fileNameFromPath(filePath: string) {
  const normalized = filePath.replace(/\\/g, "/");
  return normalized.slice(normalized.lastIndexOf("/") + 1) || filePath;
}
