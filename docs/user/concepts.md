<!-- Copyright (c) 2026 Remgrandt Works. All rights reserved. -->

# Core Concepts

OA Curator uses collector-friendly records and file-safe image handling. You do not need to understand the internal storage model to use it, but these terms appear throughout the app.

## Collection

A **Collection** is the top-level OA Curator workspace. It contains Galleries and Artworks, and it can store gallery-site IDs for ComicArtFans, SNIKT.com, and Raremarq.

A Collection is saved locally. It also has a portable `.oacollection` manifest file so the Collection can be inspected outside OA Curator and carried to other tools.

## Gallery

A **Gallery** is a room or grouping inside a Collection.

A Gallery can carry gallery-site mapping when it is connected to an outside collection service. ComicArtFans Galleries can store CAF Gallery Room `GSub` values, Raremarq Galleries can store Raremarq gallery IDs, and SNIKT.com Gallery tracking can inherit the Collection's SNIKT.com ID.

Galleries have portable `.oagallery` manifest files. A Gallery manifest lists Artwork membership by local Artwork ID; it does not duplicate mutable Artwork metadata such as title, artist, file count, or format.

## Artwork

An **Artwork** is the catalog record for one original art piece. It can include:

- Title and description.
- Artist credits.
- Media, artwork type, and publication status.
- Gallery site URLs and IDs.
- Private collector fields.
- Attached image files.
- Cached previews.
- PNG exports.

Locally created Artworks use neutral IDs such as `OAC-00001`. Gallery-site IDs are stored separately, so you can prefer CAF, SNIKT.com, or Raremarq labels without losing the OAC identity.

Artworks have portable `.oaartwork` manifest files. The Collection manifest stores the relative path to each Artwork manifest, which lets one Artwork belong to more than one Gallery without duplicating the Artwork folder.

## Original Files

Original files are your source scans or photos. OA Curator supports JPG, PNG, and TIFF.

TIFF is useful for preservation scans. PNG is useful for web-ready derivatives and gallery site workflows.

## Copy Versus Link

When you attach a file, choose how OA Curator should refer to it:

- **Copy into the Collection** creates a Collection-managed copy.
- **Link to existing location** keeps the file where it already is and records its path.

Copied files make a Collection easier to move as a unit. Linked files avoid duplicates when you already maintain a separate scan library.

## Thumbnails And Previews

OA Curator generates cached thumbnails and previews so the application can browse large images efficiently. These cached files are derived from the original and should not replace it.

## PNG Exports

A PNG derivative is an export-ready image created from an attached source file. It belongs to the Artwork record but is not the original scan.

A PNG export can be useful for web workflows such as ComicArtFans preparation, sharing, and review.

## Gallery Site Links

OA Curator can store links and gallery-site IDs for:

- ComicArtFans
- SNIKT.com
- Raremarq

Gallery site links help you connect your local catalog to public pages and supported gallery site workflows without making those services the only home for your data.

## Gallery Site Mapping

OA Curator uses common collector terms for local organization and stores gallery-site-specific IDs alongside them when those IDs are known.

- **Collection** is the top-level catalog workspace. It can store CAF Collection `GCat`, SNIKT.com user IDs, and Raremarq user slugs.
- **Gallery** is a grouping within a Collection. It can store CAF Gallery Room `GSub` values and Raremarq gallery IDs, while SNIKT.com Gallery tracking can inherit the Collection's SNIKT.com ID.
- **Artwork** is an individual original art piece. It can store CAF artwork URLs, SNIKT.com artwork IDs, and Raremarq piece slugs.

## OAA Archives

OAA stands for **Original Art Archive**. It is the portable archive format for original art collection data.

Use OAA import/export when you want your Collection to travel as collector-controlled data.

For the full format, see [Original-Art-Archive/oaa-spec](https://github.com/Original-Art-Archive/oaa-spec).
