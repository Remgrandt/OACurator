# Portable Data And OAA

![Original Art Archive logo](assets/oaa-logo.svg){ .oaa-doc-logo }

OA Curator is built around a simple promise: your collection data should belong to you.

A website can be useful. A desktop app can be useful. But your catalog should not depend on any single service staying friendly, complete, online, or unchanged forever.

## What OAA Is

**OAA** means **Original Art Archive**. It is a portable archive format for original art collection data and files.

In OA Curator, OAA is the format you use when you want to move a Collection as collector-controlled data instead of rebuilding everything by hand from a website, screenshots, or folder names.

## What OAA Is For

Use OAA when you want to:

- Back up a Collection in a portable form.
- Move a Collection between OA Curator installs.
- Prepare for future tools that understand the OAA format.
- Keep your catalog independent from any single website.

## Importing OAA

Use **File > Import OAA Archive** and choose the archive file. OA Curator will ask for a destination folder when it needs one.

Imported archives can include Collection data, Galleries, Artworks, metadata, external-site links, and files.

## Opening Older Collection Folders

When you open an older Collection folder, OA Curator automatically upgrades its referenced Collection, Gallery, and Artwork manifests together to OAA 1.0. This also completes folders that contain a mixture of 0.1 and 1.0 manifests. Collections already using 1.0 are not rewritten just because you open them.

The upgrade preserves metadata, private fields, extensions, and original scans. It checks the complete referenced manifest set before replacing any manifest. Unreferenced artwork folders are left alone. Missing scans remain available for the usual missing-file checks; a missing manifest, unsupported version, or incompatible metadata stops the upgrade and explains the problem.

Saved attachment sizes can become out of date when local files change. Those differences do not block the folder upgrade. Exporting a new OAA archive records the sizes of the files actually included and validates the archive.

Before changing anything, OA Curator saves the original manifest contents in a `.oaa-1.0-backup-….json` file in the Collection folder. Keep this backup private: it contains the same private metadata as your Collection. If an upgrade is interrupted, opening the Collection again resumes it. A write failure attempts to restore the original manifests; if recovery cannot finish, opening stops and the backup is retained. Recovery will not overwrite manifests that have been edited separately since the upgrade began.

This upgrades the local folder. It does not modify existing `.oaa` archive files. Use **File > Export OAA Archive** to create a new archive with archive-level validation.

## Exporting OAA

Use **File > Export OAA Archive** when a Collection is open.

The export wizard lets you choose whether to include artwork files and private collector metadata. Treat an archive with private metadata or files as a full collection archive, not as a public-only export.

## Privacy

OAA is designed for portability, not automatic public publishing. An OAA archive may include private collector metadata and image files depending on the export options you choose.

Keep OAA archives somewhere you trust, and only send them to people or services that should receive your collection data.

## Full Specification

This guide only explains what users need to know to use OAA in OA Curator.

For the full technical format, see [Original Art Archive OAA Specification](https://original-art-archive.github.io/oaa-spec/).

<p class="oaa-mark-note">The OAA logo is used here only to describe OA Curator's compatibility with the Original Art Archive Format. It does not imply separate certification, endorsement, or maintenance by the OAA project.</p>
