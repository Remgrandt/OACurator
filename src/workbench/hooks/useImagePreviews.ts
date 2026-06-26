import { useCallback, useEffect, useState } from "react";
import { carouselItemsForDetail } from "../../domain/formatters";
import type { ArtworkDetail, ImageDataUrlSource } from "../../domain/types";

type UseImagePreviewsOptions = {
  detail: ArtworkDetail | null;
  loadImageDataUrl: (source: ImageDataUrlSource) => Promise<string>;
};

export function useImagePreviews({ detail, loadImageDataUrl }: UseImagePreviewsOptions) {
  const [detailImageUrls, setDetailImageUrls] = useState<Record<string, string | null>>({});
  const [selectedCarouselItemKey, setSelectedCarouselItemKey] = useState<string | null>(null);

  useEffect(() => {
    let disposed = false;

    if (!detail) {
      setDetailImageUrls({});
      setSelectedCarouselItemKey(null);
      return () => {
        disposed = true;
      };
    }

    const carouselItems = carouselItemsForDetail(detail);
    setSelectedCarouselItemKey((current) =>
      current && carouselItems.some((item) => item.key === current)
        ? current
        : (carouselItems[0]?.key ?? null),
    );
    setDetailImageUrls({});

    const imageSources = new Map<string, ImageDataUrlSource>();
    for (const item of carouselItems) {
      if (item.thumbnailPath && item.thumbnailSource) {
        imageSources.set(item.thumbnailPath, item.thumbnailSource);
      }
      if (item.previewPath && item.previewSource) {
        imageSources.set(item.previewPath, item.previewSource);
      }
    }
    if (imageSources.size === 0) {
      setDetailImageUrls({});
      return () => {
        disposed = true;
      };
    }

    imageSources.forEach((source, path) => {
      void loadImageDataUrl(source)
        .then((url) => {
          if (!disposed) {
            setDetailImageUrls((current) => ({ ...current, [path]: url }));
          }
        })
        .catch(() => {
          if (!disposed) {
            setDetailImageUrls((current) => ({ ...current, [path]: null }));
          }
        });
    });

    return () => {
      disposed = true;
    };
  }, [detail, loadImageDataUrl]);

  const resetImagePreviews = useCallback(() => {
    setDetailImageUrls({});
    setSelectedCarouselItemKey(null);
  }, []);

  return {
    detailImageUrls,
    selectedCarouselItemKey,
    setSelectedCarouselItemKey,
    resetImagePreviews,
  };
}
