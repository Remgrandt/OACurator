<!-- Copyright (c) 2026 Remgrandt Works. All rights reserved. -->

# Exporting To Raremarq

Raremarq offers a bulk upload workflow:

<https://www.raremarq.com/pieces/bulk-upload-options>

Use **File > Export to Raremarq** to export the open Collection as a Raremarq bulk-upload CSV.

## Export Scope

The wizard first asks which Artworks to export:

- Export every Artwork in the Collection. OA Curator warns when Artworks already have Raremarq URLs because those rows may create duplicates.
- Export only Artworks that do not already have a Raremarq URL.

## Image URL Strategy

Raremarq requires a public URL for each primary image. OA Curator lets you choose how to fill that column:

- Use the Artwork **Generic URL** field. OA Curator warns how many rows will still be blank.
- Leave URL fields blank for manual repair in the CSV.
- Upload obfuscated temporary image copies to tmpfiles.org with a 24-hour expiry. OA Curator creates downsized temporary copies for primary image files over 20 MB, uploads the copies, verifies each URL is live, and writes those temporary URLs into the CSV.

The export wizard remains open until the CSV has finished writing so the Collection stays stable while the export is being prepared.

OA Curator does not crawl Raremarq or download missing image files for this workflow.
