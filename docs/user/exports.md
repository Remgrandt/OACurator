# Exporting Artwork

OA Curator separates original scan files from export derivatives.

## PNG Derivatives

Use PNG export to create a web-ready derivative from an attached source image. The derivative is recorded with the Artwork and can be used for review, sharing, or publishing preparation.

PNG is the preferred web/export format. TIFF remains the preferred preservation scan format.

## Originals Are Not Edited

Exporting a PNG derivative should not overwrite or modify the original JPG, PNG, or TIFF source file. If the original is linked from another folder, the derivative is still app-managed separately.

## Gallery Site Publishing Preparation

OA Curator can help prepare images and metadata for gallery site publishing workflows.

### ComicArtFans

For ComicArtFans, fill in CAF-compatible fields in OA Curator, create PNG derivatives if needed, then manually copy/paste metadata and choose image files in CAF's own artwork entry screens.

CAF does not currently provide a supported bulk upload file format. If you want CAF to add bulk import, OAA is the preferred format to request because it can carry Collection, Gallery, Artwork, gallery-site metadata, and optional embedded files in a single collector-controlled archive. You can ask CAF for OAA or CSV bulk import support through the [CAF contact page](https://www.comicartfans.com/contact.asp).

### SNIKT.com

For SNIKT.com, fill in SNIKT.com fields in OA Curator, create PNG derivatives if needed, then use the supported upload-prefill workflow or manually copy/paste metadata into SNIKT.com and choose the image file there.

SNIKT.com does not currently provide a bulk upload file format. If you want SNIKT.com to add bulk import, OAA is the preferred format to request because it can carry Collection, Gallery, Artwork, gallery-site metadata, and optional embedded files in a single collector-controlled archive. You can request OAA or CSV bulk import support by contacting [SNIKT.com](mailto:info@snikt.com).

### Raremarq

For Raremarq CSV export, see [Exporting To Raremarq](exporting-raremarq.md).

## OAA Archive Export

Use **File > Export OAA Archive** to write an `.oaa` archive for the open Collection.

The export wizard lets you include or omit artwork files and private collector metadata. When files are included, linked files are resolved into OAA-local embedded files in the exported archive without changing the open Collection on disk. When files are omitted, the archive still carries Collection, Gallery, Artwork, non-private metadata, and external-site extension data.

Leave **Include private collector metadata** enabled for a private backup. Turn it off before making an archive for public sharing.

OAA is the preferred portable backup and interchange format because it is an open ZIP-based package with plain text manifests.

## Web Publishing Workflows

Collectors may use different image sizes or metadata detail depending on their publishing destination. OA Curator can create PNG derivatives and gallery-site-specific CSV files where a supported file workflow exists.

Private fields such as purchase price and provenance are for local collection management and should not be included in public publishing material unless you intentionally share them.
