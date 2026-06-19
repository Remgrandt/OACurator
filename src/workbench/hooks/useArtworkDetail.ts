// Copyright (c) 2026 Remgrandt Works. All rights reserved.

import { useCallback, useState } from "react";
import { emptyForm } from "../../domain/constants";
import { formFromDetail } from "../../domain/formatters";
import type { ArtworkDetail, DetailForm } from "../../domain/types";

export function useArtworkDetail() {
  const [detail, setDetail] = useState<ArtworkDetail | null>(null);
  const [form, setForm] = useState<DetailForm>(emptyForm);

  const setArtworkDetailFromSnapshot = useCallback((nextDetail: ArtworkDetail) => {
    const nextForm = formFromDetail(nextDetail);
    setDetail(nextDetail);
    setForm(nextForm);
    return nextForm;
  }, []);

  const clearArtworkDetail = useCallback(() => {
    setDetail(null);
    setForm(emptyForm);
  }, []);

  return {
    detail,
    form,
    setDetail,
    setForm,
    setArtworkDetailFromSnapshot,
    clearArtworkDetail,
  };
}
