# OA Curator OAA 1.0 implementation scope

This document describes OA Curator's OAA 1.0 support and implementation limits.
It imposes no OA Curator requirement on the OAA standard.

## Release target

- Specification: [oaa-spec v1.0.0](https://github.com/Original-Art-Archive/oaa-spec/tree/v1.0.0), commit `84942f6ec037cdd6b5bf9ba347f326e1bca56539`.
- Reference validator: [oaa-validator v1.0.0](https://github.com/Original-Art-Archive/oaa-validator/tree/v1.0.0), commit `90f91b4fcf8010166c763a8650e67981361ac93c`.
- Writers emit manifest `schema_version: "1.0"`. Readers also retain the historical 0.1 path. Existing local Collections do not undergo a bulk rewrite; normal edits write current manifests.

## Reader and writer behavior

The implementation targets the Structural Reader, Metadata Reader, and Conforming Writer requirements within the capacities below. These are evidence-backed implementation targets, not third-party certification. No lossless preservation claim is made.

Collection references determine imported records. Gallery references determine membership. An artwork can exist without any gallery. Import into an existing Collection allocates separate local artwork identities even when external IDs match. Unknown provider links and repeated associations are retained. Base metadata takes precedence over provider data, and display-only artist names and custom credit roles are accepted.

Archive paths are checked independently of filesystem capabilities. Readers validate Store and Deflate payloads, including ZIP64, and stream all content (including extras) with actual output bounds, CRC checks, and compressed-stream completion checks. The central directory and root manifest are bounded before general entry/manifest allocation. Duplicate ZIP names, duplicate decoded JSON keys, special entries, overlapping entries, and file/directory conflicts are refused. Unsupported root encoding or version stops without claiming validity. Invalidity, unsupported processing, capacity, I/O failure, and destination restrictions are separate outcomes.

New-Collection extraction preflights referenced paths for case/Unicode collisions and Windows-special components on all platforms. Existing-Collection import safely assigns new file names. Both paths check existing ancestor symlinks/junctions/reparse points and use exclusive creation for payload files. This assumes another process is not concurrently replacing destination directories; it is not a hostile multi-process filesystem sandbox. Original scans and input archives are not modified.

Exports use current file lengths and validate the finished temporary archive before installing the final output. Overwriting a final archive requires the existing explicit overwrite choice.

## Declared capacities

| Limit | Default |
| --- | --- |
| Archive bytes | 2 GiB |
| Total expanded content | 4 GiB |
| Individual entry | 1 GiB |
| Entries | 100,000 |
| Central directory | 16 MiB |
| Manifest bytes | 10 MiB |
| JSON container depth | 100 |
| Imported file integer fields | signed 64-bit |
| Decimal numeric storage | finite IEEE-754 double |

Exceeding a bound stops processing and does not prove invalidity. `valid: true` is usable only together with `complete: true`. Invalid archives can have incomplete validation after an early safety failure. Multi-disk ZIP and root encodings other than Store/Deflate are unsupported.

## Privacy and preservation

Private metadata is excluded by default. That choice also removes all extension blocks and supporting attachments; only selected JPG/PNG/TIFF image files are eligible. Including private metadata opts into a private backup with retained extension data and attachments. `is_public` does not authorize disclosure. Copied image bytes are unchanged and can still contain embedded metadata or private visible content.

Retained scope: base catalog metadata, external links (including unknown providers), collection/gallery/artwork/public/private/file extension blocks, and referenced embedded file bytes, subject to application edits and export options. Imported supporting/raw classifications and declared file media types are retained separately from rendering support.

Limits: local IDs and paths change on merge; nullable/absent fields and numeric representations may normalize; unknown optional fields and extensions on references or artist credits may be lost on rewrite; duplicate identical artist-credit rows can collapse in the catalog; unreferenced extras are skipped. Known OA Curator extension keys can be regenerated as local IDs change. Import messages and user help disclose preservation limits. Keep the input archive for exact preservation. JPG, PNG, and TIFF remain the only renderable formats; other files stay inert attachments.

## Verification

Run `npm run check:release` to verify the runtime lock, strict user-guide build,
frontend types and production bundle, Rust formatting and Clippy, and release workflows.

Maintainer regression tests cover independently constructed OAA 1.0 archives,
legacy 0.1 import, both import destinations, export privacy, retained metadata,
ZIP64 and malformed ZIP streams, capacity outcomes, and destination path safety.
Exported archives are also checked with the pinned reference validator above.

The verification evidence is from Windows. macOS/Linux extraction and packaging
require their platform checks; archive validity alone does not establish those claims.
