# Privacy

OA Curator is local-first. Your catalog, gallery-site links, private collector fields, attached files, previews, and exports are intended to live on your computer unless you choose to import, export, open a gallery site workflow, or upload temporary files for a specific export.

## Local Catalog

OA Curator stores your working catalog locally and writes portable manifest files for Collection, Gallery, and Artwork records.

Metadata is stored in the local catalog and manifests. OA Curator does not embed metadata into original image files by design.

## Private Fields

These fields are private collector data:

- Purchase price
- Estimated value
- Purchase date
- Provenance
- Personal notes

Use them for your own records, such as seller, acquisition context, ownership history, value tracking, or private notes.

## Network Use

Core cataloging and browsing should work offline.

ComicArtFans CSV import, SNIKT.com CSV import, and OAA import read local files and do not require network access. OAA export writes a local archive file. Raremarq currently does not provide a bulk export file.

Raremarq CSV export is local by default when you use Generic URL values or leave URL fields blank. If you choose the tmpfiles.org option, OA Curator uploads temporary obfuscated copies of the selected primary image files to tmpfiles.org with a 24-hour expiry so Raremarq can fetch them from public URLs.

SNIKT.com upload-prefill actions and opening gallery site URLs use the normal browser/network behavior for those sites. The **SNIKT export** button opens a SNIKT.com upload URL that can include private estimated value data for the selected Artwork, because that value is part of the supported SNIKT.com upload-prefill workflow.

## OAA Archives

OAA archives are portable collection archives. They may contain private metadata and image files.

Treat OAA archives as private unless you have reviewed the contents and are comfortable sharing them. In the OAA export wizard, turn off **Include private collector metadata** before making an archive for public sharing.

## Public Sharing Boundary

Before publishing, uploading, or sharing generated files, review what the workflow includes:

- A PNG derivative is an image export.
- A SNIKT.com upload-prefill action opens a browser workflow and can include the selected Artwork's estimated value in the SNIKT.com upload URL.
- A Raremarq CSV export may include public URLs or temporary uploaded image-copy URLs.
- An OAA archive may include collection metadata, private collector metadata, and files depending on the export options you choose.

Private collector fields should be excluded from public-only sharing unless you intentionally include them.
