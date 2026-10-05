# OA Curator OAA 1.0 implementation scope

This document describes OA Curator's OAA 1.0 support and implementation limits.
It imposes no OA Curator requirement on the OAA standard.

## Release target

- Specification: [oaa-spec v1.0.0](https://github.com/Original-Art-Archive/oaa-spec/tree/v1.0.0), commit `84942f6ec037cdd6b5bf9ba347f326e1bca56539`.
- Reference validator: [oaa-validator v1.0.0](https://github.com/Original-Art-Archive/oaa-validator/tree/v1.0.0), commit `90f91b4fcf8010166c763a8650e67981361ac93c`.
- Writers emit manifest `schema_version: "1.0"`. Readers also retain the historical 0.1 path. Opening a legacy or mixed-version local Collection automatically upgrades all referenced manifests together before catalog hydration. Already-current manifests are not rewritten on open.

## Local Collection upgrade

Opening upgrades only explicitly supported 0.1 documents, including older children under a 1.0 root. It preflights the complete referenced set using the shared 1.0 metadata and cross-reference rules, verifies that the app can deserialize the result, and checks sizes of present attachments. It refuses unsupported versions, duplicate JSON members, unsafe or redirected paths, missing manifests, inconsistent references, and incompatible metadata before mutation. Missing media remains a catalog consistency issue; this metadata upgrade is not a claim of complete archive validation.

For compatible documents, only the top-level version token changes. All other source bytes, including unknown fields, numeric spelling, extensions, private data, IDs, membership, and whitespace, are retained. No unreferenced records are discovered or rewritten, and media files are never modified. Backup journals contain the exact before/after manifest text, are synced before installation, and remain in `.oaa-1.0-backup-*.json` files in the Collection folder. These files contain private metadata and are not included in OA Curator's archive exports.

Each replacement uses the existing staged atomic-file writer, with the collection root installed last. An on-disk pending pointer supports restart recovery; write errors attempt rollback. Recovery requires every file to match its recorded before or after contents and refuses external edits. This is recoverable multi-file replacement, not filesystem-wide atomicity or protection from another process racing a write. Upgrade preflight is limited to 100,000 manifests, 10 MiB per manifest, 64 MiB of total source metadata, and the existing JSON depth/numeric capacities. ZIP input files are unchanged.

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

Local upgrade regression coverage includes automatic legacy/mixed-version upgrades,
exact preservation and backups, current-version no-op, invalid-input refusal,
interrupted recovery and external-edit protection, and Windows replacement-failure rollback.

Windows verification on 2026-10-03 for [the OAA 1.0 production update](https://github.com/Remgrandt/OACurator/commit/f98ca964911098cf7a26a5768f532e2b3865457d):

- `npm run check:release` passed.
- The isolated regression run passed 160 backend integration tests and 210 frontend tests. Four existing benchmark tests were ignored, and one private-maintainer documentation check was excluded from the public-source run.
- The official OAA 1.0 validator accepted all 12 generated archives, covering three independent fixture families, both import destinations, and private/default export options.

Local upgrade validation on Windows, 2026-10-04: release checks and final backend
checks passed. An external harness passed 6 upgrade tests and 12 archive tests
against the public-source library. The pinned official validator accepted both
legacy and mixed-version folders upgraded by that library with `valid: true` and
`complete: true`; original attachment hashes and backed-up manifest bytes matched.

The verification evidence is from Windows. macOS/Linux extraction and packaging
require their platform checks; archive validity alone does not establish those claims.
