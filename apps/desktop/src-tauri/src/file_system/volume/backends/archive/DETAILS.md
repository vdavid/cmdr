# Archive backend — details

Pull-tier docs for the archive backend: the `ArchiveVolume` `Volume` layer, routing/lifecycle, and remote-backed
archives. Must-know invariants live in [CLAUDE.md](CLAUDE.md). Read this before any non-trivial work here: editing,
planning, reorganizing, or advising.

The reading engine, the zip write side, and the live content watch each carry their own docs:

- Reading core (parse → tree, Zip Slip, DoS caps, sans-IO fsm, codecs, multi-format): [`read/DETAILS.md`](read/DETAILS.md).
- Zip mutation (temp+rename, encrypted-entry refusal, metadata preservation): [`mutation/DETAILS.md`](mutation/DETAILS.md).
- Live content watch (parent-dir watch, notification identity, remote-no-watch): [`watch/DETAILS.md`](watch/DETAILS.md).

## The `ArchiveVolume` layer (`volume.rs`)

`volume.rs` is the one file in this backend that touches the `Volume` trait. It maps the archive-native core
(`ArchiveIndex` / `ArchiveNode` / `ArchiveEntryReader` / `ArchiveError`) onto `FileEntry` / `VolumeReadStream` /
`VolumeError`, and holds an `Arc<dyn Volume>` **parent** (the volume physically storing the `.zip`), the archive path,
the display name, and an `Arc<ArchiveIndexCache>`.

**The parent seam.** Two answers a read-only archive can't give itself come from the parent:

- `lane_key()` returns `parent.lane_key()` — archive work must share the physical device's serialization lane (a zip on
  an SMB share shares that share's lane), never key on the archive path. Consequence: two zips on the same mount
  serialize; only zips on different mounts parallelize (the existing per-device write-serialization).
- `get_space_info()` delegates to the parent (see the decision below).

