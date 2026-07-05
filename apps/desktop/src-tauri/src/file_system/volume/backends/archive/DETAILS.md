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
- `watch.rs`: the live content watch on the backing `.zip` (parent-directory `notify` watch + event filter). See
  § "Live content watch".
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
`max_concurrent_ops = 1`; `supports_export`/`supports_streaming = true`. `listing_is_watched` is `true` only while the
live content watch is established (§ "Live content watch"), `false` otherwise. `supports_watching` stays `false`: that
flag drives the generic per-listing FSEvents dir-watcher, which can't watch an archive-inner path — the archive
self-watches its backing `.zip` instead.

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
The user-facing "damaged archive" copy is NOT produced here: the listing seam (`listing/streaming.rs`) turns a failed
`.zip` browse into `ListingErrorReason::ArchiveUnreadable` from the PATH + this collapsed error kind (an integrity
collapse — `NotSupported`/`IoError` — on a path that `archive::boundary::path_targets_archive_file` says targets a real
archive file). A valid archive with a missing inner path stays `NotFound` (not an integrity fault). This mapping stays
message-string-free and keeps only the coarse family the FE needs, because a SINGLE combined message ("damaged,
encrypted, or an unsupported format") covers the whole family — the same wording the viewer uses
(`viewer.error.archiveUnreadable`), so recovering the fine `ArchiveError` distinction downstream isn't needed.

The match is **exhaustive on purpose — no wildcard**. It's a compile-time tripwire (the repo convention, per
`analytics.rs`): a new `ArchiveError` variant must fail to compile here and force a conscious mapping. A catch-all
`_ => NotSupported` would silently mis-serve a future *non-rejection* variant — say a transient remote-source error
once remote-backed archives land, which wants a retryable classification, not "not supported". The one-time cost is
naming each new variant; the payoff is that no failure mode is ever classified by omission.

## Routing and lifecycle (`boundary.rs` + `VolumeManager::resolve`)

An `ArchiveVolume` is never constructed here directly in production — it's minted on demand by
`VolumeManager::resolve(volume_id, path)` when a path crosses a `.zip` boundary. The detector is `boundary.rs`:

