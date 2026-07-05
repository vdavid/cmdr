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

- `LocalFileSource`: `positioned_io::RandomAccessFile` over the real file. Backs a LOCAL archive.
- `VolumeByteSource` (in `volume.rs`, the `Volume`-aware layer): backs a REMOTE archive (direct SMB / MTP). It bridges
  the blocking `read_at` to the parent volume's async `Volume::read_range` by `block_on`-ing a captured runtime handle
  from inside the `spawn_blocking` parse/decompress context. No change to the parser or reader. See
  § "Remote-backed archives (read path)".
- `TailCachedSource` (a decorator, `source.rs`, `Volume`-free): caches the file's tail so the central-directory parse
  (rc-zip hunts the EOCD + directory near the end) is ONE ranged read of a slow backend, not many. Applied only to the
  remote source.

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

## Remote-backed archives (read path)

A zip on a direct SMB or MTP volume browses and extracts through the SAME `ArchiveVolume` as a local one — only the
byte supply differs. The read side is landed for both SMB and MTP; the write (edit) side is a follow-up (see
`/docs/specs/archive-browsing-plan.md` § M5 and § "Left for the follow-up milestones" below). SMB's ranged read rides a
**temporary `smb2` source patch** until the primitive is published — see "The positioned-read primitive" below.

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

**One tail read for the central directory (`TailCachedSource`, `source.rs`).** rc-zip's sans-IO fsm finds the EOCD by
reading backward from the end, then reads the central directory that precedes it — all near the tail. Without caching,
each of those `read_at`s would be a separate backend round-trip (slow on SMB/MTP). `TailCachedSource` wraps the remote
source and, on the first read that lands in the tail window (`DEFAULT_TAIL_CACHE_LEN`, 256 KiB), fetches the whole tail
once and serves every subsequent tail read from memory. A read before the window (an entry's bytes mid-file, or a
central directory larger than 256 KiB) falls through to a real ranged read. So a normal remote browse issues ONE
`read_range` for the CD parse (plus one `get_metadata` for size/mtime, which isn't a `read_range`); a giant directory
adds a second. Pinned by `source.rs`'s fetch-count tests and `volume_test.rs`'s
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

**`smb2::FileReader` ships via a TEMPORARY source patch (David-gated).** Published `smb2` 0.11.4 has no public
positioned read (its `Tree::close_handle` is `pub(crate)`, so a hand-rolled Cmdr `read_at` would leak an SMB handle per
call). The primitive lives on the local `david/read-at` smb2 worktree, wired in by a `[patch.crates-io]` override in the
**workspace-root `Cargo.toml`**. Do NOT land that patch on Cmdr `main`. Merge-time checklist:

1. In smb2: land `david/read-at` on smb2 `main`, then publish a new `smb2` version (external action — David runs it).
2. In Cmdr: bump `smb2 = "…"` in `apps/desktop/src-tauri/Cargo.toml` to the published version.
3. In Cmdr: delete the whole `[patch.crates-io]` section from the workspace-root `Cargo.toml`.
4. `cargo update -p smb2 --precise <version>` (allowed for a ≥3-day-old or first-party pin) and re-run `pnpm check rust`.

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

### Decision: remote archives have NO live watch — freshness is "as of last read"

The content watch is a LOCAL `notify` watch on the backing `.zip`'s parent directory. A REMOTE parent (direct SMB / MTP)
has no local path for `notify` to watch, so `start_watch` returns `None` and a remote `ArchiveVolume`'s
`listing_is_watched` is permanently `false`. Two consequences, both correct-by-construction rather than a gap to close:

- **The write-op fresh-listing oracle never serves a remote archive listing from cache.** `listing_is_watched == false`
  means every pre-flight scan of a remote archive re-reads it honestly (and `try_get_watched_listing` also guards a
  remote archive-inner path explicitly — see `volume/CLAUDE.md` § `resolve`). So a copy/delete inside a remote archive
  always sizes against a fresh parse, never a stale cache.
- **No push-refresh for an EXTERNAL edit of a remote `.zip`.** Nothing forwards an out-of-band change (another app
  rewriting the zip on the share/device) to an open remote-archive listing; the pane shows the archive as of its last
  read until the user re-navigates or refreshes (F5). This is the accepted freshness model: a remote archive changes
  rarely, and the app's OWN edit refreshes the pane through the normal listing-cache path (the edit's driver invalidates
  the `(parent_id, archive_path)` listing on completion, same as any remote write). Deliberately NOT worth an
  SMB-watcher event on the `.zip` for v1 — the `(path, size, mtime)` cache key still forces a re-parse on the next read,
  so a stale render can never outlive a navigation. Revisit only if remote multi-writer scenarios become common.

**Interaction with the mutation milestone (M4).** When zip mutation lands via temp+rename, the edit's FINAL atomic rename
over `foo.zip` is a change event this watch catches — that IS the desired post-edit refresh. A concurrent browse in the
other pane reading the archive mid-edit sees either the old-complete or the new-complete file, never a torn read, because
the rename is atomic on one filesystem: the reader's `LocalFileSource` opens whichever inode the rename has published.
The read isn't serialized on the edit's lane, but it doesn't need to be — atomicity, not lane serialization, is what
prevents the torn read. The `(path, size, mtime)` key plus the watch's `cache.clear()` guarantee the post-rename read
re-parses the new file.

## Zip mutation (`mutator.rs`, temp+rename safe-overwrite)

