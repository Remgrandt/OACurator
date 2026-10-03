# Exporting OAA Archives

![Original Art Archive logo](assets/oaa-logo.svg){ .oaa-doc-logo }

Use **File > Export OAA Archive** to write the open Collection as an OAA 1.0 `.oaa` package.

OAA is the preferred OA Curator backup and interchange format. It is an open ZIP-based package with plain text manifests, so it is readable, editable, and suitable for long-term preservation.

## Export Options

The export wizard lets you choose whether to include artwork files and private collector metadata.

- **Include artwork files** embeds the files allowed by your privacy choice. Linked files are copied into the OAA package as OAA-local embedded files. The open Collection is not changed.
- **Metadata only** writes Collection, Gallery, Artwork, and external-site metadata without embedding image files.
- **Include private collector metadata** is off by default. Off excludes purchase, value, provenance, personal notes, all extension data, and supporting attachments. Turn it on for a private backup that includes those fields, retained gallery-site data, and all selected attachments.

The wizard stays open and shows progress until the archive is finished. OA Curator validates the completed archive before placing it at the final path. Existing output requires overwrite approval.

Included JPG, PNG, and TIFF files are copied unchanged. Their visible contents and embedded metadata may still be private. Review the selected Collection, descriptions, external links, and images before sharing. An Artwork marked public does not authorize disclosure of its private fields or attachments.

## External Site Data

Private backups retain imported links, including unrecognized external sites, and supported extension blocks. Export is not a byte-for-byte copy of an imported archive: local IDs, paths, and some values are normalized; unreferenced extras, unknown optional fields, and reference or artist-credit extensions may be lost. Keep the original archive for exact preservation.

## Backup Advice

An OAA archive is useful as a local backup, but it should not be your only copy. Keep an offsite backup of important original art scans and exported archives.

If you want gallery sites to support OAA bulk import, consider asking them:

- Raremarq form: <https://forms.gle/ri5ATNyqKCUkG8iU9>
- CAF contact: <https://www.comicartfans.com/contact.asp>
- SNIKT.com: <info@snikt.com>

<p class="oaa-mark-note">The OAA logo is used here only to describe OA Curator's compatibility with the Original Art Archive Format. It does not imply separate certification, endorsement, or maintenance by the OAA project.</p>
