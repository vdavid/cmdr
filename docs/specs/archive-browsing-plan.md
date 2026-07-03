# Archive browsing and editing (zip-first) — implementation plan

Status: in execution. Reviewed (3 adversarial rounds — 6 BLOCKERs fixed, 11 refinements folded, rc-zip read-side
adopted + consistency verified), all decisions resolved, refreshed 2026-07-03 against the write-ops/commands refactor
wave. Owner: David. Worktree: `.claude/worktrees/archive-browsing`, branch `david/archive-browsing`.

## Execution ground rules (locked with David, 2026-07-03)

- **Everything lands on the worktree branch.** No merge to `main` until the ENTIRE feature is done and David has
  reviewed it. Milestones end green and demoable, but they accumulate on `david/archive-browsing`.
- **M1a starts immediately; M1b is gated on the `FilePane.svelte` drain refactor** (its hook points `handleNavigate`,
  `handleOpenOrParentKey`, and `navigateToParent` are being extracted to `pane/` modules). David signals when the drain
  has landed and M1b can start. The `DualPaneExplorer.svelte` drain ideally also lands before M1b (`swapDualPaneState`
  is on decision 1's threading list) but isn't a hard gate.
- **Human-eyes review and translations are batched after the milestones.** David reviews the M2 Enter menu, the Archives
  settings section, and all user-facing copy once the whole feature works, not per milestone. English catalog keys still
  land with each milestone (`i18n-coverage` is a build error); other languages come after.

## What we're building

Press Enter on a `foo.zip` and step inside it as if it were a folder. The path bar reads `/path/to/foo.zip/inner-dir`,
transparently, with no scheme prefix and no separator glyph. The containing drive stays the selected volume. Full
two-way file ops work on zips: copy/move/delete files out of, into, and within an archive, with the **same**
cancellation, pause/resume, queueing, and progress/ETA the app already gives local, MTP, and SMB transfers. Live-watch
the archive file so external edits refresh the view. Stay pure-Rust on the backend, and keep it fast where the format
allows.

Large, so it's milestoned. M1 (read + extract-out, incl. previewing files inside) is independently shippable and
delivers most of the value; mutation (M4) is the second big lift. Each milestone ends green and demoable.

## Scope (locked with David)

In scope:

- **Formats**: ZIP first-class (browse + extract + mutate). `tar`/`tar.gz`/`tar.xz`/`tar.bz2`/`tar.zst` and `7z`
  read-only (browse + extract). All pure-Rust.
- **Transparent path**: `/path/to/foo.zip/inner`, archive renders exactly like a folder. No volume-selector entry.
- **Enter behavior**: per-format three-way Browse | Open | Ask, via an Enter-key popup when set to Ask, plus a settings
  section. Covers real archives AND macOS bundles (`.app`, `.bundle`, `.framework`).
- **Mutation (zip only)**: add/delete/rename, plus creating new folders and files inside the archive (mkdir/mkfile), via
  temp+rename safe-overwrite (see Piece 3 — the in-place append method is a later optimization, not the default).
  Cancellation (no rollback needed), pause/resume, queueing, progress/ETA — all via the existing operation manager.
- **Remote-backed archives**: a zip on SMB or MTP is browsed/edited through the same `Volume` abstraction as a local one
  ("even if slower"). MTP in-place editing is a stretch (M6).
- **Compaction**: moot under temp+rename (each edit already rewrites compactly, no dead space). Returns with M-append
  (the fast in-place path that produces dead space): a checkbox on transfer dialogs + a Behavior › File ops slider
  (0–100%, 5% steps, default 20%).
- **Live watching**: watch the archive file on its parent volume; invalidate the archive listing on change.

Out of scope (v1): RAR, multipart/spanned archives, nested archives (inner archive offers "Open with external app"
only), 7z/tar mutation, encrypted archives (detect + reject), in-archive search/indexing.

## Review corrections (what round 1 changed)

The first plan made three "for free" claims that are false against the actual code; all are now fixed in the design
below. Recorded here so the rationale isn't lost:

1. **Append-incremental is not crash/cancel-safe with `zip` 8.6** (empirically: `ZipWriter::new_append` overwrites the
   old central directory; truncating before the new EOCD yields "Could not find EOCD" — the archive is unreadable, the
   original does NOT survive). → Default mutation is the app's temp+rename safe-overwrite. Append-past-EOF is a later,
   hand-rolled, reader-compat-tested optimization.
2. **`open_read_stream_at_offset` is implemented only on MTP**; the trait default is `Err(NotSupported)`. The `zip`
   crate's `ZipArchive<R>` requires sync `Read + Seek`. → **Superseded by the rc-zip decision** (Piece 1): reads use
   `rc-zip` (sans-IO, ranged), so neither a forward `VolumeReadStream` nor a sync `Read+Seek` adapter is needed — local
   and remote both feed rc-zip ranged reads.
3. **One routing interception point is insufficient**: ~18 commands resolve `VolumeManager::get(volume_id)` without the
   path, and several (`viewer_open*`, local `copy_files`/`move_files`/`trash_files`, `stat_paths_kinds`,
   `go_to_path::resolve`) have no `volume_id` at all. → A new path-aware `VolumeManager::resolve(volume_id, path)`
   adopted at every site, plus per-command patches.

## Core architecture

Three pieces. Guiding insight unchanged and confirmed by review: reads are filesystem-shaped (a `Volume` fits), writes
are a transactional batch transform (not per-path filesystem mutation), and the generic operation-lifecycle layer
(`manager::spawn_managed`, `DeferredStart`) already gives cancel/pause/queue/progress regardless of the work's shape.

### Piece 1 — `ArchiveVolume` (read side), a read-only `Volume` (new `VolumeKind`)

New backend under `file_system/volume/backends/archive/` implementing the `Volume` trait per the Tier 1→3 checklist.

- **Tier 1 (browse)**: `list_directory`, `get_metadata`, `exists`, `is_directory`, `get_space_info`, `name`, `root`.
  Built from the directory index (zip central directory; one-time sequential scan for tar/7z). Synthesize the directory
  tree from entry path prefixes (most archives have no explicit dir entries); cache it.
- **Tier 2 (extract-out)**: `supports_streaming()`/`supports_export() = true`, `open_read_stream` /
  `open_read_stream_at_offset` decompress an entry on the fly (chunked, never whole-buffer). **Also implement
  `scan_for_copy`** — default is `NotSupported` and the copy engine calls it to size and enumerate a directory
  extraction; extract-out is NOT free without it. (`scan_for_copy_batch`'s default already loops `scan_for_copy`, so the
  batch comes free — implement only the single.)
- **Backing bytes — read via `rc-zip` (sans-IO), not the `zip` crate.** The `zip` crate's `ZipArchive<R>` needs sync
  `Read + Seek`, which would force a blocking seek-adapter over async ranged reads for remote. Instead use **`rc-zip`**
  (sans-IO: its state machine requests byte ranges, the caller supplies them under any I/O model). Drive it from the
  **parent `Volume`'s ranged reads** — local and remote identical, async-native (`rc-zip-tokio`), no fake `Read+Seek`
  adapter. Local parent can read via `parent.local_path()` + `std::fs::File`; remote parents (M5) need a parent
  ranged-read primitive added to `LocalPosixVolume`/`SmbVolume` (MTP already has `GetPartialObject64`). The
  `ArchiveVolume` holds an `Arc<dyn Volume>` parent + the archive path and feeds rc-zip ranged reads. (`rc-zip` is
  read-only — the write side uses the `zip` crate; see Piece 3.)
- **Identity & lifecycle (corrected — this was a leak)**: registered on demand via `register_if_absent` under a stable
  id `archive-{hash(canonical_zip_path)}`, `root()` = the real `.zip` path. **Refcount it.** Panes/tabs/history entries
  that point at the archive id hold a reference; unregister on last release (mirror the existing search-results snapshot
  release in `navigation-history.ts` `droppedEntries`). Without this, browsing 100 zips leaks 100 volumes + parents +
  index caches forever. An LRU cap is the backstop.
- **`lane_key()` (corrected)**: return the **parent volume's `lane_key()`**, NOT the archive path. An archive on SMB
  must share the SMB session's lane or the op manager runs archive edits in parallel with other ops on the same
  session/USB pipe — the exact contention lanes prevent. Consequence (budget is 1 per lane): two different zips **on the
  same local mount serialize** (the mount root is the lane); only zips on **different mounts** parallelize. This is the
  existing per-device write-serialization, not new behavior — and it already guarantees same-zip serialization, so no
  "finer sub-key" is needed or possible (a sub-key can only add serialization within a budget-1 lane).
- **Capability flags — set explicitly, don't inherit defaults**: `local_path() = None`, `space_poll_interval() = None`
  (default is `Some(2s)` — would poll a read-only archive), `supports_export`/`supports_streaming = true`,
  read-only/virtual semantics so `get_space_info` doesn't read as "disk full" and block paste.
- **New `'archive'` `VolumeKind`** on the FE (see decision 1) so `capabilitiesFor(archive-id)` doesn't fall through to
  the `local` default and advertise `canPasteInto`/`canRenameInPlace = true` for a read-only zip.
- Friendly errors (typed, word-free): not-a-real-archive, encrypted (reject), unsupported (RAR/spanned),
  corrupt/truncated central directory.

### Piece 2 — `ArchiveEditOperation` (write side), a batch transform

Mutation is NOT per-path `Volume::create_file`/`delete`/`rename` (every zip change rewrites the shared central
directory; per-file would be N transforms). The operation planner detects an archive destination/source and builds one
changeset `{ add, delete, rename }` applied in a single pass by a new `ArchiveEditOperation`.

- **Driver is net-new (~85 lines), not "mirror delete."** The volume-delete branch (`write_operations/mod.rs`) is
  bespoke inline ceremony (UUID, state, `ManagedTaskGuard`, `WriteSettledGuard`, terminal match, `on_settled`); the
  manager provides only queue/lanes/busy-set/settle. Cancel/pause/conflict are re-implemented per driver by convention.
  Budget a new driver of that size. The event sink is now injected at the IPC edge (starters take
  `Arc<dyn OperationEventSink>`; `TauriEventSink` lives only in the command layer), so the driver is headless-testable
  from day one — TDD it without Tauri.
- **Plugs into `manager::spawn_managed`** via a `DeferredStart` — confirmed generic, no scheduler change. Inherits queue
  (parent lane), busy-volumes, and the `write-settled` contract.
- **Pause/cancel** wired in the driver: `PauseGate` checks between entries (and between chunks while streaming an added
  file's bytes, like `CheckpointStream`); `OperationIntent` for cancel. With temp+rename, cancel = abandon the temp,
  original intact (clean, no rollback ledger).
- **Conflict — net-new resolver sibling (corrected).** `conflict.rs` is hard-wired to the live FS (`fs::metadata(dest)`,
  an O_EXCL placeholder for `find_unique_name`), so it can't consult a zip index — exactly why MTP/SMB have a parallel
  `transfer/volume_conflict.rs`. In-archive conflicts need a THIRD resolver that reuses only the pure `ApplyToAll`
  latch + the oneshot prompt plumbing, consulting the archive index for "name exists".
- **`WriteOperationType` fan-out**: adding `ArchiveEdit` touches the closed enum (`types.rs`), the wildcard-less match
  in `analytics.rs` (compile error otherwise — the only exhaustive Rust match), `bindings.ts` (regen), and **two**
  hand-written FE string unions in `src/lib/file-explorer/types.ts` (`WriteOperationType` AND `TransferOperationType`,
  both `'copy'|'move'|'delete'|'trash'` — the second is easy to miss). List these in M4.
- **Progress/ETA**: every edit is an O(archive) `raw_copy_file` of all retained entries (deleting one file from a 10 GB
  zip copies ~10 GB), so progress is driven by **bytes raw-copied (retained) + bytes compressed (added)** — NOT modeled
  as instant. The two-axis estimator still applies (files done as entries land, bytes as they copy), but a "delete"
  shows a real progress bar, not a near-instant flash.
- Operation mapping: copy/move OUT = pure read (existing engine, needs `scan_for_copy`); copy/move IN = `{ add }`;
  delete/rename INSIDE = `{ delete | rename }`; **mkdir/mkfile INSIDE = `{ add }`** (an explicit directory entry /
  zero-byte file); move ACROSS = transfer + archive-edit compound on one lifecycle.
- **mkdir/mkfile/rename interception happens at the managed layer, not the command layer.** These three now run as
  managed instant ops (`manager::run_instant`, in `write_operations/create.rs` / `rename.rs`; the IPC commands in
  `commands/file_system/write_ops.rs` and `commands/rename.rs` are thin shims). An archive target is NOT instant — it's
  an O(archive) rewrite — so the managed fns fork on an archive target and route to `ArchiveEditOperation` via
  `spawn_managed` (real progress bar, lane admission) instead of the instant path.

### Piece 3 — zip mutation mechanism: temp+rename safe-overwrite (default)

**Default and only v1 strategy: build the edited archive to a `.cmdr-` temp, then atomic-rename over the original** —
the app's mandated safe-overwrite (AGENTS.md principle 4). Retained entries are copied with the `zip` crate's
`raw_copy_file` (verified: no decompress/recompress, preserves entries byte-for-byte); only newly added entries are
compressed. This is:

- **Cancel/crash-safe by construction**: the original is untouched until the final rename; cancel abandons the temp.
  This is what makes the cancellation David wants actually work (naive in-place append corrupts on cancel — verified).
- **Mostly-uniform across backends**: local edits the source in place — open it as a `zip::ZipArchive<File>` (cheap
  local `Read+Seek`), `raw_copy_file` retained entries into the temp, add new ones, rename. **Remote (M5) first pulls
  the old archive to a local temp**, because `zip`'s `raw_copy_file` consumes a `zip::ZipArchive<R: Read+Seek>` and
  cannot ingest arbitrary bytes — so the retained-entry copy must run over a local file; then `write_from_stream`
  uploads the result and swaps (same as MTP). So it's a shared local-build path with a remote pull-first prologue, not
  literally one path — the remote pull is the "even if slower" cost.
- **Honest cost**: O(archive) per edit (a raw byte copy, no recompress). Fast locally (~1 s/GB), slower remote — the
  "even if slower" David accepted. A near-instant delete is NOT achievable this way; see the optimization below.

**Deferred optimization — true append-past-EOF (M-later, needs a spike).** To get O(new file) adds / O(CD) deletes, you
must **hand-roll** the layout (seek to EOF, append entries + a fresh full central directory + new EOCD, leaving the old
CD/EOCD as dead bytes mid-file) — `ZipWriter::new_append` will not do this (it overwrites the old CD). Then verify macOS
Archive Utility, Finder Quick Look, `unzip`, and 7-Zip all accept the dead-bytes-in-middle layout, and add the
compaction story (dead-space slider/checkbox, `raw_copy_file` repack). This is a research spike with real reader-compat
risk, explicitly out of v1. The temp+rename default already gives correct, safe, uniform editing.

Building a fresh archive into a temp uses `ZipWriter` over `W: Write + Seek` (a plain `std::fs::File`) — the
`Read+Write+Seek` bound is only `new_append`'s, which we don't use. **Remote** temp+rename does NOT need a remote write
adapter: build the new archive in a **local** seekable temp, then `write_from_stream` uploads the finished bytes whole
(same path as MTP). The only place a remote random-access **write** adapter is needed is M-append's true in-place path.

Data-safety invariants temp+rename must inherit (the app already enforces these for file overwrites):

- **Temp is a same-directory sibling** of the `.zip` (`foo.zip.cmdr-tmp-<uuid>`), so the final rename is atomic on one
  filesystem — never an OS-temp-dir build + cross-device move. The startup `.cmdr-` reaper covers leftovers.
- **Preserve the original zip's own metadata**: a rewrite yields a fresh inode, so copy the original's mode, mtimes, and
  xattrs (macOS Finder tags, quarantine, creation date) onto the temp before the rename — else "edit in place" silently
  strips tags a plain copy would keep.
- **Move-across ordering**: a move OUT-of-or-INTO an archive deletes the source side only AFTER the destination side is
  durably committed (the app's existing move invariant), so a crash never loses both copies.

### Transparent path routing (corrected — path-aware resolver)

Add **`VolumeManager::resolve(volume_id, path) -> (Arc<dyn Volume>, inner_path)`**: if `path` crosses an archive
boundary (a path component is a real **file** that sniffs as a supported archive), register/lookup the `ArchiveVolume`
and return it + the inner relative path; else return the requested volume + path unchanged. Adopt it at every site that
currently does `VolumeManager::get(volume_id)` then `volume.method(path)` — ~18 commands: `path_exists`, listing,
`create_file`/`create_directory`, `delete_files_start`, `rename_file`, `copy_between_volumes`/`move_between_volumes`,
`scan_volume_for_copy`/`scan_volume_for_conflicts`, `start_drag_paths`, space polling, the watcher, the listing cache.
**Re-derive this site list at M1b start**: the commands layout shifted in the 2026-07 refactor wave (`commands/ui.rs`
split by concern; file-system commands moved under `commands/file_system/`; rename/mkdir/mkfile resolve their volume in
the managed layer now — intercept there, per Piece 2).

**No-`volume_id` commands need their own archive-aware patch** (these bypass VolumeManager entirely): `viewer_open` /
`viewer_open_as_text` / `viewer_write_range_to_file` (raw path — **previewing a file inside an archive is unrouted
today; required for M1 to be useful**), `stat_paths_kinds`, the local
`copy_files`/`move_files`/`delete_files`/`trash_files` fast-paths, the `rename_file` root branch, and
`go_to_path::resolve`.

**Layer into the existing resolve flow, don't add a fourth resolver.** `commands/volumes.rs::resolve_path_volume` /
`resolve_location` (backend) and `resolvePathVolume` (FE) already map paths→volumes; fold archive detection into them
rather than standing up a parallel concept, or the boundary logic forks.

`ArchiveVolume::root()` = the real `.zip` path, inner paths join under it, so `/path/to/foo.zip/inner` renders for free
(`splitPathSegments` splits on `/`). The git delegation hook in `LocalPosixVolume::list_directory` is the closest
existing analog for path-based routing.

## Decisions (resolved with David)

1. **RESOLVED — containing drive stays selected; the pane gets its own routing volume.** This means a new `'archive'`
   `VolumeKind` + a second identity field. Today the pane's single `volumeId` (on the active tab) is simultaneously
   routing id, display id, capabilities key, tint key, drop-policy key, and persistence key — read in ~10 places.
   Decoupling means: the new VolumeKind (so capabilities are read-only-correct, not the `local` default that advertises
   `canPasteInto`/`canRenameInPlace = true`), the second identity threaded through capabilities / view-selection gates /
   the drop-foreign-listing branch / tint / the selector pill / `getPaneVolumePath` / `getPaneLocation` / MCP sync /
   `swapDualPaneState`, AND a persistence fix: **persist the parent drive + zip path and re-derive the archive lazily**
   (else quitting inside an archive persists `archive-<hash>` + an unresolvable path → unreachable banner on restart).
   **The reader set is ~2x the "~10" sketch and must be enumerated and bucketed routing|display before building**
   (derive from the two getters `getPaneVolumeId`/`getFocusedPaneVolumeId`). Routing consumers (take the archive id):
   the backend `listDirectoryStart(volumeId,…)`, the git-repo subscription gate, `watchVolumeSpace`, the SMB reconnect
   manager, rename routing (`rename-flow.svelte.ts`), transfer/conflict-scan source+dest ids
   (`file-operation-commands.ts`), drag-drop dest, clipboard gating, `isDiskImageVolume`. Display consumers (take the
   parent drive): tint, breadcrumb, selector pill, persistence. Mis-bucketing one silently routes I/O to the wrong
   volume or mislabels the pane. **Restore re-derivation hook**: `resolveVolumeId` (`initialization.ts`) already ignores
   the stored `volumeId` and calls `resolvePathVolume(path)` with a trusted `'network'` special-case — add an
   archive-aware branch there (and make `resolvePathVolume` re-derive the archive from a `…/foo.zip/…` path), in
   lockstep with the backend resolver. **Refcount vs persist use two id forms, by design**: in-session history/panes
   carry `archive-<hash>` (drives the refcount/eviction; history isn't persisted), while persistence stores parent
   drive + zip path (drives restore). The mount hook increments; the search-results
   `droppedEntries`/`snapshotIdFromEntry` release is the eviction template.
2. **RESOLVED — bundles (`.app`/`.bundle`/`.framework`) default to Ask.** Nothing silently changes vs today's
   browse-into behavior; both options are offered.
3. **Mutation strategy — needs one explicit confirmation (reverses my earlier pitch).** My earlier "near-instant
   in-place like Total Commander" was wrong against the `zip` crate (verified: `new_append` overwrites the old central
   directory; cancel mid-edit corrupts the archive). So v1 mutation is **temp+rename safe-overwrite**: O(archive) per
   edit (raw byte copy, no recompress), safe, uniform across local/SMB/MTP, and genuinely cancellable. David's 20%
   compaction default is **recorded** and returns with the fast path below — temp+rename produces no dead space, so
   there's nothing to compact in v1. The fast **append-past-EOF** path (O(new file) adds / O(CD) deletes, plus
   compaction + the 20% slider) is a hand-rolled spike with reader-compat risk, scoped as a fast-follow milestone
   (M-append) behind the same `ArchiveMutator` interface so it slots in without rework. **RESOLVED**: ship safe
   temp+rename in v1 (M4); in-place append-speed (+ compaction + the 20% slider) as the fast-follow (M-append).

## Milestones

Sequential is fine. M1 is split per review (the backend half is headless-testable independent of the FE/routing half).

### M1a — `ArchiveVolume` backend (read + scan), local-backed, headless

- New `backends/archive/` module: `ArchiveVolume` (Tier 1 + read streaming + `scan_for_copy`), central-directory reader
  via **`rc-zip`** (sans-IO; `rc-zip-tokio`'s `HasCursor` = "AsyncRead at a given offset", no seek adapter) driven by
  the parent's ranged reads (local = `positioned-io` over `local_path()`+File), synthetic directory-tree builder,
  `(path,size,mtime)`-keyed index cache. rc-zip-tokio decompresses entries itself (deflate on by default), so no
  separate decompressor is needed for ZIP.
- Explicit capability flags (local_path None, space_poll None, export/streaming true, read-only semantics).
- Rabbit holes here: **Zip Slip** (sanitize `..`/absolute/symlink entry paths on extraction — data-safety hard
  requirement), **zip bombs** (browse reads only the index; extraction streams, no whole-entry buffer), **filename
  encoding** (zip CP437 vs the often-wrong UTF-8 flag; best-effort decode, `\`→`/`), **synthetic dirs** (no mtime/size →
  sensible `FileEntry`), **cancelable mount scan** (`list_directory_with_cancel`), **concurrency** (rc-zip reads use
  independent ranged-read cursors — no shared `&mut`; `max_concurrent_ops = 1` in M1; CPU-bound decompress off the async
  executor).
- Docs: `backends/archive/CLAUDE.md` + `DETAILS.md` (the C/D pair is enforced); `ArchiveVolume` column in the capability
  matrix; one map line in `docs/architecture.md`.
- Tests (TDD red→green for parsing/safety): CD parse against fixture zips; synthetic-tree (implied/nested/no-explicit
  dirs); **Zip Slip rejection**; encrypted/corrupt → typed error; `scan_for_copy` counts/bytes; extract streams chunks
  (no whole-buffer). Unit + integration against fixtures and `InMemoryVolume`. Checks: `pnpm check rust`.

Fully parallelizable with M1b until they meet at the resolver seam.

### M1b — Routing, navigation, path bar, viewer-into-archive

- **Path-aware `VolumeManager::resolve(volume_id, path)`** + adoption at the ~18 path-blind sites and the no-`volume_id`
  commands (incl. **`viewer_open*` — without this you can browse a zip but not preview a file in it**).
- `FileEntry.is_archive` computed backend-side in `listing/reading.rs`/`metadata.rs`, crossing IPC via `bindings:regen`
  so the FE fork is data-driven (don't flip `is_directory`). **At listing time use extension-only** (no per-file byte
  read — a magic sniff per entry would be a round-trip-per-file on SMB/MTP, violating principles 3/5); the magic-byte
  confirmation happens once at navigation/`resolve` time when the user actually enters.
- New `'archive'` `VolumeKind` + selected-volume decoupling (decision 1): pane display-volume vs routing-volume; new
  identity field threaded; capabilities read-only-correct; **persistence persists parent drive + zip path, re-derives
  lazily** (fixes restore).
- `handleNavigate` fork (`FilePane.svelte`): a file with `is_archive` routes to archive-open via the switch arm
  (`onGoToLocation` → `Location { volumeId: archiveId, path: innerPath }`) — the established "entry on another volume"
  pattern (mirrors search-results `goToRealEntry`). M1b enters directly (no Ask menu yet — M2).
- `navigateToParent` boundary: at the archive root, a custom branch bubbles
  `onGoToLocation(containingDrive, zipParentDir)` to exit (the root-equality guard otherwise blocks it).
- Path bar: `breadcrumbDisplayPath` and `breadcrumb-navigation.ts` need archive-aware branches (the routing volume's
  mount prefix is stripped today, so the path bar would show `/inner` not `…/foo.zip/inner`); the FE must know the zip's
  real containing path. The archive must register as an FE `VolumeInfo` with `path` = the parent mount so
  `getPaneVolumePath` resolves it (else it falls back to `'/'`, breaking prefix stripping and
  `enrichBreadcrumbSegments`'s base) — WITHOUT surfacing as a selector pill (search-results is the precedent for a
  path-resolvable, non-pill volume).
- Put new logic in `*.svelte.ts`/`*.ts` helpers (FilePane.svelte is ~3000 lines, file-length-flagged).
- **Friendly errors come from the raw `ArchiveError` at the resolve boundary**, not from `VolumeError`. The landed
  `ArchiveVolume` deliberately collapses the integrity family (not-a-zip / corrupt / encrypted / unsupported) to
  `NotSupported`/`IoError` as a mid-browse backstop (decision recorded in `backends/archive/DETAILS.md`); the
  user-facing "not a real archive" / "encrypted" copy is produced here, at navigation time, from the raw typed error.
- **Symlink-target safety (owed from the read milestone)**: `sanitize_entry_name` clamps entry NAMES, but a symlink
  entry's CONTENT is its target and streams verbatim. Decide extraction semantics for `is_symlink` entries (write the
  target as a regular file's content, or skip) and pin that extraction never CREATES a symlink from archive data — a
  symlink pointing outside the extraction root would be Zip Slip through the back door. Test it.
- Tests: boundary detection (path with `foo.zip` mid-string vs a real dir literally named `foo.zip` — real dir wins);
  navigate in/out; path-bar round-trip; **preview a file inside the zip**; copy a file out; **extract a symlink entry**
  (per the semantics above). E2E via Playwright + `dispatchMenuCommand`. i18n keys for any new strings (`i18n-coverage`
  is now an ERROR — untranslated keys fail the build). Checks: full `pnpm check`, `bindings:regen`.

### M2 — Enter behavior menu + per-format settings (Browse | Open | Ask)

- New `lib/ui/Menu.svelte` (Ark UI `Menu`; none exists — context menus are native/muda today) as the Enter popup:
  "Browse like a folder" / "Open with external app" / "Configure…", default-highlighting the configured action; keyboard
  mechanics modeled on `VolumeBreadcrumb.svelte`. Hook at `handleOpenOrParentKey` before the open: archive-or-bundle +
  policy Ask → menu; Browse → browse; Open → `openFile`/launch.
- Settings: Behavior › Archives section (custom layout — a list of formats, each a three-way `ToggleGroup`; store a
  pinned-shape JSON object, render via `lib/ui/ToggleGroup.svelte`; the `fileOperations.allowFileExtensionChanges`
  yes/no/ask setting is the wiring template). SectionCards for Archives and Bundles. "Configure…" deep-links via
  `openSettingsWindow(section)`.
- `.app` launch: "Open" for a bundle = LaunchServices launch (new — today bundles only browse); honors decision 2.
- Defaults: true archives → Ask; OOXML (`.docx`/`.xlsx`/`.jar`/`.apk`) → Open; bundles → Ask (resolved).
- Docs + tests (policy resolver unit test; E2E Enter→menu→Browse, Enter↓Enter→Open, Configure deep-link, Browse-skips-
  menu). i18n strings. Checks: full `pnpm check`, `bindings:regen` if a settings command is added.

### M3 — Live watching

- `ArchiveVolume::listing_is_watched(path) -> true` + a single-file content watch on the parent `.zip` (different shape
  from the dir-NonRecursive `start_watching` — register an own notify watch). On change:
  `notify_directory_changed(archive_volume_id, inner_path, DirectoryChange::FullRefresh)`; invalidate the index cache.
- Edge cases: editor temp+rename (inode swap) → re-mount; mid-write unreadable CD → keep the old listing until a clean
  re-read; debounce; **index-cache invalidation racing an in-flight edit** (the `(path,size,mtime)` key flips mid-edit;
  a concurrent browse in the other pane could read a half-written file — spell out the lane interaction since the read
  isn't on the edit's lane).
- Tests: modify a fixture zip → listing refreshes; truncated/mid-write keeps old listing. Checks: `pnpm check rust`.

Nice-to-have; can follow M4 if mutation is the priority.

### M4 — Zip mutation (add/delete/rename/mkdir/mkfile), temp+rename

- `ArchiveMutator`: applies a changeset by building the new archive to a `.cmdr-` temp (`raw_copy_file` for retained
  entries, compress only new ones), then atomic-rename. `ZipWriter` over `std::fs::File` (local).
- **mkdir/mkfile inside archives**: fork the managed instant-op path (`create_directory_managed` / `create_file_managed`
  / `rename_managed`) on an archive target → `{ add }` changeset (explicit directory entry via
  `ZipWriter::add_directory` / zero-byte file), through the same driver. Users see a real managed op with progress, not
  a fake-instant one.
- **Flip zip capabilities to writable here** (the `'archive'` VolumeKind ships read-only in M1b; M4 turns on
  `canPasteInto`/`canRenameInPlace`/mkdir/mkfile for zips; tar/7z stay read-only).
- `ArchiveEditOperation`: net-new ~85-line driver plugged into `manager::spawn_managed`; `PauseGate`/`OperationIntent`
  wiring; progress/ETA on bytes processed; `write-settled` guard; downloads-watcher ignore-set contract.
- Net-new in-archive conflict resolver sibling (reuse only the pure `ApplyToAll` latch + oneshot plumbing).
- `WriteOperationType::ArchiveEdit` + the fan-out (enum, `analytics.rs` match, `bindings:regen`, FE string unions).
- Operation planner: detect archive dest/source and BATCH per-file into one changeset; move-across = transfer +
  archive-edit compound.
- In-archive delete confirm copy: permanent, no Trash (David accepted).
- Tests (TDD red→green, data-safety critical): round-trip add/delete/rename → re-read; **cancel mid-add leaves the
  original fully readable** (temp+rename makes this true — the headline safety property); merge invariant (an edit never
  drops an untouched sibling); two zips on **different mounts** edit in parallel while **same-mount (incl. same-zip)**
  serialize (existing per-device write-serialization, parent lane); pause/resume an add; in-archive name conflict
  prompt; mkdir/mkfile round-trip (dir entry and zero-byte file survive re-read by `unzip`/Archive Utility). Re-run
  data-safety tests yourself before merge; `cargo mutants` on new write-side files. Checks: full
  `pnpm check --include-slow`, `bindings:regen`.

(Compaction checkbox + dead-space slider deferred to M-append — temp+rename produces no dead space.)

### M-append — fast in-place editing (fast-follow to M4, research spike)

The speed David wants (O(new file) adds, O(CD) deletes), behind the same `ArchiveMutator` interface so M4 needs no
rework. NOT via `ZipWriter::new_append` (it overwrites the old CD). Hand-roll the layout: seek to EOF, append new
entries + a fresh full central directory + a new EOCD, leaving the old CD/EOCD as dead bytes mid-file. This is
cancel-safe by the same reasoning the original plan claimed — but only because it's hand-rolled (the old EOCD survives
until the new one lands), which is exactly what the crate does NOT do for you.

- **Reader-compat spike first** (gate the whole milestone on it): verify macOS Archive Utility, Finder Quick Look,
  `unzip`, and 7-Zip all accept the dead-bytes-in-middle layout. If any reject it, this path is dead and temp+rename
  stays the only strategy.
- Dead-space accounting + **compaction** (a `raw_copy_file` repack dropping dead bytes), run on the checkbox or past the
  threshold; itself an `ArchiveEditOperation`-shaped op.
- Frontend: compaction checkbox on `TransferDialog.svelte` (thread through `onConfirm` → `handleTransferConfirm` →
  `transfer-progress-state.svelte.ts` → a new `WriteOperationConfig` field → `bindings:regen`); Behavior › File ops
  `SettingSlider` (min 0, max 100, step 5, unit `%`, **default 20%** per David), description spelling out the trade-off
  (space reclaimed vs repack time); 0% = always compact, 100% = never.
- Needs the random-access-write `Volume` capability (local `std::fs::File` seek+write; SMB offset writes; MTP via M6 or
  whole-object fallback).
- Tests: reader-compat matrix (the gate); cancel mid-append leaves the original readable (old EOCD intact); compaction
  reclaims space and preserves entries byte-for-byte; dead-space accounting. Checks: full `pnpm check --include-slow`.

### M5 — Remote-backed archives (SMB + MTP)

- Add a parent ranged-read primitive to `LocalPosixVolume`/`SmbVolume` (MTP has it) and feed `rc-zip`'s sans-IO reader
  from it — no sync `Read+Seek` adapter needed (that's the whole point of choosing rc-zip). Browse/extract become
  uniform with local.
- **Remote writes need NO remote write adapter, but DO pull the old archive local first.** `zip`'s `raw_copy_file`
  requires a `zip::ZipArchive<Read+Seek>` source, which it can't get over async ranged reads — so download the old
  archive to a local temp, build the edited archive locally (`raw_copy_file` retained + compress added), then
  `write_from_stream` uploads it whole and swaps (same as MTP). The remote pull is the cost; no random-access write
  adapter is involved (that's only M-append's true in-place path).
- Register `ArchiveVolume` for non-local parents; aggressively cache the parsed central directory (one tail ranged read;
  a second only if the CD exceeds the first chunk); stream an entry's compressed range in one request for extract.
- Tests: Docker SMB fixtures (browse + extract + a small edit); MTP via the virtual-device harness. Docs: SMB
  offset-write vs MTP whole-object asymmetry. Checks: `pnpm check --include-slow`.

### M6 — MTP in-place editing (stretch)

Add the four Android edit ops to `mtp-rs` (`BeginEditObject` 0x95C4, `SendPartialObject` 0x95C2, `TruncateObject`
0x95C3, `EndEditObject` 0x95C5 — low-risk: flat opcode enum + public `execute*` primitives, prototype via
`OperationCode::Unknown(0x95Cx)` then promote to ~3 files + a `can_edit_in_place` `Capabilities` flag). Probe per device
via `DeviceInfo::supports_operation`; fall back to whole-object rewrite when unsupported (device-honesty is the real
risk). `mtp-rs` is first-party (its release policy applies). Only relevant if the append optimization lands (in-place
editing on the device); otherwise the temp+rename whole-object path already works on MTP from M5.

### M7 — tar + 7z read-only browsing

- `tar` crate + pure-Rust decompressors (`flate2` `rust_backend`, `lzma-rs`, `bzip2-rs`, `ruzstd`) and `sevenz-rust2`
  (actively-maintained pure-Rust 7z, decode).
- **Sequential trap**: no random access — build a path→offset index with one cancelable mount scan; for solid formats
  extract sequentially in one pass when copying the whole archive (don't issue random per-entry reads). Declare the
  random-access-vs-sequential class so the copy planner avoids O(n²).
- Mutation paths return `NotSupported` cleanly. Tests: browse + extract per format; the sequential strategy. Checks:
  `cargo deny check` per new crate (license + 3-day age gate; verify latest on crates.io). Full `pnpm check`.

## Dependencies (verify latest + license at add-time)

Landed with the read core: `rc-zip` 5.4.1 (deflate/bzip2/lzma/zstd features) + `positioned-io` 0.3.5. **`rc-zip-tokio`
was evaluated and dropped** — its only public entry reader borrows its `ArchiveHandle` (can't back an owned, cached
stream) and it decompresses on the async executor; we drive `rc-zip`'s sans-IO fsms directly over our own
`ArchiveByteSource` trait instead (see `backends/archive/DETAILS.md`). References to `rc-zip-tokio` elsewhere in this
plan read as "the rc-zip read core".

`cargo deny check` every crate; verify ≥3 days old on crates.io; don't trust training data; Renovate handles updates
after. Shortlist: **`rc-zip` + `rc-zip-tokio`** (sans-IO zip READ — browse/extract, local + remote; enable the `deflate`
default + any codec feature in-scope archives use — `bzip2`/`lzma`/`zstd` — or extract errors) + **`positioned-io`**
(ranged-read cursor over the local File), **`zip`** (zip WRITE — temp+rename via `raw_copy_file`/`ZipWriter`; pure-Rust
codec backends — `miniz_oxide`, not zlib-ng), `tar`, `flate2` (`rust_backend`), `lzma-rs`, `bzip2-rs`, `ruzstd`,
`sevenz-rust2`. M6 touches `mtp-rs` (first-party).

**Evaluated and rejected: `async_zip` (Majored/rs-async-zip) as a single read+write replacement.** Read its source
(v0.0.18). It's pure-Rust for Stored+Deflate (via `async-compression`→`flate2`→`miniz_oxide`; bzip2/lzma/zstd/xz pull C,
same as everyone), does async read+write in one crate, and has no-recompress _writes_ (`write_entry_*_precompressed`).
But: (1) its reader has NO raw-compressed-bytes API (`CompressedReader` always decodes per the entry's method), so the
one thing that could justify it — streaming a remote raw-entry copy without a full local pull — would still be
hand-rolled; (2) it's pre-1.0 single-maintainer, and we won't bet the data-safety-critical write path on that when `zip`
is mature; (3) for reads, `rc-zip`'s sans-IO (read-at-offset) fits `Volume` ranged reads better than async_zip's
`AsyncRead + AsyncSeek`. **Revisit only if it reaches 1.0 with a raw-entry-read API** (then it could unify read+write
and enable streaming remote edits).

## Cross-cutting risks (carry through every milestone)

- **Lifecycle/eviction**: refcount ArchiveVolumes by panes/tabs/history entries; unregister on last release (mirror the
  search-results snapshot release); LRU backstop. Without it, browsing many zips leaks unboundedly.
- **Read-only space semantics**: `get_space_info` must not read as "disk full"/block paste;
  `space_poll_interval = None`.
- **Restore-from-persistence**: persist parent drive + zip path, re-derive the archive lazily (else restart →
  unreachable).
- **Concurrency vs the crates**: `rc-zip` reads are sans-IO (concurrent reads via independent ranged-read cursors, no
  shared `&mut`); the `zip` `ZipWriter` (writes) is `&mut`, one handle per edit; decompression runs on a CPU pool /
  `spawn_blocking`, off the async executor.
- **Teardown + temp reaping**: unmount drops handles + temp cache; a startup reaper for orphaned `.cmdr-` archive temps.
- **i18n-coverage is an error** (commit `570f4ed1`): every new settings/menu/error/confirm string needs a catalog key or
  the build fails.
- **MCP automation** inherits FE routing (so fixing the FE covers it) — except the FE "open file" action routes to
  `viewer_open`, so it inherits that gap until M1b patches the viewer.
- **Docs**: don't transcribe what codegraph owns (symbol locations); spend doc tokens on the why and the wiring.

## Testing strategy summary

Lean TDD (real red→green) for data-safety/parsing-critical logic: CD parse, synthetic tree, Zip Slip, boundary
detection, every mutation round-trip + cancel-leaves-original-intact. Unit-test backends against real fixture archives +
`InMemoryVolume`; integration-test resolve/browse/extract/edit end to end; E2E the user-visible flows (Enter menu,
navigate in/out, preview-in-archive, copy in/out, delete/rename inside, cancel). Re-run data-safety tests yourself
before any FF-merge; `cargo mutants` on new write-side files. Read `docs/testing.md` + `docs/tooling/testing.md` first.
