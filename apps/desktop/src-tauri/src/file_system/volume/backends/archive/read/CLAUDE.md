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

Depth, rationale, and the full test list: `DETAILS.md`. Read it before any non-trivial work here: editing,
planning, reorganizing, or advising.

## Must-knows

- **Zip Slip: `sanitize_entry_name` is the single choke point every entry passes before entering the tree; don't
  bypass it.** No `Accepted` inner path escapes its root. Don't swap in rc-zip's coarser `Entry::sanitized_name`.
- **Format is decided by NAME SUFFIX (`format_for_name`, the single source of truth), then confirmed by per-format
  magic (in `../boundary.rs`).** Longest-suffix wins: `.tar.gz` is a gzip tar, a bare `.gz` is not an
  archive.
- **We drive rc-zip's sans-IO fsm directly, NOT `rc-zip-tokio`** (which borrows its handle and decompresses on the
  executor).
- **Decompression runs on `spawn_blocking`; reads are chunked, never whole-entry buffered** — don't add a whole-entry
  `Vec` to the read path.
- **Compressed tar and 7z are SEQUENTIAL-access**; plain `.tar` and zip are random. `ArchiveFormat::is_sequential`
  declares the class (the volume layer surfaces it via `Volume::extraction_is_sequential`).
- **Encryption: browsing usually works; extraction decrypts with a per-archive password.** Zip (ZipCrypto AND WinZip
  AES) routes through the `zip` crate's `by_index_decrypt` by CENTRAL-DIRECTORY ORDINAL (rc-zip parses but can't decrypt;
  the ordinals align — pinned). 7z threads the password through `sevenz.rs`'s `parse` + every re-open. No password ⇒
  `Encrypted`; wrong ⇒ `WrongPassword`. Wrong-password detection differs: AES zip has a 2-byte verifier (caught at open);
  ZipCrypto may surface late as an end-of-stream CRC (io-kind, not message); 7z AES has NO verifier, so a wrong password
  first fails an integrity check mid-decode — `sevenz.rs` recovers the wrapped typed error and, since a password WAS
  supplied, types it `WrongPassword` (never string-matched). **A HEADER-encrypted 7z (`-mhe=on`) needs the password to
  even BROWSE** (encrypted metadata), so `parse` — not just extraction — returns `Encrypted`/`WrongPassword`; the volume
  layer surfaces it as `NeedsPassword` on the LISTING path (browse-time prompt). Filename encoding is rc-zip's job for
  zip — consume the decoded `entry.name`.
- **The index cache key is `(path, size, mtime)`** (external edits auto-invalidate); `index_for_local` is blocking, call
  it from `spawn_blocking`.
- **Two DoS caps bound the synthetic tree**: per-entry depth (`name::MAX_COMPONENT_DEPTH`, over-deep entries quarantine)
  and total node count (`index::MAX_TREE_NODES`, over-cap fails the parse `TooLarge`). Don't remove either.