- **Two tiers.** `archive_boundary_candidate` is a pure string split (extension-only, leftmost `.zip` component wins so a
  nested `a.zip/b.zip` treats the inner as a plain file — nested archives are out of scope). `confirm_archive_boundary`
  adds the I/O: the component must be a real FILE (a directory named `foo.zip` loses to normal navigation) whose first
  bytes are a zip signature (a mislabeled file isn't routed). Extension-only feeds `FileEntry.is_archive` at listing time
  (no per-entry byte read — that's a round-trip-per-file on a remote backend); confirm runs once at navigation time.
- **`SUPPORTED_ARCHIVE_EXTENSIONS` is the one source of truth** shared by `is_archive` and boundary detection; the later
  tar/7z read milestone extends it (and confirm's magic check gains sibling signatures).

**resolve returns the FULL path, unchanged.** The decision (over returning a stripped inner path): `ArchiveVolume`
already maps a volume-namespace path to its inner key via `inner_path()`, `node_to_entry` builds full paths, and
`root()` is the `.zip` — so a full-path passthrough makes every adoption site uniform (`resolve` only swaps the volume,
never rewrites the path) and keeps the listing cache, the FE, and the entries all speaking full paths.

**Routing id vs display id.** The registry id is `archive-{hash(canonical zip path)}`, backend-internal only: it never
enters FE state, history, persistence, or MCP sync. The FE holds the PARENT drive id (display), and
`resolve_path_volume` / `resolve_location` return that parent drive for an archive-inner path (resolved from the `.zip`'s
real location, since the inner path isn't a real FS path). So the listing cache keys on the parent id too, and the
downstream re-read sites (`notify_full_refresh`, `try_get_watched_listing`, `watcher::handle_directory_change`,
`refresh_listing`) RE-RESOLVE from `(parent_id, full_path)` rather than `get`-ing a stored archive id — which is what
makes LRU eviction safe.

**Archive LRU (cap 16).** `VolumeManager` tracks archive registration recency; resolving past the cap unregisters the
least-recently-resolved archive (its `ArchiveIndexCache` drops with the volume). Eviction is harmless because every read
site re-resolves: an evicted archive re-registers lazily on the next navigation (`ArchiveVolume::new` is cheap; the index
re-parses on demand). No frontend refcount exists — the FE never holds an archive id, so there's nothing to refcount.

**Read-only write guards.** Because routing is path-based, the write seams that bypass `VolumeManager` guard themselves
against an archive target: the local `copy`/`move`/`delete`/`trash` fast-paths and the cross-volume dest reject with
`WriteOperationError::ReadOnlyDevice`; the managed instant-op forks (`create_directory_core`, `create_file_core`,
`rename_managed`) return a clean refusal. Move also rejects an archive SOURCE (a move deletes the source side). These
seams become archive-edit routing when mutation lands.

## Live content watch (`watch.rs`)

An external edit to the backing `.zip` (an editor rewriting it, a `cp` over it, a future in-app mutation's final rename)
refreshes any open listing inside the archive. The watch lives on the `ArchiveVolume`.

**Watch the parent directory, not the file.** macOS editors and every safe-overwrite (including this app's own planned
temp+rename mutation) replace the file's inode: write `foo.zip.tmp`, then atomically rename over `foo.zip`. A `notify`
watch pinned to the OLD inode goes silent after such a swap. So `start_watch` watches the archive's PARENT DIRECTORY
(`RecursiveMode::NonRecursive`) — the directory inode is stable across the swap, so no re-arming is needed — and filters
the directory's child events down to the archive file (`event_path_targets_archive`). The filter compares on the
firmlink-normalized forms (`indexing::firmlinks::normalize_path`), the same rebasing `file_system::watcher` does, because
FSEvents reports canonical `/private/tmp/…` paths while the archive path is the user-navigated `/tmp/…` form. This
mirrors the local listing watcher's own parent-directory-non-recursive shape.

**Notification identity: parent drive id + full path, never the archive id.** The listing cache keys archive listings on
the PARENT DRIVE id plus the full `/…/foo.zip/inner` path (see § "Routing and lifecycle"). On a matching event the
callback drops the stale parsed index (`cache.clear()`) and calls `caching::refresh_archive_listings(parent_drive_id,
archive_path)`, which finds every open listing at or inside the archive path (`Path::starts_with`, component-wise — so
`/a/foo.zip` matches the root and inner listings but not a `/a/foo.zipper` sibling or the containing `/a`) and re-reads
each through `notify_full_refresh`. That re-resolves `(parent_id, inner_path)` back to this `ArchiveVolume`, so an
LRU-evicted archive re-registers lazily. It deliberately does NOT go through `notify_directory_changed`: that runs the
drive-index sync (`apply_smb_change`) up front, and an archive-inner path isn't a real filesystem path, so feeding it to
the index would be meaningless.

**Cache invalidation.** The `(path, size, mtime)` key already misses after any edit, so `cache.clear()` isn't needed for
correctness — but it releases the old `Arc<ArchiveIndex>` instead of leaking one parsed index per edit. Clearing runs in
the (synchronous) `notify` callback before the async refresh spawns, so the re-read re-parses the new bytes.

**Off the executor.** The debouncer callback runs on notify-rs's own thread, which has no Tokio runtime, so it uses
`tauri::async_runtime::spawn` (never `tokio::spawn`, which would panic) — the same rule as `file_system::watcher`.

**Mid-write safety (keep the old listing).** A writer mid-rewrite leaves a truncated central directory. On such a
refresh, `list_directory` errors (`ArchiveError::Corrupt`/`NotAnArchive` → `VolumeError`), and `notify_full_refresh`
returns early WITHOUT touching the cache — so the pane keeps its last-good entries rather than blanking, and the next
event (when the write settles) retries. The damaged-archive banner is produced only at NAVIGATION time (the listing seam,
§ "Decision: typed `ArchiveError → VolumeError` mapping"), never from this refresh path, so a transient mid-write never
flashes an error. Pinned by `a_truncated_midwrite_archive_keeps_the_previous_listing`.

**Lifecycle (leak-free).** `VolumeManager::resolve` starts the watch exactly once, gated on the `register_if_absent`
winner, so repeated resolves of an already-registered archive don't churn watchers. The `ArchiveContentWatch` handle
lives on the `ArchiveVolume`; when the archive LRU evicts the volume (`unregister` drops the registry's `Arc`) or the app
tears down, the handle drops and the `Debouncer`'s own `Drop` stops the OS watch. A held `Arc` (an in-flight read)
keeps the watch alive until the last reference drops — harmless (a re-resolve after eviction starts a fresh watch; two
briefly overlap, both fire idempotent refreshes). `active_watch_count` (incremented on start, decremented in the handle's
`Drop`) lets `lru_eviction_releases_the_archive_and_its_watch` prove eviction leaks no watcher. `listing_is_watched` is
`true` only while the handle is present, so a listing never claims freshness the backend can't back; if the watch fails
to establish (`notify` refuses the path — e.g. a non-local parent), it stays `false`.

**Interaction with the mutation milestone (M4).** When zip mutation lands via temp+rename, the edit's FINAL atomic rename
over `foo.zip` is a change event this watch catches — that IS the desired post-edit refresh. A concurrent browse in the
other pane reading the archive mid-edit sees either the old-complete or the new-complete file, never a torn read, because
the rename is atomic on one filesystem: the reader's `LocalFileSource` opens whichever inode the rename has published.
The read isn't serialized on the edit's lane, but it doesn't need to be — atomicity, not lane serialization, is what
prevents the torn read. The `(path, size, mtime)` key plus the watch's `cache.clear()` guarantee the post-rename read
re-parses the new file.

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

`watch.rs` unit-tests the pure event filter (`event_path_targets_archive`: exact match, the `/private` firmlink
normalization, sibling and prefix-similar rejection). `watch_integration_test.rs` drives the whole refresh through
`VolumeManager::resolve` + `LISTING_CACHE` against real temp zips: an on-disk edit reflected in the listing while an
outside listing is left untouched (scoping), a truncated mid-write keeping the previous listing, the real-notify
end-to-end refresh (polls a condition with a generous timeout, no fixed sleep), and LRU eviction releasing the archive's
watch (`Arc::strong_count` drops to the test's own after eviction).

## Left for the follow-up milestones

`ArchiveVolume` (browse + extract + `scan_for_copy`, read-only) exists in `volume.rs`, and backend routing (§ "Routing
and lifecycle" above) is landed: `VolumeManager::resolve`, the shared `boundary.rs` detector, the archive LRU, and the
read-only write guards. What's still ahead (sequencing lives in `/docs/specs/archive-browsing-plan.md`):

Landed since: the FE `'archive'` capabilities `VolumeKind` (kind-from-path), the Enter-into-archive fork, the
breadcrumb/path-bar `…/foo.zip/inner` rendering, the bounded temp-extract viewer preview, the listing-path
`ArchiveUnreadable` friendly copy (§ "Decision: typed `ArchiveError → VolumeError` mapping"), the M2 Enter-behavior
menu + per-format Settings (`docs/specs/archive-browsing-plan.md` § M2), and the live content watch (§ "Live content
watch": `listing_is_watched` reflects it, the backing `.zip` is watched for external edits). What's still ahead:

- **Open-with-external-app for a file INSIDE an archive (deferred, M2 carried-over item b).** Enter on a file inside a
  `.zip` still opens the VIEWER (bounded temp-extract), not the OS default app. Extract-then-launch isn't a clean reuse
  of `file_viewer/archive_extract.rs`: that extractor is viewer-`pub(super)`-scoped and its temp is reaped on VIEWER
  SESSION close, whereas a detached launched app holds the file for an unknown lifetime and has no close event to hook —
  it needs its own extract-and-persist-until-startup-reaper lifecycle. Deferred deliberately; the viewer interim stands.
- **Mutation**: turn the mutation methods and the `'archive'` VolumeKind writable (add/delete/rename/mkdir/mkfile
  via temp+rename). The write-guard seams (§ "Routing and lifecycle") become the mutation routing points, and the edit's
  final atomic rename is the change event the live watch (§ "Live content watch") turns into the post-edit refresh.