`ArchiveVolume` serves BOTH a local parent (`LocalFileSource` + `index_for_local`'s local stat+parse) and a remote one
(`VolumeByteSource` + `index_for_source`, freshness from the parent's metadata). It picks by
`parent.supports_local_fs_access()` (§ "Remote-backed archives (read path)"). The index and reader are unchanged either
way — both already speak `dyn ArchiveByteSource`.

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
live content watch is established ([`watch/DETAILS.md`](watch/DETAILS.md)), `false` otherwise. `supports_watching` stays
`false`: that flag drives the generic per-listing FSEvents dir-watcher, which can't watch an archive-inner path — the
archive self-watches its backing `.zip` instead.

**Bulk extract is one-pass for sequential archives.** Extracting a whole subtree from a compressed tar / solid 7z would
be O(n²) if the copy engine read it entry-by-entry (each `open_read_stream` re-decodes the prefix). It doesn't:
`Volume::extraction_is_sequential` declares the class, and the copy planner routes a sequential directory source through
a one-pass extractor (`Volume::open_sequential_extract` → `ArchiveIndex::open_subtree_extract`) that decodes the stream
ONCE. The extractor mechanism lives in [`read/DETAILS.md`](read/DETAILS.md) § "One-pass subtree extract"; the copy-engine
dispatch (create dirs from the tree, then a single decode pass for the files) lives in
[`write_operations/transfer/DETAILS.md`](../../../write_operations/transfer/DETAILS.md) § "One-pass sequential extract".
A plain `.tar` and zip are random-access and keep the per-entry path unchanged.

### Decision: `get_space_info` delegates to the parent volume

**Why**: An archive isn't a disk with its own free space. The pre-copy space check (`volume_copy.rs`) blocks a copy when
`dest.available_bytes < total_bytes`, so reporting zeros (or `available = 0`) would read as "disk full" and block a
paste with a spurious message instead of the correct read-only / `NotSupported` outcome. Any archive edit (temp+rename)
is built on the parent drive, so the parent's free space is the honest constraint AND a non-blocking answer. Delegating
is one line and stays correct when mutation turns on. Pinned by `get_space_info_delegates_to_the_parent`.

### Decision: `max_concurrent_ops = 1` for the read-only phase

**Why**: The core supports concurrent independent reads (each `ArchiveEntryReader` owns its `EntryFsm` and read offset
over a shared `pread` source, no shared cursor — `concurrent_reads_on_two_entries_are_independent` proves it through the
`Volume` API). But the copy engine's parallelism is a separate hint; a single stream in flight against an archive is
pinned for this phase. Raise it later if a real workload wants parallel extract.

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
once remote-backed archives grow, which wants a retryable classification, not "not supported". The one-time cost is
naming each new variant; the payoff is that no failure mode is ever classified by omission.

## Remote-backed archives (read path)

A zip on a direct SMB or MTP volume browses and extracts through the SAME `ArchiveVolume` as a local one — only the
byte supply differs. The read side is landed for both SMB and MTP; the write (edit) side pulls the archive local, edits,
and uploads (see [`mutation/DETAILS.md`](mutation/DETAILS.md) and `write_operations/DETAILS.md` § "Remote edit").

**Local vs remote is the parent's capability, not the path.** `ArchiveVolume::parent_is_local()` returns
`parent.supports_local_fs_access()`. A `LocalPosixVolume` parent (a plain drive OR an OS-mounted share) reports `true`
⇒ the fast local path: `LocalFileSource` `pread` + `ArchiveIndexCache::index_for_local` (local `std::fs` stat). SMB
(direct) and MTP report `false` ⇒ the remote path. The discriminator is deliberately NOT "can the archive path be
opened locally": a direct-SMB volume keeps its `/Volumes/...` mount point, so the `.zip` might still be openable through
the OS mount — but reading it that way defeats the direct connection and can block on a hung mount. Keying on the
capability forces the read through the parent volume.

**The bridge (`VolumeByteSource`, `volume.rs`).** The core's `ArchiveByteSource::read_at` is blocking (the parse and
every decompress run on `spawn_blocking`), but `Volume::read_range` is async. `VolumeByteSource` captures the tokio
runtime handle at construction (on the async executor, in `open_remote_source`) and `block_on`s the parent's
`read_range` inside the blocking read. Sound because `read_at` only ever runs on a `spawn_blocking` thread (never a
runtime worker), so `block_on` doesn't reenter the executor — the same bridge the viewer's archive extractor uses. It
clamps requests to the known size so rc-zip's read-ahead past EOF doesn't ask the backend for absent bytes.

**One tail read for the central directory (`TailCachedSource`, `read/source.rs`).** rc-zip's sans-IO fsm finds the EOCD
by reading backward from the end, then reads the central directory that precedes it — all near the tail. Without
caching, each of those `read_at`s would be a separate backend round-trip (slow on SMB/MTP). `TailCachedSource` wraps the
remote source and, on the first read that lands in the tail window (`DEFAULT_TAIL_CACHE_LEN`, 256 KiB), fetches the whole
tail once and serves every subsequent tail read from memory. A read before the window (an entry's bytes mid-file, or a
central directory larger than 256 KiB) falls through to a real ranged read. So a normal remote browse issues ONE
`read_range` for the CD parse (plus one `get_metadata` for size/mtime, which isn't a `read_range`); a giant directory
adds a second. Pinned by `read/source.rs`'s fetch-count tests and `volume_test.rs`'s
`remote_central_directory_parse_is_a_single_tail_read`. Entry extraction opens its own (uncached) source and streams the
entry's compressed range through the parent's `read_range` in bounded chunks.

**The positioned-read primitive (`Volume::read_range(path, offset, len)`).** Optional trait method, `NotSupported`
default. `LocalPosixVolume` implements it as a `pread` (`FileExt::read_at` loop), `MtpVolume` as one bounded
`GetPartialObject64` window opened at the offset, and `SmbVolume` via `smb2::FileReader` — an open handle that serves
positioned `read_at(offset, len)`s (the SMB analog of `pread`) then an explicit `close`. `SmbVolume::read_range` does one
`open_file_reader` → `read_at` → `close` per call: the `Volume` trait is stateless (no handle persists across calls), so
opening per call is the simple, correct shape. `FileReader` itself serves many reads per open, so caching an open reader
per path is a cheap future optimization if the round-trip ever matters; a normal remote browse issues only a handful of
`read_range`s (the `TailCachedSource` collapses the CD parse to ~1). Pinned end to end by
`smb_integration_test::smb_integration_archive_browse_and_extract_via_read_range` (browse + extract a zip on a real
Docker Samba share). The freshness key for the remote index cache comes from the parent's `get_metadata` (`size` +
second-granularity `modified_at` widened to nanos) — a remote `.zip` can't be `std::fs`-stat'd.

**`smb2::FileReader` (published crate).** `smb2` exposes `FileReader`, an open handle serving positioned
`read_at(offset, len)`s plus an explicit `close` — the primitive `SmbVolume::read_range` needs. `smb2 = "0.12.0"` in
`apps/desktop/src-tauri/Cargo.toml` pulls it straight from crates.io; there is no workspace `[patch.crates-io]` override.
(A hand-rolled `read_at` would need `Tree::close_handle`, which stays `pub(crate)`, so it'd leak an SMB handle per call —
`FileReader` owns the close.)

## Routing and lifecycle (`boundary.rs` + `VolumeManager::resolve`)

An `ArchiveVolume` is never constructed here directly in production — it's minted on demand by
`VolumeManager::resolve(volume_id, path)` when a path crosses a `.zip` boundary. The detector is `boundary.rs`:

- **Two tiers.** `archive_boundary_candidate` is a pure string split (extension-only, leftmost `.zip` component wins so a
  nested `a.zip/b.zip` treats the inner as a plain file — nested archives are out of scope). `confirm_archive_boundary`
  adds the I/O: the component must be a real FILE (a directory named `foo.zip` loses to normal navigation) whose first
  bytes are a zip signature (a mislabeled file isn't routed). Extension-only feeds `FileEntry.is_archive` at listing time
  (no per-entry byte read — that's a round-trip-per-file on a remote backend); confirm runs once at navigation time.
- **Confirm is parent-aware, so `resolve` is `async`.** `confirm_archive_boundary` is `std::fs`-only, so it can confirm
  only a LOCAL parent. For a REMOTE parent (direct SMB / MTP), the `.zip` isn't reachable through the local filesystem,
  so `VolumeManager::resolve` confirms through the parent volume's OWN I/O (`manager.rs::confirm_remote_archive_boundary`):
  `parent.get_metadata(zip)` (must be a file, not a directory) plus a four-byte `parent.read_range(zip, 0, 4)` sniffed
  with the SAME `bytes_start_with_zip_signature` predicate the local path uses — one shared detector, never forked.
  `resolve` picks local-vs-remote confirm by `parent.supports_local_fs_access()`, keeping the local path's zero-I/O fast
  filter (no archive-extension component ⇒ no I/O at all) byte-identical.
  - **Refuse-typed when the primitive is missing.** If `read_range` is `NotSupported` (a backend without a positioned
    read) or hits a transient remote fault, we can't rule the file out, so we route it anyway. The archive layer then
    re-attempts the read while parsing and surfaces a clean typed "unreadable archive" (`NotSupported`/`IoError` →
    `ArchiveUnreadable`) rather than mis-listing the `.zip` as a plain file — and it starts browsing for real, with no
    code change, the moment the backend's `read_range` works. Only a *successful* read whose bytes AREN'T a zip
    signature (a genuinely mislabeled remote file) declines the route.
  - **The sync `resolve_local_only`** confirms only LOCAL boundaries (no async I/O) for the one caller that can't
    `.await`: the write-op fresh-listing oracle (`listing::caching::try_get_watched_listing`), which runs on sync
    recursive scan walkers. That oracle guards a REMOTE archive-inner path itself (a non-local parent's volume-level
    `listing_is_watched` would falsely claim freshness for an archive whose content watch is local-only and never
    established), declining the cache so the pre-op scan reruns honestly.
- **`SUPPORTED_ARCHIVE_EXTENSIONS` is the one source of truth** shared by `is_archive` and boundary detection; confirm's
  magic check carries a sibling signature per format.
- **Write-routing reuses the same parent-aware confirm.** The write seams (delete / rename / create / copy-out-source)
  don't `resolve` — they need only a yes/no "is this archive-inner?" — so they call
  `VolumeManager::path_is_inside_archive` / `path_crosses_archive_boundary` (in `manager.rs`), the async siblings of the
  `std::fs`-only `boundary.rs` predicates. Same `supports_local_fs_access()` fork, same `confirm_remote_archive_boundary`
  for a remote parent. Without them a write inside a remote zip falls through to the parent volume and errors; see
  `write_operations/DETAILS.md` § "Reaching the edit driver: parent-aware write-routing".

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
seams are the archive-edit routing points when the target is a writable zip. The live watch turns the edit's final
atomic rename into the post-edit refresh (see [`watch/DETAILS.md`](watch/DETAILS.md)).

## Testing

`volume_test.rs` (colocated with `volume.rs`) drives `ArchiveVolume` against real zips written to a temp file (the local
source needs a real path): list/metadata/exists round-trips incl. synthetic dirs and the transparent path, absolute-path
prefix stripping, the progress tick, the cancelable-listing default, streaming extract + the at-offset tail, concurrent
two-entry reads, `scan_for_copy` (subtree / single file / missing), every-mutation-unsupported (incl. the
`create_directory_all` guard), encrypted/corrupt/non-zip typed errors through the `Volume` API, the parent `lane_key` /
`get_space_info` delegation, and the capability flags. It uses the shared `test_fixtures.rs` builders (at the archive
root). `boundary.rs` tests the per-format magic (incl. plain-tar ustar-at-257) and the double-extension split. The
reading-core, mutation, and watch tests live with their modules.

## Left for the follow-up milestones

`ArchiveVolume` (browse + extract + `scan_for_copy`) and backend routing (§ "Routing and lifecycle") are landed:
`VolumeManager::resolve`, the shared `boundary.rs` detector, the archive LRU, the read-only write guards, the live
content watch, and zip mutation (browse + extract + edit, local and remote-hosted). What's still ahead (sequencing lives
in `/docs/specs/later/archive-browsing-polish.md`):

- **Open-with-external-app for a file INSIDE an archive (deferred).** Enter on a file inside a `.zip` still opens the
  VIEWER (bounded temp-extract), not the OS default app. Extract-then-launch isn't a clean reuse of
  `file_viewer/archive_extract.rs`: that extractor is viewer-`pub(super)`-scoped and its temp is reaped on VIEWER
  SESSION close, whereas a detached launched app holds the file for an unknown lifetime and has no close event to hook —
  it needs its own extract-and-persist-until-startup-reaper lifecycle. Deferred deliberately; the viewer interim stands.
- **NON-LOCAL sources INTO a zip** — an MTP/SMB source copied into an archive (`route_archive_copy_into` still requires a
  `local_path()` source; the archive itself may be remote).
