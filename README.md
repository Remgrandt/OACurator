<p align="center">
  <img src="docs/user/assets/oac-logo.svg" alt="OA Curator logo" width="220">
</p>

# OA Curator

Local-first desktop catalog for original art collectors.

OA Curator helps collectors keep a private, durable catalog of original art scans, metadata, gallery-site links, notes, and export-ready files on their own computer. It is built to complement community gallery sites without making a collection depend on any one website.

## License

OA Curator is source-available freeware, not open source. You may view, build, install, and use unmodified copies under the terms in `LICENSE`. Redistribution, modified builds, and third-party code contributions are not permitted unless Remgrandt Games LLC authorizes them in writing.

## What It Does Today

- Create and open local Collections, Galleries, and Artworks.
- Attach JPG, PNG, and TIFF scans by copying them into a Collection or linking to their existing location.
- Attach generic supporting files such as PDF, PSD, HEIC, AVIF, WebP, ZIP, or documents without treating them as renderable images.
- Generate cached thumbnails and previews without modifying original files.
- Import ComicArtFans and SNIKT.com CSV metadata.
- Import and export OAA archives for portable collector-owned data.
- Edit public metadata, gallery-site URLs, artist credits, and private collector fields.
- Generate Basic and Premium PNG derivatives for web workflows.
- Export Raremarq bulk-upload CSV files.
- Open SNIKT.com upload-prefill URLs for browser-assisted publishing.

## What It Does Not Do

- No cloud sync or hosted collection storage.
- No mobile app.
- No direct ComicArtFans upload automation.
- No ComicArtFans, SNIKT.com, or Raremarq site scraping.
- No automatic artist assignment from folder names.
- No editing original image pixels during cataloging, viewing, thumbnailing, or export preparation.
- No rendering PDF, PSD, HEIC, AVIF, WebP, or similar attachment formats as images.

## Safety Model

OA Curator is local-first. Core cataloging, browsing, metadata editing, file organization, and export preparation are designed to work offline.

Original scans are source assets. Thumbnails, previews, and PNG exports are separate generated files. The app should not overwrite original image pixels.

Purchase price, estimated value, purchase date, provenance, and personal notes are private collector fields. Public sharing workflows should exclude private fields by default unless the collector deliberately includes them.

Move and rename workflows are expected to use preview, validation, no-overwrite checks, explicit confirmation, and operation logging before physical files are changed.

## Updates

Official desktop builds can check for signed OA Curator updates from the Help menu. Updates are not installed silently; the app asks before downloading and installing, and Windows builds close the app to finish installation.

## Imports, Exports, and Gallery Site Workflows

OA Curator uses the Collection, Gallery, and Artwork structure. It can store gallery-site IDs and links for ComicArtFans, SNIKT.com, and Raremarq while keeping the local OAC identity as the stable record.

ComicArtFans and SNIKT.com support are CSV-based import and local curation workflows. SNIKT.com upload-prefill opens a browser URL with metadata filled in where supported, but the user still chooses the image file on SNIKT.com.

Raremarq support is centered on bulk-upload CSV export. OA Curator can write a local Raremarq CSV using selected scope and URL-fill options. It does not crawl Raremarq or directly upload artwork records.

<img align="right" src="docs/user/assets/oaa-logo.svg" alt="Original Art Archive logo" width="96">

OAA is the portable archive path for collector-owned data. OAA import/export can carry OA Curator metadata, gallery-site links, and optionally artwork files.

The OAA logo is used only to truthfully describe OA Curator's compatibility with the Original Art Archive Format. It does not imply separate certification, endorsement, or maintenance by the OAA project.

## Development

Prerequisites:

- Node.js and npm.
- Rust MSVC toolchain on Windows.
- Windows: Microsoft C++ Build Tools and WebView2 for Tauri development.
- Python with MkDocs dependencies from `requirements-docs.txt` when building user docs.

Install dependencies:

```powershell
npm install
python -m pip install -r requirements-docs.txt
```

Fast local verification:

```powershell
npm run check:fast
```

Full verification:

```powershell
npm run check:full
```

Run the desktop app in development:

```powershell
npm run tauri dev
```

The npm package is marked private because OA Curator is distributed as a desktop app, not as an npm package.

## Documentation and Notices

User documentation lives in `docs/user` and builds into the app's offline help with MkDocs.

Project attribution notes for bundled visual/theme/UI resources live in `ATTRIBUTIONS.md`. OA Curator's own license is in `LICENSE`.
