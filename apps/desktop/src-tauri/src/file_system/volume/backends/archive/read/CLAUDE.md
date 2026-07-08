# Archive reading core

The read-only engine under the `ArchiveVolume` backend: parse an archive's directory into a synthetic tree and
stream-decompress entries. Serves zip, tar (+ gzip/xz/bzip2/zstd), and 7z behind ONE tree + query surface; only parsing
and the per-entry read handle differ per format.

**Volume-free by design.** Everything here deals in archive-native types (`ArchiveIndex` / `ArchiveNode` /
`ArchiveError`); the [volume layer](../volume.rs) maps them onto `FileEntry` / `VolumeError`. Keep this module free of
the `Volume` trait, capability flags, and any write path.

## Module map

- `index.rs`: `ArchiveIndex` (parsed tree + query surface), `ArchiveNode`, the generic `build_index<H>` seam, `EntryStore`
  dispatch, and the DoS caps.
- `format.rs`: `ArchiveFormat`, `format_for_name` (detection SoT), `is_sequential`, `open_tar_decoder` (the codecs).
- `zip.rs` / `tar.rs` / `sevenz.rs`: per-format parse + producer + `EntryStore` arm.
- `source.rs`: the `ArchiveByteSource` seam + `LocalFileSource` / `BytesSource` / `TailCachedSource`.
- `reader.rs`: `ArchiveEntryReader` — chunked, off-executor decompression. `extract.rs`: `SubtreeExtractReader` — the
  one-pass subtree extractor for sequential formats (compressed tar, 7z), decode-once bulk extract.
- `name.rs`: `sanitize_entry_name` — the Zip Slip defense. `cache.rs`: `ArchiveIndexCache`. `error.rs`: `ArchiveError`.

Depth, rationale, and the full test list: [DETAILS.md](DETAILS.md). Read it before any non-trivial work here: editing,
planning, reorganizing, or advising.

## Must-knows

- **Zip Slip: `sanitize_entry_name` is the single choke point every entry passes before entering the tree; don't
  bypass it.** No `Accepted` inner path escapes its root. Don't swap in rc-zip's coarser `Entry::sanitized_name`.
- **Format is decided by NAME SUFFIX (`format_for_name`, the single source of truth), then confirmed by per-format
  magic (in [`../boundary.rs`](../boundary.rs)).** Longest-suffix wins: `.tar.gz` is a gzip tar, a bare `.gz` is not an
  archive.
- **We drive rc-zip's sans-IO fsm directly, NOT `rc-zip-tokio`** (which borrows its handle and decompresses on the
  executor).
- **Decompression runs on `spawn_blocking`; reads are chunked, never whole-entry buffered** — don't add a whole-entry
  `Vec` to the read path.
- **Compressed tar and 7z are SEQUENTIAL-access**; plain `.tar` and zip are random. `ArchiveFormat::is_sequential`
  declares the class (the volume layer surfaces it via `Volume::extraction_is_sequential`).
- **Encryption: browsing always works; extraction decrypts a legacy ZipCrypto entry with a per-archive password**,
  routed through the `zip` crate's `by_index_decrypt` by CENTRAL-DIRECTORY ORDINAL (rc-zip parses but can't decrypt; the
  ordinals align — pinned). No password ⇒ `Encrypted`; wrong ⇒ `WrongPassword` (ZipCrypto's wrong-password may surface
  late as an end-of-stream CRC, mapped by io-kind not message). WinZip AES and 7z AES are deferred (`aes` crate conflicts
  with `smb2`) and refuse honestly as `Unsupported` (→ `NotSupported`; never a "damaged archive", never a password
  prompt that can't succeed) — for 7z via `sevenz.rs::map_sevenz_err`. Filename encoding is rc-zip's job — consume the
  decoded `entry.name`.
- **The index cache key is `(path, size, mtime)`** (external edits auto-invalidate); `index_for_local` is blocking, call
  it from `spawn_blocking`.
- **Two DoS caps bound the synthetic tree**: per-entry depth (`name::MAX_COMPONENT_DEPTH`, over-deep entries quarantine)
  and total node count (`index::MAX_TREE_NODES`, over-cap fails the parse `TooLarge`). Don't remove either.
