# Archive reading core — details

Pull-tier docs for `backends/archive/`: the read-only zip core the `ArchiveVolume` backend is built on.
Must-know invariants live in [CLAUDE.md](CLAUDE.md). This module is decoupled from the `Volume` trait on purpose — it
deals in archive-native types and the volume layer maps them — so it's fully unit-testable without Tauri or volume
machinery.

## What this module does

Parse a zip's central directory into a synthetic directory tree, and stream-decompress individual entries. Browse +
extract-out, read-only. It does **not** touch the `Volume` trait, capability flags, `scan_for_copy`, registration, or
any write path — those live in the `ArchiveVolume` impl built on top of this.

## Module map

- `source.rs`: `ArchiveByteSource` (the byte-supply seam) + `LocalFileSource` (a `pread` over a local file) and
  `BytesSource` (in-memory, tests + small resident archives).
- `index.rs`: `ArchiveIndex` (the parsed tree + query surface), `ArchiveNode`, the central-directory parse driver, and
  the pure synthetic-tree builder.
- `name.rs`: `sanitize_entry_name` — the Zip Slip defense (pure).
- `read.rs`: `ArchiveEntryReader` — chunked, off-executor decompression.
- `cache.rs`: `ArchiveIndexCache` — parsed indexes keyed by `(path, size, mtime)`.
- `test_fixtures.rs` / `archive_test.rs`: fixture builders and behaviour tests.

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
the per-method decoders (deflate/bzip2/lzma/zstd via cargo features on `rc-zip`). The remote M5 source implements the
same `ArchiveByteSource` trait — one uniform seam for local and remote — which is cleaner than `rc-zip-tokio`'s GAT
`HasCursor` for our caching needs.

## The byte-source seam (`ArchiveByteSource`)

A tiny, **blocking**, positioned reader: `size()` + `read_at(offset, buf)`. Blocking (not async) because both the
central-directory parse and every entry decompress already run on `spawn_blocking`, so a sync `read_at` is the natural
fit and keeps the trait trivial. `Send + Sync`, shared as `Arc` across concurrent reads — `read_at` is a `pread` with no
shared cursor, so parallel entry reads don't contend.

- `LocalFileSource`: `positioned_io::RandomAccessFile` over the real file. This is the only backing implemented now.
- Remote (M5): a parent volume's ranged read implements the same trait, bridging its async read to the blocking call
  from inside the `spawn_blocking` context. No change to the parser or reader.

## Central-directory parse → synthetic tree

`ArchiveIndex::parse` runs in two stages, split so the tree logic is pure and I/O-free:

1. `parse_central_directory` drives `ArchiveFsm`: loop `wants_read()` → `read_at` into `space()` → `fill(n)` →
   `process()` until `Done(archive)`, then clone out the flat `Vec<Entry>`. A `read_at` returning 0 while the fsm still
   wants data is a truncated archive (`Corrupt`).
2. `build_index` sanitizes each name (below), classifies it (file / explicit dir / symlink / encrypted), stashes each
   readable file's `rc_zip::Entry` for later reads, then `build_tree` synthesizes the hierarchy.

**Synthetic dirs.** Most zips carry no explicit directory entries: `a/b/c.txt` alone must produce browsable `a/` and
`a/b/`. `build_tree` walks each accepted entry's ancestors shallowest-first, creating a synthetic dir node for any that's
missing (no timestamp — it's inferred). An explicit dir entry (trailing slash or dir mode bits) that arrives later
*upgrades* the implied one in place (fills its real mtime) rather than duplicating it; order-independent. A file whose
path collides with a directory (malformed zip) loses — the directory, with children, wins. Duplicate file names: last
entry wins. Children are stored per-directory, pre-sorted directories-first then case-insensitive by name.

`ArchiveNode` is archive-native (name, inner path, is_dir/is_symlink, sizes, mtime, encrypted). The volume layer maps it
onto `FileEntry`. Inner paths are `/`-separated, no leading/trailing slash; the archive root is `""`. Lookups
(`get`/`list`/`is_directory`/`open_read`) trim surrounding slashes so `/dir/` and `dir` resolve the same node.

## Zip Slip guarantee (`sanitize_entry_name`)

Entry names are attacker-controlled. `sanitize_entry_name` is the **single choke point** every entry passes before it
enters the tree; its guarantee: *no `Accepted` path, joined under any root, escapes that root.* Enforced at the index
layer (not only at extraction) so an escaping path never even becomes a browsable node — defense in depth.

Rules: normalize `\`→`/`; drop empty and `.` components (collapses leading/trailing/doubled slashes); **clamp** absolute
paths to the root (strip leading slashes — the entry stays visible, can't escape, matches `unzip`); **quarantine**
(reject) any entry with a `..` component (it can't be safely clamped to one in-root location) or that normalizes to
nothing. Quarantined raw names are recorded on the index (`quarantined()`) for diagnostics; they're absent from the tree.

Note `..` matches only a whole component — `..foo` and `foo..bar` are legitimate filenames. (rc-zip's own
`Entry::sanitized_name` uses a coarser `contains("..")` substring test and doesn't normalize `\`; we don't use it.)

## Streaming reads, off the executor (`ArchiveEntryReader`)

Mirrors the SMB backend's channel-backed read (see `../DETAILS.md` § Pattern B):

- A `spawn_blocking` producer owns the `Arc<dyn ArchiveByteSource>` and the `rc_zip::Entry`, seeks to the entry's local
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
returns `Encrypted`, and `has_encrypted_entries()` lets the volume layer gate extraction up front.

## Filename encoding

rc-zip owns this: its central-directory parser detects UTF-8 (flag bit 11) vs the legacy OEM/CP437 code page (chardetng
heuristic + a CP437 suspicious-byte check) and decodes names into UTF-8 `String`s. We consume the decoded `entry.name`
directly. `non_utf8_name_is_decoded_best_effort` pins that a high-byte, non-UTF-8-flagged name decodes without erroring
and preserves its ASCII parts.

## Index cache (`ArchiveIndexCache`)

A plain content cache keyed by `(path, size, mtime)`; hits are a cheap `Arc` clone. Any external edit changes size or
mtime, so it's a natural miss and re-parse — no explicit invalidation. `index_for_local` is **blocking** (stats the
file, parses on a miss); call it from `spawn_blocking`. No eviction policy here — the volume layer owns archive lifetime
(refcount + LRU per the plan) and can `clear()` on teardown.

## Testing

`test_fixtures.rs` builds clean zips with the `zip` crate (no checked-in blobs) and byte-patches hostile variants
(traversal name via equal-length patch, encrypted GP flag, overstated EOCD record count). `archive_test.rs` covers parse
+ listing, synthetic dirs, Zip Slip quarantine, encrypted/corrupt/not-an-archive typed errors, streaming (bounded chunks
+ decompression correctness), stored reads, concurrent reads, best-effort encoding, and cache hit/invalidation.
`name.rs` and `index.rs` carry pure unit tests for the sanitizer and the tree builder.

## Left for the `ArchiveVolume` (Volume-impl) layer

The `Volume` trait impl, capability flags, `scan_for_copy`, registration/refcount/LRU, mapping `ArchiveNode → FileEntry`
and `ArchiveError → VolumeError`, wrapping `ArchiveEntryReader` as a `VolumeReadStream`, and the capability-matrix column
+ `docs/architecture.md` line (they describe the volume's capabilities, which don't exist until that layer). All of the
map-from surface it needs is public on `ArchiveIndex` / `ArchiveNode` / `ArchiveEntryReader`.
