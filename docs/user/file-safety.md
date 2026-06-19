<!-- Copyright (c) 2026 Remgrandt Works. All rights reserved. -->

# File Safety

OA Curator is built around the assumption that original art scans are valuable source files.

## Original Files

Original JPG, PNG, and TIFF files should not be overwritten by cataloging, browsing, metadata editing, thumbnail generation, preview generation, or PNG export.

TIFF is supported as a preservation scan format. The app should generate usable previews without requiring you to convert TIFF originals by hand.

## Copy Or Link Attachments

When attaching an image, choose the file handling mode that matches your intent:

- **Copy** creates a Collection-managed copy of the file.
- **Link** records the existing file location without moving the file.

Copy is useful when you want the Collection to be self-contained. Link is useful when you already maintain source files elsewhere and do not want another copy.

## Cached Assets

Thumbnails and previews are generated assets. They belong in the app cache, not with your original source files.

If a cached preview is missing or stale, OA Curator should be able to regenerate it from the original file.

## Moving Or Renaming Files

OA Curator does not currently provide a scan, move, or rename workflow for source files.

If you linked to a file in its existing location and later move or rename that file outside OA Curator, update the Artwork so previews and exports can find the file again.

## Missing Files

If an Artwork references a linked file that has been moved or deleted outside OA Curator, the catalog record can remain but the preview or export action may fail until the file path is corrected.

## Non-Image Files

An Artwork may have non-image supporting files as collection records. You can attach them manually or import them through an OAA archive, but OA Curator does not try to preview, thumbnail, render, convert, or execute them.

Only supported image types should enter the thumbnail, preview, and PNG export pipeline.

JPG, PNG, and TIFF are the supported image types for previews, thumbnails, and PNG exports. PDF, PSD, HEIC, AVIF, WebP, and similar formats are generic attachments only.
