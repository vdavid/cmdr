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
the per-method decoders (deflate/bzip2/lzma/zstd via cargo features on `rc-zip`). A future remote source (remote-backed
archives) implements the same `ArchiveByteSource` trait — one uniform seam for local and remote — which is cleaner than
`rc-zip-tokio`'s GAT `HasCursor` for our caching needs.

## The byte-source seam (`ArchiveByteSource`)

A tiny, **blocking**, positioned reader: `size()` + `read_at(offset, buf)`. Blocking (not async) because both the
central-directory parse and every entry decompress already run on `spawn_blocking`, so a sync `read_at` is the natural
fit and keeps the trait trivial. `Send + Sync`, shared as `Arc` across concurrent reads — `read_at` is a `pread` with no
shared cursor, so parallel entry reads don't contend.

- `LocalFileSource`: `positioned_io::RandomAccessFile` over the real file. This is the only backing implemented now.
- Remote (future, with remote-backed archives): a parent volume's ranged read implements the same trait, bridging its
  async read to the blocking call from inside the `spawn_blocking` context. No change to the parser or reader.

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
*upgrades* the implied one in place (fills its real mtime) rather than duplicating it; order-independent. Children are
stored per-directory, pre-sorted directories-first then case-insensitive by name.

**Path collisions are first-writer-wins (order-dependent, by design).** A malformed zip can carry both a file and a
directory at the same path. Whoever claims the path first keeps it: `foo` (file) then `foo/bar` → `foo` stays a file and
`foo/bar` is **dropped** (a file can't hold children — it is not left as an unreachable orphan); `foo/bar` then `foo`
(file) → the implied directory `foo` wins and the later file is dropped. Duplicate file names (two files, same path) are
last-writer-wins instead — the later entry replaces the earlier node. A dropped file is also removed from the `files`
map, so it can never be read via `open_read` even though its `Entry` was parsed. Pinned by
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

## The `ArchiveVolume` layer

`volume.rs` is the one file in this module that touches the `Volume` trait. It maps the archive-native core
(`ArchiveIndex` / `ArchiveNode` / `ArchiveEntryReader` / `ArchiveError`) onto `FileEntry` / `VolumeReadStream` /
`VolumeError`, and holds an `Arc<dyn Volume>` **parent** (the volume physically storing the `.zip`), the archive path,
the display name, and an `Arc<ArchiveIndexCache>`.

**The parent seam.** Two answers a read-only archive can't give itself come from the parent:

- `lane_key()` returns `parent.lane_key()` — archive work must share the physical device's serialization lane (a zip on
  an SMB share shares that share's lane), never key on the archive path. Consequence: two zips on the same mount
  serialize; only zips on different mounts parallelize (the existing per-device write-serialization).
- `get_space_info()` delegates to the parent (see the decision below).

Only a **local** parent is exercised now: the backing bytes come from a `LocalFileSource` opened over the archive path,
and the cache's `index_for_local` does the local stat+parse. A remote parent (when remote-backed archives land) supplies
bytes by implementing `ArchiveByteSource` over its ranged reads — no change to the index or reader.

**Path namespace.** `root()` is the real `.zip` path and inner entries join under it, so `/path/to/foo.zip/inner`
renders transparently (the FE splits on `/`). `inner_path()` maps a volume-namespace path back to the index key: it
strips the archive-path prefix (the FE sends full absolute paths), accepts an already-inner relative path, and treats an
empty path / `.` as the root `""`. `node_to_entry` builds each `FileEntry`'s full path as `archive_path/inner`; the root
node (`""`) carries the archive's own file name and path. `ArchiveNode::modified` is Unix seconds, matching `FileEntry`
(a negative timestamp is dropped); `extended_metadata_loaded` is `true` — the archive listing is complete in one pass,
no deferred enrichment.

**Streaming reads.** `open_read_stream` / `open_read_stream_at_offset` parse the index (cached) and open the byte source
on `spawn_blocking`, then wrap `ArchiveEntryReader` as an `ArchiveVolumeReadStream`. A compressed entry has no random
access, so a non-zero offset means "decompress from the start and discard the leading `offset` bytes" — correct, not
cheap. `total_size()` reports the FULL uncompressed size (per the trait); `bytes_read()` counts only the delivered
segment. Nothing calls the at-offset path with a non-zero offset today, so the common path discards nothing.

**`scan_for_copy`.** Counts and byte totals come straight from the central directory — no decompression during the scan.
A single file is one entry at its uncompressed size; a directory walks the subtree via the index's per-dir child lists
(the top-level dir isn't counted, matching `LocalPosixVolume`). `dedup_bytes == total_bytes`: a zip has no hardlinks.

**Capability flags (set explicitly, not inherited).** `local_path = None` and `supports_local_fs_access = false` (inner
paths aren't reachable via `std::fs`, so no `copyfile` fast path and the legacy synthetic-diff path is skipped);
`space_poll_interval = None` (a read-only archive's space never changes — the default `Some(2s)` would poll pointlessly);
`max_concurrent_ops = 1`; `supports_export`/`supports_streaming = true`; `listing_is_watched = false` (no live watcher
yet, so it must not claim listing freshness).

### Decision: `get_space_info` delegates to the parent volume

**Why**: An archive isn't a disk with its own free space. The pre-copy space check (`volume_copy.rs`) blocks a copy when
`dest.available_bytes < total_bytes`, so reporting zeros (or `available = 0`) would read as "disk full" and block a
paste with a spurious message instead of the correct read-only / `NotSupported` outcome. Any archive edit (temp+rename)
is built on the parent drive, so the parent's free space is the honest constraint AND a non-blocking answer. Delegating
is one line and stays correct when mutation turns on. Pinned by `get_space_info_delegates_to_the_parent`.

### Decision: `max_concurrent_ops = 1` for the read-only phase

**Why**: The core supports concurrent independent reads (each `ArchiveEntryReader` owns its `EntryFsm` and read offset
over a shared `pread` source, no shared cursor — `concurrent_reads_on_two_entries_are_independent` proves it through the
`Volume` API). But the copy engine's parallelism is a separate hint; the plan pins a single stream in flight against an
archive for this phase. Raise it later if a real workload wants parallel extract.

### Decision: typed `ArchiveError → VolumeError` mapping, no message strings

**Why** (`no-string-matching`): `to_volume_error` maps the path-shaped errors to their `VolumeError` twins
(`NotFound → NotFound`, `IsADirectory → IsADirectory`) so path-aware callers keep working, the I/O family
(`Corrupt` / `Io → IoError`), and the rejection family (`NotAnArchive` / `Encrypted` / `Unsupported` / the `TooLarge`
DoS cap `→ NotSupported`). This is a **mid-browse backstop** (the archive was swapped or corrupted after navigation).
The user-facing "not a real archive" / "encrypted" friendly copy is produced at the routing boundary straight from
the raw `ArchiveError` at navigation time — not recovered from a `VolumeError` here — so this mapping deliberately
doesn't need a new `VolumeError` variant or a dedicated friendly-error reason yet.

The match is **exhaustive on purpose — no wildcard**. It's a compile-time tripwire (the repo convention, per
`analytics.rs`): a new `ArchiveError` variant must fail to compile here and force a conscious mapping. A catch-all
`_ => NotSupported` would silently mis-serve a future *non-rejection* variant — say a transient remote-source error
once remote-backed archives land, which wants a retryable classification, not "not supported". The one-time cost is
naming each new variant; the payoff is that no failure mode is ever classified by omission.

## Testing

`volume_test.rs` drives `ArchiveVolume` against real zips written to a temp file (the local source needs a real path):
list/metadata/exists round-trips incl. synthetic dirs and the transparent path, absolute-path prefix stripping, the
progress tick, the cancelable-listing default, streaming extract + the at-offset tail, concurrent two-entry reads,
`scan_for_copy` (subtree / single file / missing), every-mutation-unsupported (incl. the `create_directory_all` guard),
encrypted/corrupt/non-zip typed errors through the `Volume` API, the parent `lane_key` / `get_space_info` delegation,
and the capability flags.

`test_fixtures.rs` builds clean zips with the `zip` crate (no checked-in blobs; `large_file(true)` for a real zip64
fixture) and byte-patches hostile variants (traversal name via equal-length patch, encrypted GP flag, overstated EOCD
record count). `archive_test.rs` covers parse + listing, synthetic dirs, Zip Slip quarantine, the depth-bomb quarantine,
first-writer-wins file/dir collisions (both orders), the empty archive, a zip64 round-trip, encrypted/corrupt/not-an-
archive typed errors, streaming (bounded chunks + decompression correctness + truncated-entry error), stored reads,
concurrent reads, best-effort encoding, and cache hit/invalidation. `name.rs` and `index.rs` carry pure unit tests for
the sanitizer (incl. the depth cap) and the tree builder (incl. the node-count cap and the collision orders).

## Left for the follow-up milestones

`ArchiveVolume` (browse + extract + `scan_for_copy`, read-only) now exists in `volume.rs`, with the capability-matrix
column filled in `../../DETAILS.md` and the map line in `docs/architecture.md`. What's still ahead (sequencing lives in
`/docs/specs/archive-browsing-plan.md`):

- **Routing**: path-aware `VolumeManager::resolve(volume_id, path)` that detects an archive boundary and
  registers/looks up an `ArchiveVolume`; `register_if_absent` + **refcount + LRU eviction** (browsing many zips must not
  leak volumes + parents + index caches); the magic-byte sniff at navigation time (listing uses extension only); the
  `'archive'` FE `VolumeKind` so capabilities read read-only-correct; persistence of parent drive + zip path (re-derive
  the archive lazily on restore); and the FE friendly errors, produced from the raw `ArchiveError` at the resolve
  boundary (this layer's typed `VolumeError` mapping is only a mid-browse backstop).
- **Live watching**: flip `listing_is_watched` to `true` and watch the parent `.zip` for external edits.
- **Mutation**: turn the mutation methods and the `'archive'` VolumeKind writable (add/delete/rename/mkdir/mkfile
  via temp+rename).
