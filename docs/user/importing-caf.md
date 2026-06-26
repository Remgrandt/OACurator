# Importing From ComicArtFans CSV

OA Curator can import a ComicArtFans CSV export into a local OA Curator workspace.

## What You Need

Use a CAF CSV export file from disk. CAF exposes this from:

<https://www.comicartfans.com/my/galleryreport.asp>

On that page, look for the link labeled "CLICK HERE TO DOWNLOAD A CSV FILE OF YOUR CAF GALLERY ARTWORKS".

The CSV import does not crawl ComicArtFans pages and does not download images. It reads the CSV rows, uses the CSV image URL to identify CAF Collection/Gallery values and dedupe records, and imports the metadata available in the file.

ComicArtFans terms are at [https://www.comicartfans.com/termsconditions.asp](https://www.comicartfans.com/termsconditions.asp) and prohibit scraping. OA Curator imports CAF data only through the CSV export file you download from your own account.

## What Gets Created

The import can create:

- An OA Curator Collection with the CAF `GCat` value.
- Galleries that map to CAF Gallery Rooms when the CSV image URLs include a `GSub`/subcat value.
- Artwork records with CAF metadata available in the CSV.

After import, review the Collection and attach local JPG, PNG, or TIFF preservation scans, plus any supporting files you want to keep with the Artwork. CAF CSV import does not automatically find matching local originals on your computer.

If a Collection is already open, CAF import merges into that Collection. Close the open Collection first if you want the import to create a new local Collection.

## Expected Limits

CAF CSV export data can be incomplete. Known limitations include:

- It does not include CAF artwork page URLs.
- It does not include CAF piece IDs directly.
- It does not include alternate or additional artwork image URLs.
- It does not include CAF Collection or Gallery names. OA Curator creates placeholder Collection and Gallery names from the CAF IDs in the CSV.
- Artist names are exported without CAF artist role IDs.
- It includes private purchase price, estimated value, purchase date, and personal notes, but it does not appear to include provenance.
- Embedded HTML in CAF description fields can prevent some pieces from appearing in the CAF CSV export.

If a downloaded CAF CSV is missing expected rows, check whether the missing pieces have embedded HTML in their CAF description fields. As a practical workaround, remove embedded HTML from those description fields in CAF and download a fresh CSV.

The CSV image URL remains useful as a stable CAF-derived identity when no CAF artwork page URL is present, but OA Curator treats it as metadata only. OA Curator does not download or scrape images from those URLs.

When CAF data does not uniquely identify an existing Artwork, OA Curator may ask you to reconcile the CSV row with a local Artwork or import it as new.

After a CAF CSV import, OA Curator can offer a missing-item report showing tracked CAF rows that appear absent from the current local import result.

## Publishing Back To CAF

CAF does not currently provide a bulk import path for adding a prepared Collection back to CAF. OA Curator can prepare local records, PNG derivatives, public metadata, and OAA archives, but CAF publishing still happens through CAF's own manual artwork entry workflow.

If CAF adds a supported bulk import path, OA Curator can target it. OAA is a good interchange target for that request because it can carry Collection, Gallery, Artwork, gallery-site metadata, and optional embedded files in a single collector-controlled archive. You can ask CAF for OAA or CSV bulk import support through the [CAF contact page](https://www.comicartfans.com/contact.asp).
