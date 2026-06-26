# Exporting OAA Archives

![Original Art Archive logo](assets/oaa-logo.svg){ .oaa-doc-logo }

Use **File > Export OAA Archive** to write the open Collection as an `.oaa` package.

OAA is the preferred OA Curator backup and interchange format. It is an open ZIP-based package with plain text manifests, so it is readable, editable, and suitable for long-term preservation.

## Export Options

The export wizard lets you choose whether to include artwork files and private collector metadata.

- **Include artwork files** creates a self-contained archive. Linked files are copied into the OAA package as OAA-local embedded files. The open Collection is not changed.
- **Metadata only** writes Collection, Gallery, Artwork, and external-site metadata without embedding image files.
- **Include private collector metadata** includes purchase, value, provenance, and personal note fields. Leave this enabled for a private backup. Turn it off before making an archive for public sharing.

The wizard stays open and shows progress until the archive is finished.

## External Site Data

OAA can carry OA Curator-native metadata plus CAF, SNIKT.com, and Raremarq gallery site data through extension fields.

## Backup Advice

An OAA archive is useful as a local backup, but it should not be your only copy. Keep an offsite backup of important original art scans and exported archives.

If you want gallery sites to support OAA bulk import, consider asking them:

- Raremarq form: <https://forms.gle/ri5ATNyqKCUkG8iU9>
- CAF contact: <https://www.comicartfans.com/contact.asp>
- SNIKT.com: <info@snikt.com>

<p class="oaa-mark-note">The OAA logo is used here only to describe OA Curator's compatibility with the Original Art Archive Format. It does not imply separate certification, endorsement, or maintenance by the OAA project.</p>
