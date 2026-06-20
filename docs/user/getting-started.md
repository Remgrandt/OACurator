<!-- Copyright (c) 2026 Remgrandt Works. All rights reserved. -->

# Getting Started

This page walks through a basic OA Curator workflow from an empty workspace to a cataloged artwork with an export derivative.

## Create Or Open A Collection

Use **New Collection** to create a local Collection workspace, or **Open Collection** to load an existing one.

A Collection is the top-level container for your catalog. It can store gallery-site IDs for ComicArtFans, SNIKT.com, and Raremarq when the Collection is aligned to those services, but it can also be completely local.

## Create A Gallery

Use **New Gallery** inside the open Collection.

A Gallery is a room or grouping within the Collection. Galleries can store gallery-site IDs when they map to ComicArtFans or Raremarq groupings. SNIKT.com imports use the Collection's SNIKT.com ID as their source context, so Gallery setup does not require a separate SNIKT.com ID.

## Create An Artwork

Use **New Artwork** in the selected Gallery.

OA Curator assigns locally created Artworks an ID such as `OAC-00001`. If you later add ComicArtFans, SNIKT.com, or Raremarq URLs, OA Curator stores those gallery-site IDs separately. The visible Artwork ID can be changed in **Preferences** to favor OAC, CAF, SNIKT, or Raremarq labels.

## Assign An Artwork To Another Gallery

An Artwork can belong to more than one Gallery. Open the Artwork, find **Galleries** in the Artwork Properties panel, and use the Gallery add button to choose another Gallery.

The same **Galleries** section shows all current Gallery assignments. Use the Gallery remove button beside a Gallery to remove that assignment. OA Curator disables removal when the Artwork belongs to only one Gallery.

## Attach A File Or Image

Open the Artwork and use the attachment controls to add a file. JPG, PNG, and TIFF attachments can be rendered for previews and PNG exports. Other supported attachments remain file records.

When you attach a file, choose one attachment mode:

- **Copy into the Collection** stores a Collection-managed copy for that Artwork.
- **Link to existing location** records the file path and leaves the file where it already is.

Choose copy when you want the Collection to own its own copy of the file. Choose link when you want OA Curator to reference an existing scan or attachment without moving or duplicating it.

## Edit Metadata

Open the Artwork detail view and update the fields you know:

- Title
- Artist
- Media
- Format
- ComicArtFans URL
- SNIKT.com URL
- Raremarq URL
- Generic URL
- Purchase price
- Provenance

Purchase price and provenance are private collector fields. They are useful in your local catalog but should not appear in public exports by default.

## Export A PNG Derivative

Use the PNG export action from the Artwork detail view. OA Curator creates a PNG derivative from the attached source image and records it with the Artwork.

The derivative is separate from the original scan. Export preparation should not overwrite or edit the source file.

## Import From ComicArtFans

If you already have a ComicArtFans CSV export, use **Import CAF Collection** and select the CSV file.

The import creates local Collection, Gallery, and Artwork records from CAF CSV metadata. It does not download images; after import, review the results and attach local preservation scans as needed.

## Import From SNIKT.com

If you have a SNIKT.com CSV export, use **Import SNIKT.com Collection** and select the CSV file.

The import reads local CSV data only. It does not crawl SNIKT.com pages and does not download images.

## Import From Raremarq

Raremarq currently does not provide a bulk export file. OA Curator can still store Raremarq URLs and export a Raremarq bulk-upload CSV.

## Import Or Export OAA

Use **Import OAA Archive** to load an `.oaa` archive from disk.

Use **Export OAA Archive** to write the open Collection as an `.oaa` package. The export wizard lets you choose whether to include artwork files.

## Export To Raremarq

Use **File > Export to Raremarq** to create a Raremarq bulk-upload CSV for the open Collection.

Rows need a value in Raremarq's `primary_image_url` column. The export wizard can use the Artwork Generic URL field, leave URL fields blank for manual repair, or upload temporary obfuscated image copies to tmpfiles.org with a 24-hour expiry.
