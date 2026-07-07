# Archive reading core — details

Pull-tier docs for the read-only reading engine. Must-know invariants live in [CLAUDE.md](CLAUDE.md). Read this before
any non-trivial work here: editing, planning, reorganizing, or advising.

This module is decoupled from the `Volume` trait on purpose — it deals in archive-native types and the
[volume layer](../volume.rs) maps them — so it's fully unit-testable without Tauri or volume machinery. The
`ArchiveVolume` layer, routing, remote-backed byte source, and the `ArchiveError → VolumeError` mapping live in the
parent [`../DETAILS.md`](../DETAILS.md).

## What this module does

Parse an archive's directory into a synthetic directory tree, and stream-decompress individual entries. Browse +
extract-out, read-only. It does **not** touch the `Volume` trait, capability flags, `scan_for_copy`, registration, or
any write path.

## Decision: drive rc-zip's sans-IO fsm directly, not `rc-zip-tokio`

We depend on `rc-zip` (the sans-IO core) and drive its `ArchiveFsm` (central directory) and `EntryFsm` (per-entry
decompress) ourselves, over `ArchiveByteSource`. We deliberately do **not** use the `rc-zip-tokio` wrapper, for two
reasons that are both load-bearing here:

1. **Owned, cached streams.** `rc-zip-tokio`'s only public entry reader (`EntryHandle::reader()`) borrows its
   `ArchiveHandle`, which itself borrows the byte source. That can't back a reader we hand out and keep alive alongside a
   cached, owned index. Driving `EntryFsm` ourselves lets the reader own its state (a `spawn_blocking` producer) and read
   from a shared `Arc<dyn ArchiveByteSource>` with no self-referential lifetime.
2. **Off-executor decompression.** `rc-zip-tokio` decompresses inside its `AsyncRead::poll_read` — i.e. on the async
   executor. Project principle 3 (never block the runtime) requires CPU-bound decompress off it. Our reader runs the
   whole `EntryFsm` loop on a `spawn_blocking` thread.

`rc-zip` still owns everything hard: the EOCD/zip64 hunt, central-directory parsing, filename encoding detection, and
the per-method decoders (deflate/bzip2/lzma/zstd via cargo features on `rc-zip`). A future remote source implements the
same `ArchiveByteSource` trait — one uniform seam for local and remote — which is cleaner than `rc-zip-tokio`'s GAT
`HasCursor` for our caching needs.

## The byte-source seam (`ArchiveByteSource`)

A tiny, **blocking**, positioned reader: `size()` + `read_at(offset, buf)`. Blocking (not async) because both the
central-directory parse and every entry decompress already run on `spawn_blocking`, so a sync `read_at` is the natural
fit and keeps the trait trivial. `Send + Sync`, shared as `Arc` across concurrent reads — `read_at` is a `pread` with no
shared cursor, so parallel entry reads don't contend.

- `LocalFileSource`: `positioned_io::RandomAccessFile` over the real file. Backs a LOCAL archive.
- `TailCachedSource` (a decorator, `Volume`-free): caches the file's tail so the central-directory parse (rc-zip hunts
  the EOCD + directory near the end) is ONE ranged read of a slow backend, not many. Applied only to the remote source.
- `BytesSource`: in-memory (tests + small resident archives).
- The REMOTE byte source (`VolumeByteSource`) lives in the `Volume`-aware [`../volume.rs`](../volume.rs); it bridges the
  blocking `read_at` to the parent volume's async `read_range`. See [`../DETAILS.md`](../DETAILS.md) § "Remote-backed
  archives (read path)". No change to the parser or reader.

## Central-directory parse → synthetic tree

`ArchiveIndex::parse` runs in two stages, split so the tree logic is pure and I/O-free:

1. Each format's parser drives its state machine (zip: `ArchiveFsm`; tar/7z: their crates over a `SourceReader`) and
   yields a `Vec<(RawEntry, H)>`. For zip, a `read_at` returning 0 while the fsm still wants data is a truncated archive
   (`Corrupt`).
2. `build_index<H>` sanitizes each name (below), classifies it (file / explicit dir / symlink / encrypted), stashes each
   readable entry's handle for later reads, then `build_tree` synthesizes the hierarchy.

