# Importing OAA Archives

![Original Art Archive logo](assets/oaa-logo.svg){ .oaa-doc-logo }

OA Curator can import an `.oaa` Original Art Archive package from disk.

OAA is an open interchange format for original art archives. It stores Collection, Gallery, Artwork, file, and external-site metadata in plain text manifests inside a ZIP-based package.

## Import Behavior

If a Collection is already open, OAA import merges into the open Collection. Close the open Collection first if you want the OAA import to create a new local Collection.

If no Collection is open, OAA import can create a new local Collection from the archive.

## Files

An OAA archive may include embedded artwork files, or it may contain only metadata and file records. If embedded files are present, OA Curator imports those files into the local Collection structure.

OAA can represent OA Curator, CAF, SNIKT.com, Raremarq, and other external-site metadata through base fields and extension blocks. When OA Curator imports OAA, it parses OA Curator-native data and the CAF, SNIKT.com, and Raremarq gallery site data it knows about.

<p class="oaa-mark-note">The OAA logo is used here only to describe OA Curator's compatibility with the Original Art Archive Format. It does not imply separate certification, endorsement, or maintenance by the OAA project.</p>
