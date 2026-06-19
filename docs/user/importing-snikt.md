<!-- Copyright (c) 2026 Remgrandt Works. All rights reserved. -->

# Importing From SNIKT.com

OA Curator can import a SNIKT.com gallery CSV export from disk.

The SNIKT.com CSV import does not crawl SNIKT.com pages and does not download images.

If a Collection is already open, SNIKT.com import merges into that Collection. Close the open Collection first if you want the import to create a new local Collection.

## Matching Limits

The current SNIKT.com CSV export does not provide a stable per-artwork ID. OA Curator matches imported rows to existing local Artworks using the title and created date when possible.

If the CSV row cannot be matched safely, OA Curator imports it as a new Artwork or asks for reconciliation when there is an ambiguous local candidate.

SNIKT.com terms are at [https://snikt.com/terms](https://snikt.com/terms) and prohibit scraping.
