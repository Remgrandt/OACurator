# Importing OAA Archives

![Original Art Archive logo](assets/oaa-logo.svg){ .oaa-doc-logo }

OA Curator imports OAA 1.0 archives and retains support for legacy 0.1 archives. New exports use 1.0. Opening an existing local Collection does not require a bulk conversion.

OAA is an open interchange format for original art archives. It stores Collection, Gallery, Artwork, file, and external-site metadata in plain text manifests inside a ZIP-based package.

## Import Behavior

If a Collection is already open, OAA import adds separate Artwork records to the open Collection. Matching gallery-site IDs do not replace or merge existing Artworks. Close the open Collection first if you want the OAA import to create a new local Collection.

If no Collection is open, OAA import can create a new local Collection from the archive. Artwork without Gallery membership stays ungrouped. Gallery membership comes from the archive; OA Curator does not invent a fallback Gallery.

## Files

An OAA archive may include embedded artwork files, or it may contain metadata with an empty file list. Every listed file must be present in the archive. OA Curator checks the archive before importing and copies referenced files into the local Collection. Unreferenced extras are skipped. Original archives are left unchanged.

OAA can represent OA Curator, CAF, SNIKT.com, Raremarq, and other external-site metadata through base fields and extension blocks. When OA Curator imports OAA, it parses OA Curator-native data and the CAF, SNIKT.com, and Raremarq gallery site data it knows about. It imports base metadata and retains links from unrecognized external sites. Artist display names and custom credit roles are accepted. Gallery-site data does not override base metadata. Unknown optional fields and extensions attached to references or artist credits may not survive later edits or exports. Keep the original archive when exact preservation matters.

## Import Limits

An import may stop because the archive is invalid, its version is unsupported, it exceeds OA Curator's processing limits, or its paths cannot be represented safely at the destination. These are different outcomes. A stopped check does not certify the archive as valid.

Current limits are 2 GiB per archive, 4 GiB of expanded content, 1 GiB per file, 100,000 ZIP entries, 10 MiB per manifest, and 100 levels of JSON nesting. Windows reserved names and paths that collide after case or Unicode normalization are refused for new Collections. Choose a destination without symbolic links or filesystem junctions. Importing into an existing Collection assigns new local names safely.

<p class="oaa-mark-note">The OAA logo is used here only to describe OA Curator's compatibility with the Original Art Archive Format. It does not imply separate certification, endorsement, or maintenance by the OAA project.</p>
