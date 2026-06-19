<!-- Copyright (c) 2026 Remgrandt Works. All rights reserved. -->

# Troubleshooting

Use this page when something in OA Curator does not behave as expected.

## An Attached Image Does Not Preview

Check that the file still exists at the recorded path. Linked files can be moved outside OA Curator, which breaks the link until the Artwork is updated.

Also confirm the file is a supported image type: JPG, PNG, or TIFF.

## A TIFF Opens Slowly

Large TIFF files can be expensive to decode. OA Curator generates cached previews to make browsing smoother, but the first preview generation can still take longer than JPG or PNG.

## A PNG Export Is Missing

Open the Artwork detail view and check the image details. Confirm that the source file is still available and that the derivative appears in the Artwork's file list.

If an export action reports an error, read the status message before retrying. It should indicate whether the source file, destination path, or saving step failed.

## CAF CSV Import Finds Fewer Artworks Than Expected

CAF CSV import reads the rows available in the CSV export. Missing Artworks may be absent from the export or missing the image URL data OA Curator uses to identify the CAF Collection/Gallery context.

Check the CSV file directly and confirm the missing Artwork appears in it.

If the CSV itself is missing expected rows or field data, check whether the affected CAF descriptions contain embedded HTML. If they do, remove the embedded HTML from those CAF description fields and download the CSV again.

## SNIKT.com CSV Import Does Not Match Everything

The current SNIKT.com CSV export does not provide a stable per-artwork ID. OA Curator matches rows by title and created date when possible. Ambiguous or changed rows may need review.