**Synthetic dirs.** Most archives carry no explicit directory entries: `a/b/c.txt` alone must produce browsable `a/` and
`a/b/`. `build_tree` walks each accepted entry's ancestors shallowest-first, creating a synthetic dir node for any that's
missing (no timestamp — it's inferred). An explicit dir entry (trailing slash or dir mode bits) that arrives later
*upgrades* the implied one in place (fills its real mtime) rather than duplicating it; order-independent. Children are
stored per-directory, pre-sorted directories-first then case-insensitive by name.

**Path collisions are first-writer-wins (order-dependent, by design).** A malformed archive can carry both a file and a
directory at the same path. Whoever claims the path first keeps it: `foo` (file) then `foo/bar` → `foo` stays a file and
`foo/bar` is **dropped** (a file can't hold children — it is not left as an unreachable orphan); `foo/bar` then `foo`
(file) → the implied directory `foo` wins and the later file is dropped. Duplicate file names (two files, same path) are
last-writer-wins instead — the later entry replaces the earlier node. A dropped file is also removed from the handle
map, so it can never be read via `open_read` even though its handle was parsed. Pinned by
`file_shadowing_a_directory_path_*` (both orders, unit and integration).

`ArchiveNode` is archive-native (name, inner path, is_dir/is_symlink, sizes, mtime, encrypted). The volume layer maps it
onto `FileEntry`. Inner paths are `/`-separated, no leading/trailing slash; the archive root is `""`. Lookups
(`get`/`list`/`is_directory`/`open_read`) trim surrounding slashes so `/dir/` and `dir` resolve the same node.

## Resource caps (memory-amplification defense)

The synthetic tree materializes one node (with a path string) per ancestor prefix of every entry, so a small central
directory can expand into a huge tree — a browse-time DoS. Two caps bound it on both axes:

- **Per-entry depth** (`name::MAX_COMPONENT_DEPTH`, 256): an entry named `a/a/…` with N components costs O(N) nodes whose
  path strings sum to O(N²) bytes; a `u16` name field allows N ≈ 32k (≈1 GB from one entry). Over-deep entries are
  **quarantined** at sanitize time (`QuarantineReason::TooDeep`), before any tree building, so the archive stays
  browsable. 256 is an order of magnitude past any real archive's nesting.
- **Total node count** (`index::MAX_TREE_NODES`, 2,000,000): the many-entries backstop. Exceeding it fails the whole
  parse with `ArchiveError::TooLarge` rather than risking an OOM. Checked once per seed in `build_tree` (a seed adds at
  most `MAX_COMPONENT_DEPTH` nodes, so the overshoot is bounded). Well beyond real archives (Linux kernel ~90k files,
  Chromium ~400k); per-node path length is separately bounded by the 64 KB name field, so worst-case memory is bounded
  too. Tested via `build_tree`'s injectable cap (`tree_building_fails_when_node_count_exceeds_the_cap`) rather than a
  multi-million-node fixture.

## Zip Slip guarantee (`sanitize_entry_name`)

Entry names are attacker-controlled. `sanitize_entry_name` is the **single choke point** every entry passes before it
enters the tree; its guarantee: *no `Accepted` path, joined under any root, escapes that root.* Enforced at the index
layer (not only at extraction) so an escaping path never even becomes a browsable node — defense in depth.

Rules: normalize `\`→`/`; drop empty and `.` components (collapses leading/trailing/doubled slashes); **clamp** absolute
paths to the root (strip leading slashes — the entry stays visible, can't escape, matches `unzip`); **quarantine**
(reject) any entry with a `..` component (it can't be safely clamped to one in-root location), that normalizes to
nothing, or that nests past the depth cap (see Resource caps above). Quarantined raw names are recorded on the index
(`quarantined()`) with their reason for diagnostics; they're absent from the tree.

Note `..` matches only a whole component — `..foo` and `foo..bar` are legitimate filenames. (rc-zip's own
`Entry::sanitized_name` uses a coarser `contains("..")` substring test and doesn't normalize `\`; we don't use it.)

## Streaming reads, off the executor (`ArchiveEntryReader`)

Mirrors the SMB backend's channel-backed read (see [`../../DETAILS.md`](../../DETAILS.md) § Pattern B):

- A `spawn_blocking` producer owns the `Arc<dyn ArchiveByteSource>` and the entry handle, seeks to the entry's local
  header offset, and drives `EntryFsm` (read compressed bytes → decompress) entirely off the executor.
- It sends decompressed chunks (≤ 128 KiB each) through a bounded channel (capacity 4). Peak in-flight memory is
  `capacity × chunk` (~512 KiB) regardless of the entry's uncompressed size — never the whole entry (principle 5, the
  trait's "must stream" rule). The `streams_large_entry_in_bounded_chunks` test pins the per-chunk bound.
- `next_chunk().await` pulls from the channel. Dropping the reader drops the receiver; the producer's next `send` fails
  and it stops — that's the cancel path, no extra signalling.

`total_size()` reports the entry's full uncompressed size (from the central directory) up front; `bytes_read()` tracks
delivery. Concurrency: each reader has its own `EntryFsm` and read offset, so N concurrent reads over one source are
independent (`concurrent_reads_are_independent`).

The fsm reads ahead (its buffer always has spare room), so it asks to read past the entry's own bytes and reaches the
real file end even for a complete entry — EOF alone is not truncation. Truncation is EOF *plus* a `process` that then
makes no progress; that yields a typed `Corrupt` error rather than a spin on repeated empty reads
(`truncated_entry_data_errors_instead_of_hanging`).

## Typed errors (`ArchiveError`)

Every failure is a distinct variant so callers classify by `matches!`, never by message substring (the
`no-string-matching` rule); `String` payloads are display-only. `From<rc_zip::Error>` maps:
`DirectoryEndSignatureNotFound → NotAnArchive` (a non-zip / RAR / 7z / plain file — magic-byte format detection is the
routing layer's job, not ours), other `Format → Corrupt`, `Unsupported → Unsupported` (a method this build can't decode,
or an LZMA variant), `Decompression → Corrupt`, `IO(UnexpectedEof) → Corrupt` (truncated) else `Io`.

**Encryption** isn't in `rc_zip::Error` — we detect it ourselves from general-purpose flag bit 0 or the AE-x marker
method. Browsing an encrypted archive works (names live in the central directory); `open_read` on an encrypted entry
returns `Encrypted`, and `has_encrypted_entries()` lets the volume layer gate extraction up front. The
`ArchiveError → VolumeError` mapping (a mid-browse backstop) lives in [`../DETAILS.md`](../DETAILS.md).

## Filename encoding

rc-zip owns this: its central-directory parser detects UTF-8 (flag bit 11) vs the legacy OEM/CP437 code page (chardetng
heuristic + a CP437 suspicious-byte check) and decodes names into UTF-8 `String`s. We consume the decoded `entry.name`
directly. `non_utf8_name_is_decoded_best_effort` pins that a high-byte, non-UTF-8-flagged name decodes without erroring
and preserves its ASCII parts.

## Index cache (`ArchiveIndexCache`)

A plain content cache keyed by `(path, size, mtime)`; hits are a cheap `Arc` clone. Any external edit changes size or
mtime, so it's a natural miss and re-parse — no explicit invalidation. `index_for_local` is **blocking** (stats the
file, parses on a miss); call it from `spawn_blocking`. No eviction policy here — the volume layer owns archive lifetime
(refcount + LRU) and can `clear()` on teardown.

## tar and 7z (read-only, multi-format core)

The read core serves three formats behind ONE tree + query surface. Only parsing and the per-entry read handle differ;
the Zip Slip sanitizer (`name.rs`), the synthetic-tree builder and DoS caps (`index.rs`), the byte-source seam
(`source.rs`), the cache, and `ArchiveEntryReader` are all format-agnostic.

**The generic seam.** Each format's `parse` produces a `Vec<(RawEntry, H)>` — a format-neutral `RawEntry`
(name/is_dir/is_symlink/size/mtime/encrypted) plus a per-format read handle `H`. `index::build_index<H>` runs the SAME
sanitize + `build_tree` + prune over any `H`, returning a `BuiltIndex<H>` the format wraps into an `EntryStore` variant.
`ArchiveIndex::open_read` looks the node up (dir → `IsADirectory`), then dispatches to the store. So a new format is a
new `parse` + producer + `EntryStore` arm; the tree, safety, and caps come for free. `ArchiveVolume` holds the
`ArchiveFormat` (decided from the path at resolve time) and threads it into every parse.

**Format detection (`format.rs`).** `format_for_name` is the single source of truth (both `has_supported_archive_extension`
and the FE mirror defer to it). It matches by NAME SUFFIX, longest-first, so `.tar.gz` is a gzip tar (not the `.tar` its
substring suggests, nor a bare `.gz` — a single compressed file with nothing to browse is deliberately NOT an archive).
The boundary detector (`../boundary.rs`) then confirms with per-format magic (`bytes_match_archive_magic`): zip `PK`,
gzip `1f 8b`, bzip2 `BZh`, xz `fd 37 7a 58 5a 00`, zstd `28 b5 2f fd`, 7z `37 7a bc af 27 1c`, and — the one that isn't
at offset 0 — a plain tar's `ustar` at offset 257, so the confirm reads a 512-byte prefix (`ARCHIVE_MAGIC_PREFIX_LEN`).

**Codecs — all pure-Rust, all pull-model `Read`, bounded memory (`format.rs::open_tar_decoder`).** gzip → `flate2`
(`miniz_oxide`), bzip2 → `bzip2` (pure `libbz2-rs-sys` default, no C libbz2), xz → `lzma-rust2`'s `XzReader` (the pure
streaming `.xz` reader `lzma-rs` lacks), zstd → `ruzstd`'s `StreamingDecoder`. Each handles concatenated members
(`gzip -c a b`). tar parsing/streaming rides the `tar` crate over a `SourceReader` (a `Read`+`Seek` cursor on the byte
source); 7z uses `sevenz-rust2`'s `ArchiveReader` (needs `Read`+`Seek`, hence `SourceReader`). LZMA/LZMA2 (the common 7z
codec) is built into `sevenz-rust2`; `bzip2`/`deflate`/`ppmd` are feature-enabled. `aes256` is OFF, so an encrypted 7z
surfaces a typed error rather than being decrypted (matches zip's "encrypted → reject").

**Access class — the sequential trap (`ArchiveFormat::is_sequential`, `Volume::extraction_is_sequential`).** A plain
`.tar` is RANDOM-access: `TarStore` records each member's data offset, so `open_read` seeks and streams the member's
exact bytes (no decompression). A COMPRESSED tar and 7z are SEQUENTIAL: there's no random access into the decoded stream.

- **tar (compressed):** `open_read` prefix-decodes the whole stream from the start, matching entries by their sanitized
  name, and streams the target — O(prefix) per entry, bounded memory (data before the target is decoded and discarded).
- **7z:** the header (metadata, at the file end like zip's CD) builds the index with NO decompression. `open_read`
  drives `for_each_entries`; inside a SOLID block the entries share one decode stream, so an earlier entry must be fully
  CONSUMED (read to a sink) to advance the decoder to the target — skipping without reading desyncs it into a checksum
  failure. Reaching the target streams it, then stops.

The O(n²) bulk-extract caveat for compressed tar / solid 7z (the copy engine re-decodes the prefix per entry) is a
documented performance fast-follow, tracked with the volume layer in [`../DETAILS.md`](../DETAILS.md).

**Remote (SMB/MTP).** tar/7z fall out of the remote seams naturally: `SourceReader` reads over any `ArchiveByteSource`,
including the parent-backed `VolumeByteSource`, so a remote tar/7z browses and extracts through the parent's ranged
reads with no new code. A compressed-tar index parse streams the whole file once over `read_range`; a 7z header parse
benefits from `TailCachedSource`. The plain-tar random-access path issues one `read_range` per member read.

## Testing

`test_fixtures.rs` (lives at the archive root, shared with the volume/mutation/watch tests) builds clean zips with the
`zip` crate (no checked-in blobs; `large_file(true)` for a real zip64 fixture) and byte-patches hostile variants
(traversal name via equal-length patch, encrypted GP flag, overstated EOCD record count).

`archive_test.rs` covers parse + listing, synthetic dirs, Zip Slip quarantine, the depth-bomb quarantine,
first-writer-wins file/dir collisions (both orders), the empty archive, a zip64 round-trip, encrypted/corrupt/not-an-
archive typed errors, streaming (bounded chunks + decompression correctness + truncated-entry error), stored reads,
concurrent reads, best-effort encoding, and cache hit/invalidation. `name.rs` and `index.rs` carry pure unit tests for
the sanitizer (incl. the depth cap) and the tree builder (incl. the node-count cap and the collision orders).

`multiformat_test.rs` builds tar and 7z fixtures in memory (dev-only encoders: `tar`/`flate2`/`bzip2` writers,
`lzma-rust2`'s `XzWriter`, `zstd`, and `sevenz-rust2`'s `compress` feature — the shipped path stays decode-only) and
covers the tar synthetic tree, a per-codec round-trip (plain/gzip/bzip2/xz/zstd), bounded-chunk streaming, the tar Zip
Slip quarantine (a hostile `../evil.txt` injected as a raw ustar header), the symlink-extracts-to-nothing safety, and 7z
browse + solid-block extract of a later member. `format.rs` unit-tests suffix detection and the sequential class.
