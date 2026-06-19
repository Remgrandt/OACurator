<!-- Copyright (c) 2026 Remgrandt Works. All rights reserved. -->

# Application Interface

The OA Curator application interface is the main screen for browsing and editing a Collection.

## Title Bar And Menus

The top of the app contains a Visual Studio-style title bar, menu bar, and toolbar.

The **File** menu includes workspace actions such as:

- New Collection
- Open Collection
- Close Collection, when a Collection is open
- Import CAF Collection
- Import SNIKT.com Collection
- Import OAA Archive
- Export to Raremarq, when a Collection is open
- Export OAA Archive, when a Collection is open
- New Gallery
- Open Gallery

Raremarq currently does not provide a bulk export file, so no Raremarq import command is shown.

Commands that require extra information open a dialog. For example, New Collection, New Gallery, Import CAF Collection, Import SNIKT.com Collection, Import OAA Archive, Export to Raremarq, and Export OAA Archive collect their input in a focused dialog.

The **Preferences** menu controls application defaults such as theme, startup behavior, attachment mode, PNG export settings, gallery site focus, Artwork ID label style, and the default workspace root.

The **Help** menu includes the user guide, app information, license information, and **Check for Updates**. Official desktop builds check signed OA Curator update metadata and ask before installing an available update.

The toolbar contains common Collection and Gallery actions, plus the theme toggle.

## Explorer

The explorer shows the open Collection hierarchy:

- Collection
- Gallery
- Artwork
- Files

Use the explorer when you need to move between Collection, Gallery, and Artwork context. The search box filters the tree as you type.

Item actions appear near the selected or hovered item. Context menus provide actions such as rename or delete where available.

## Preview And Carousel

The preview area shows the selected Artwork image. The carousel below it shows attached images and lets you choose which one is selected.

If an Artwork does not yet have an attached image, it can still appear as a catalog record, but it will not have a preview image until a supported file is attached.

Large source images are displayed through cached previews where practical, so browsing does not need to load the full original scan for every thumbnail.

## Selected File Details

Selected file details show the attached file record, format, path, and file role. Image-only facts such as dimensions and DPI appear when they are available.

Use **File role** to mark what a file represents, such as Raw Scan, Detail, Verso, Reference, Basic, or Premium.

PNG export controls appear only when the selected file can be rendered as JPG, PNG, or TIFF.

## Properties

Properties are the editable fields for the selected Artwork, Collection, or Gallery. Artwork properties include gallery-site links, artist credits, public metadata, SNIKT.com fields, and private collector fields.

Gallery site filters let you focus the property list on CAF, SNIKT.com, Raremarq, or all compatible fields.

Changes are written to local records and the selected Artwork manifest. Normal metadata edits do not rewrite Collection or Gallery manifests. Original image pixels are not modified by metadata edits.

## Status Feedback

The application reports the result of actions such as opening a Collection, importing gallery site data, attaching images, and exporting PNG derivatives. If a command fails, read the status message first; it usually identifies whether the issue is a missing file, unsupported input, or external import problem.
