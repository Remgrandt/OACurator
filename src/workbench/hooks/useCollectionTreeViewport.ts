// Copyright (c) 2026 Remgrandt Works. All rights reserved.

import { useCallback, useLayoutEffect, useRef, useState, type UIEvent } from "react";

export type CollectionTreeViewport = {
  scrollTop: number;
  height: number;
};

export function useCollectionTreeViewport(dependencyKey: string) {
  const ref = useRef<HTMLDivElement | null>(null);
  const [viewport, setViewport] = useState<CollectionTreeViewport>({ scrollTop: 0, height: 0 });

  const updateViewport = useCallback(() => {
    const element = ref.current;
    if (!element) return;
    setViewport({
      scrollTop: element.scrollTop,
      height: element.clientHeight,
    });
  }, []);

  const handleScroll = useCallback((event: UIEvent<HTMLDivElement>) => {
    setViewport({
      scrollTop: event.currentTarget.scrollTop,
      height: event.currentTarget.clientHeight,
    });
  }, []);

  useLayoutEffect(() => {
    const element = ref.current;
    if (!element) return;

    updateViewport();
    const resizeObserver = "ResizeObserver" in window ? new ResizeObserver(updateViewport) : null;
    resizeObserver?.observe(element);
    window.addEventListener("resize", updateViewport);
    return () => {
      resizeObserver?.disconnect();
      window.removeEventListener("resize", updateViewport);
    };
  }, [dependencyKey, updateViewport]);

  return { ref, viewport, handleScroll };
}
