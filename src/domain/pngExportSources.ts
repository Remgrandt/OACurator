import { isRenderableImageExtension } from "./formatters";
import type { CarouselImageItem, FileAsset } from "./types";

export function selectRenderableSourceForPngExport(
  files: FileAsset[],
  selectedItem: CarouselImageItem | null,
): FileAsset | null {
  if (!selectedItem) return null;

  const sourceFile =
    selectedItem.kind === "file"
      ? files.find((asset) => asset.id === selectedItem.id)
      : selectedItem.sourceFileAssetId
        ? files.find((asset) => asset.id === selectedItem.sourceFileAssetId)
        : null;

  return sourceFile && isRenderableImageExtension(sourceFile.extension) ? sourceFile : null;
}
