<!-- Copyright (c) 2026 Remgrandt Works. All rights reserved. -->

# Metadata

Metadata makes OA Curator more useful than folders and filenames. It helps you search, browse, prepare supported gallery site workflows, and export portable OAA archives.

Editing Artwork metadata updates the local catalog and that Artwork's `.oaartwork` manifest. It does not rewrite Collection or Gallery manifests unless a separate workspace operation changes Collection or Gallery membership.

## Core Artwork Fields

These fields describe the Artwork itself:

- **Title:** the display title for the piece.
- **Description:** notes or description that can travel with public gallery site metadata.
- **Artist:** one or more creator names.
- **Media type:** the physical or artistic medium, such as ink, watercolor, paint, or mixed media.
- **Artwork type:** the object type, such as cover, interior page, pinup, commission, prelim, or animation art.
- **Publication status:** whether the Artwork is tracked as published or unpublished.
- **For sale status:** sale-status text for publishing or gallery site workflows.
- **ComicArtFans URL:** the public CAF artwork URL when one exists.
- **SNIKT.com URL:** the public SNIKT.com artwork URL when one exists.
- **Raremarq URL:** the public Raremarq artwork URL when one exists.
- **Generic URL:** any additional reference URL you want to track for the Artwork. The Raremarq bulk-upload export wizard can use this value for the CSV `primary_image_url` column when populated.

When a CAF URL includes a `piece` ID, a SNIKT.com URL includes an image ID, or a Raremarq URL includes a piece slug, OA Curator stores that gallery-site ID alongside the local Artwork ID.

Raremarq URL fields are link/reference fields. OA Curator can also export the open Collection to Raremarq's bulk-upload CSV format from the File menu. The export wizard can use Generic URL values, leave URL fields blank, or upload temporary image copies to tmpfiles.org.

## CAF-Oriented Fields

OA Curator includes CAF-friendly fields such as Media Type, Artwork Type, Publication Status, Active, Illustration Exchange, and IX for sale.

These fields help preserve CAF-compatible metadata locally. CAF CSV import does not expose every CAF field, so some CAF-compatible fields are manually entered local metadata.

## SNIKT.com Fields

SNIKT.com metadata can include:

- Art type
- Publisher
- Series title
- Issue number
- Page number
- Year
- Character
- Animation subcategory, studio, episode number, and episode title
- Published date
- Strip title
- Sunday-strip flag
- Other notes
- Tags
- NSFW
- For sale
- Sale price
- Open to offers

Some SNIKT.com fields are only relevant for certain art types. For example, animation fields matter for animation art, while issue and page fields matter for comic interiors.

## Raremarq Fields

OAC's Raremarq support includes local gallery-site links, IDs, and bulk-upload CSV export. Raremarq currently does not provide a bulk export file.

## Private Collector Fields

These fields are for your local records:

- **Purchase price**
- **Estimated value**
- **Purchase date**
- **Provenance**
- **Personal notes**

Private collector fields should stay out of public sharing unless you deliberately include them. OAA archives are portable collection archives, so treat them as private if they include private metadata or image files.

## Character Metadata

OAC supports character tagging through gallery site metadata. SNIKT.com is the gallery site that currently exposes character fields in OA Curator, so character entries are stored with SNIKT.com metadata and upload-prefill preparation.

## Source Filters

The Artwork Properties panel includes gallery site filter buttons for:

- CAF
- SNIKT.com
- Raremarq

Disable a gallery site filter when you want to hide fields that only apply to that external site. OAC-native fields stay visible.

## Terms And Consistency

Media and format values are most useful when entered consistently. Use the same spelling and capitalization for repeated values so they are easier to scan and reuse.
