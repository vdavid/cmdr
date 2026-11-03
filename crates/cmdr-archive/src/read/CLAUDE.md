# Archive reading core

The read-only engine under the `ArchiveVolume` backend: parse an archive's directory into a synthetic tree and
stream-decompress entries. Serves zip, tar (+ gzip/xz/bzip2/zstd), and 7z behind ONE tree + query surface; only parsing
and the per-entry read handle differ per format.

**Volume-free by design.** Everything here deals in archive-native types (`ArchiveIndex` / `ArchiveNode` /
`ArchiveError`); the [volume layer](../volume.rs) maps them onto `FileEntry` / `VolumeError`. Keep this module free of
the `Volume` trait, capability flags, and any write path.

## Module map

- `index.rs`: `ArchiveIndex` (parsed tree + query surface), `ArchiveNode`, the generic `build_index<H>` seam, and
  `EntryStore`, the one per-format dispatcher (entry reads and the one-pass producer). `raw.rs`: the leaf parser
  contract, `RawEntry` + `MAX_TREE_NODES`. Nothing imports back from `index` (DETAILS § Layering).
- `format.rs`: `open_tar_decoder` (the codecs), plus a re-export of the naming vocabulary — `ArchiveFormat`,
  `format_for_name` (detection SoT), `is_sequential` — which lives in `crates/cmdr-fs/src/archive_format.rs` because
  `FileEntry.is_archive` reads it.
- `zip.rs` / `tar.rs` / `sevenz.rs`: per-format parse + producer + `EntryStore` arm. `zip_times.rs`: DOS-only entry
  times (DETAILS § Entry times).
- `source.rs`: the `ArchiveByteSource` seam + `LocalFileSource` / `BytesSource` / `TailCachedSource`.
- `reader.rs`: `ArchiveEntryReader` — chunked, off-executor decompression. `extract.rs`: `SubtreeExtractReader` — the
  one-pass subtree extractor for sequential formats (compressed tar, 7z), decode-once bulk extract.
- `sidecars.rs`: which entries are AppleDouble sidecars per format, and the xattrs a parse holds or a copy reads.
- `name.rs`: `sanitize_entry_name` — the Zip Slip defense, its decision Kani-proven (`name/proofs.rs`). `cache.rs`:
  `ArchiveIndexCache`. `error.rs`: `ArchiveError`.

## Must-knows

- **Zip Slip: `sanitize_entry_name` is the single choke point every entry passes before entering the tree; don't bypass
  it.** No `Accepted` inner path escapes its root. Don't swap in rc-zip's coarser `Entry::sanitized_name`.
- **Format is decided by NAME SUFFIX (`format_for_name`, the single source of truth), then confirmed by per-format magic
  (in `../boundary.rs`).** Longest-suffix wins: `.tar.gz` is a gzip tar, a bare `.gz` is not an archive.
- **We drive rc-zip's sans-IO fsm directly, NOT `rc-zip-tokio`** (which borrows its handle and decompresses on the
  executor).
- **Decompression runs on `spawn_blocking`; reads are chunked, never whole-entry buffered** — don't add a whole-entry
  `Vec` to the read path.
- **Compressed tar and 7z are SEQUENTIAL-access**; plain `.tar` and zip are random. `ArchiveFormat::is_sequential`
  declares the class (the volume layer surfaces it via `Volume::extraction_is_sequential`).
- **Encryption: browsing usually works; extraction decrypts with a per-archive password.** Zip (ZipCrypto and WinZip
  AES) decrypts through the `zip` crate's `by_index_decrypt`, paired by LOCAL HEADER OFFSET (rc-zip can't decrypt; ❌
  never by ordinal, which a duplicate name shifts). No password ⇒ `Encrypted`; wrong ⇒ `WrongPassword`, typed, never
  string-matched. A HEADER-encrypted 7z needs the password to even BROWSE. Detection per cipher: DETAILS § "Decryption
  (zip)", and `../../DETAILS.md` § "Password-protected archives".
- **`ArchiveNode::mode` is what the archive RECORDED, `None` when it recorded nothing** (zip external attributes, the
  tar header, 7z's `0x8000` unix extension). ❌ Never a plausible `0o644`: the copy engine puts it on what an extract
  writes. Low nine bits only — setuid/setgid/sticky are dropped at the parser.
- **The index cache key is `(path, size, mtime)`** (external edits auto-invalidate); `index_for_local` is blocking, call
  it from `spawn_blocking`.
- **DoS caps**: per-entry depth (`name::MAX_COMPONENT_DEPTH`, over-deep entries quarantine), total node count
  (`raw::MAX_TREE_NODES`), and an xz block's dictionary (`format::XZ_MEMORY_LIMIT_KIB`) fail as `TooLarge`; one
  sidecar's and one index's held xattr bytes drop metadata. ❌ Don't remove any. DETAILS § "Resource caps".
- **AppleDouble sidecars (`__MACOSX/._x`, a tar's `._x`) pair AFTER the sanitizer and never become nodes**: their xattrs
  ride the item (`item_xattrs`). DETAILS § "AppleDouble sidecars".

Depth, rationale, and the full test list: `DETAILS.md`. Read it before any non-trivial work here: editing, planning,
reorganizing, or advising.