The write side. `ArchiveMutator::apply(archive_path, changeset, hooks)` applies a batched `Changeset`
(`add` / `mkdir` / `delete` / `rename`; a mkfile is an add of empty bytes) by building the FULL new archive into a
same-directory sibling temp (`foo.zip.cmdr-tmp-<uuid>`), then atomically renaming it over the original. It is
`Volume`-free and manager-free (like the read core): the write-ops `ArchiveEditOperation` driver wraps it with the real
event sink, pause gate, and cancel intent via the `MutationHooks` seam. Full driver wiring:
`write_operations/DETAILS.md` § "Archive edits".

- **Temp+rename, not append-in-place.** `zip`'s `ZipWriter::new_append` overwrites the old central directory, so a
  cancel mid-edit corrupts the archive (verified: truncating before the new EOCD yields "Could not find EOCD"; the
  original does NOT survive). Building fresh into a temp and renaming is the app's mandated safe-overwrite and the only
  shape where cancel is genuinely free (abandon the temp, no rollback ledger). The original is byte-for-byte untouched
  until the final rename; a cancel or crash at any earlier point leaves it fully readable.
- **Retained entries copy verbatim** via `raw_copy_file_rename` (no decompress/recompress); only added files
  stream-compress (chunked, never whole-buffered — the add chunk is also the pause/cancel granularity mid-file). An
  added file carries its SOURCE's modification time into the entry (`add_entry_options`), not the write time — zip stores
  it as MS-DOS date/time (2-second granularity, 1980–2107 range; an mtime outside that range keeps the default). The
  decompose is done in UTC because `rc-zip` reads the DOS fields back as UTC, so the mtime round-trips through the index
  parse.
- **Metadata preservation (the archive FILE, not the entries).** A rewrite yields a fresh inode, so the original `.zip`'s
  mode, timestamps, and xattrs are carried onto the temp before the swap: macOS `copyfile` with
  `COPYFILE_STAT | COPYFILE_ACL | COPYFILE_XATTR`. `COPYFILE_STAT` carries mode and all timestamps INCLUDING the
  creation/birth date (`st_birthtime` lives in the inode, not an xattr); `COPYFILE_XATTR` carries Finder tags,
  quarantine, and `com.apple.FinderInfo` VERBATIM — a faithful copy that keeps the custom-icon flag, so the `tags.rs`
  FinderInfo gotcha doesn't apply here. mode+mtime+xattr elsewhere. Best-effort: metadata loss never fails a data-safe
  edit.
- **External replacement of the `.zip` between planning and the final rename is last-writer-wins.** The changeset is
  planned against the archive as parsed at plan time; if an outside process rewrites the same `.zip` before the mutator's
  atomic rename, that outside write is simply overwritten by our temp (and vice-versa — a rename that lands after ours
  wins). This is acceptable for the single-user local model Cmdr targets; there's no cross-process lock. It's stated here
  so a future multi-writer scenario revisits it deliberately rather than assuming a guard exists.
- **Decision — refuse to retain an encrypted entry (data-safety, deviates from the plan).** `zip`'s raw copy
  reconstructs an entry's options from `ZipFile::options()`, which does NOT carry the traditional-PKWARE encryption GP
  flag. So a retained encrypted entry would keep its ciphertext bytes but lose the "encrypted" header bit — semantically
  corrupt (a reader hands back ciphertext as plaintext). `apply` therefore returns `EncryptedEntryRetained` for any edit
  that would KEEP an encrypted entry, leaving the original untouched. Deleting an encrypted entry is allowed (it isn't
  retained). Editing encrypted archives is out of scope in v1 (matches the plan's "encrypted: detect + reject"). The
  plan's "raw_copy retains encrypted entries byte-for-byte" claim is false against `zip` 8.6 (verified by
  `mutator_test.rs`); this refusal is the resolution.
- **Leftover policy — no startup reaper.** A leftover `foo.zip.cmdr-tmp-*` is always an ABANDONED build (the original is
  intact), so it's harmless. `apply` reaps siblings of the target at the START of the next edit of that archive (before
  building its own fresh-uuid temp), which is sufficient; a cancel/error removes its own temp immediately via an RAII
  guard.
- **Deletes/renames reshape the retained set.** A delete drops a file or a whole subtree (component-wise match, so
  `foo` never catches `foobar`); a rename rewrites a subtree prefix. Both are computed per original entry in one pass
  (`plan_new_name`); deletes win over renames.

`mutator_test.rs` is the red-first TDD anchor for every data-safety property: round-trips (add/delete/rename/mkdir/mkfile
verified via our reader AND external `unzip -t`), cancel-midway-leaves-the-original-intact-with-no-temp, a leftover temp
reaped on the next edit, the merge invariant (a delete keeps siblings' raw compressed bytes byte-identical), metadata
(mode/mtime/xattr) survival, pause-parks-mid-add-then-completes-on-resume, and the encrypted-entry refusal.

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
- **Mutation is landed on the backend** (§ "Zip mutation"): `mutator.rs` (temp+rename) plus the write-ops
  `ArchiveEditOperation` driver, and the write-guard seams (§ "Routing and lifecycle") are now archive-edit routing
  points — mkdir/mkfile/rename/delete inside and copy/move INTO a zip run as managed edit ops. `ArchiveVolume`'s own
  mutation methods deliberately STAY `NotSupported`: routing is path-based and backend-side (the driver drives the
  mutator directly from the archive path), so nothing calls `Volume::create_file`/`delete`/`rename` on an archive. The
  edit's final atomic rename is the change event the live watch (§ "Live content watch") turns into the post-edit
  refresh. Still ahead: the interactive in-archive conflict prompt (ApplyToAll + oneshot — the non-interactive policy
  ships now), the compound move ACROSS the boundary (out-of-zip = extract + archive-delete), non-local sources into a
  zip, and flipping the FE `'archive'` VolumeKind writable.
