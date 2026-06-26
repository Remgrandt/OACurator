import { useCallback, useEffect, useRef } from "react";
import { metadataAutosaveKey } from "../../domain/metadataRequests";
import type { ArtworkDetail, DetailForm } from "../../domain/types";

export type MetadataAutosaveFlushResult = "clean" | "flushed" | "failed";

type UseMetadataAutosaveOptions = {
  detail: ArtworkDetail | null;
  form: DetailForm;
  delayMs?: number;
  onSave: (artworkId: number, form: DetailForm, savedKey: string) => boolean | Promise<boolean>;
};

export function useMetadataAutosave({
  detail,
  form,
  delayMs = 500,
  onSave,
}: UseMetadataAutosaveOptions) {
  const metadataAutosaveKeyRef = useRef<string | null>(null);
  const onSaveRef = useRef(onSave);
  const latestDetailRef = useRef<ArtworkDetail | null>(detail);
  const latestFormRef = useRef(form);
  const timeoutRef = useRef<number | null>(null);

  const clearScheduledAutosave = useCallback(() => {
    if (timeoutRef.current === null) return;
    window.clearTimeout(timeoutRef.current);
    timeoutRef.current = null;
  }, []);

  useEffect(() => {
    onSaveRef.current = onSave;
  }, [onSave]);

  useEffect(() => {
    latestDetailRef.current = detail;
    latestFormRef.current = form;
  }, [detail, form]);

  useEffect(() => {
    if (!detail) {
      metadataAutosaveKeyRef.current = null;
      clearScheduledAutosave();
      return;
    }

    const nextKey = metadataAutosaveKey(detail.id, form);
    if (metadataAutosaveKeyRef.current === null) {
      metadataAutosaveKeyRef.current = nextKey;
      return;
    }
    if (metadataAutosaveKeyRef.current === nextKey) return;

    const timeout = window.setTimeout(() => {
      if (timeoutRef.current === timeout) {
        timeoutRef.current = null;
      }
      void onSaveRef.current(detail.id, form, nextKey);
    }, delayMs);
    timeoutRef.current = timeout;

    return () => {
      if (timeoutRef.current === timeout) {
        timeoutRef.current = null;
      }
      window.clearTimeout(timeout);
    };
  }, [clearScheduledAutosave, delayMs, detail, form]);

  const markMetadataAutosaveBaseline = useCallback((artworkId: number, nextForm: DetailForm) => {
    metadataAutosaveKeyRef.current = metadataAutosaveKey(artworkId, nextForm);
  }, []);

  const markMetadataAutosaveSaved = useCallback((savedKey: string) => {
    metadataAutosaveKeyRef.current = savedKey;
  }, []);

  const flushMetadataAutosave = useCallback(async (): Promise<MetadataAutosaveFlushResult> => {
    const latestDetail = latestDetailRef.current;
    if (!latestDetail) return "clean";

    const latestForm = latestFormRef.current;
    const nextKey = metadataAutosaveKey(latestDetail.id, latestForm);
    if (metadataAutosaveKeyRef.current === null) {
      metadataAutosaveKeyRef.current = nextKey;
      return "clean";
    }
    if (metadataAutosaveKeyRef.current === nextKey) return "clean";

    clearScheduledAutosave();
    const saved = await onSaveRef.current(latestDetail.id, latestForm, nextKey);
    return saved ? "flushed" : "failed";
  }, [clearScheduledAutosave]);

  return {
    flushMetadataAutosave,
    markMetadataAutosaveBaseline,
    markMetadataAutosaveSaved,
  };
}
