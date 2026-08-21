# Network-volume enrichment — details

The depth behind `CLAUDE.md`. Read this before any non-trivial work here: editing, planning, reorganizing, or advising.
Subsystem-wide context lives in `media_index/DETAILS.md`; the scheduler that routes SMB volumes here is
`../scheduler/DETAILS.md` § The lifecycle bus.

Making an opted-in NAS's images searchable by content is the headline use case (`/Volumes/naspi` over SMB). `importance`
follows a hard rule ("never a filesystem syscall against an SMB/MTP mount"), but media enrichment MUST read image bytes
off the wire, so everything network lives here. Scoped to the existing Vision backend — no new models.

## The byte-fetch decision (`fetch.rs`) — the app's own session first, OS mount as fallback

**Decision (plan M1): read image bytes through the `Volume` trait when the app holds its own transport session — a
Direct-smb2 `SmbVolume` — and fall back to the OS mount path (`/Volumes/<share>/…` via `std::fs`) only for mount-only
volumes.** Two fetchers behind the one `ByteFetcher` seam, picked per pass by `Volume::supports_local_fs_access()` (the
same local-vs-remote predicate the archive backend uses for its byte source). Why the direct session, not the mount:

- **macOS TCC ("network volumes") owns the mount.** `std::fs` on `/Volumes/…` gets `EPERM` for unsigned dev binaries
  (rebuilds shed grants — reproduced twice, 2026-07-16, the pass stalled at zero images) and triggers a permission
  prompt in prod. The direct smb2 session is the connection Cmdr already owns, health-checks, and auto-reconnects; TCC
  has no say over it.
- **Typed errors.** The direct path fails with `VolumeError` variants (`DeviceDisconnected`, `NotFound`, …), so
  pause-vs-skip classification is exact instead of errno inference on the mount.
- **`VolumeByteFetcher`** drains `Volume::open_read_stream_for_scan` (SMB serves small hinted files via the 1-RTT
  compound read, from the scan-connection pool when one is up — § Network parallelism) and bridges async→sync with a
  captured runtime handle + `block_on`, sound because enrichment fetch runs on `spawn_blocking`/plain worker threads,
  never a runtime worker (the archive backend's `VolumeByteSource` bridge). The whole read sits under
  `tokio::time::timeout` ⇒ a hung transport is a `Disconnected` pause, never a wedge.
- The mount root still comes from `host::volumes::current().get(volume_id).root()` — the same source
  `indexing::paths::routing::index_read_path` uses for its read-side mount strip — and `Volume` impls accept
  mount-absolute display paths, so the os-joined path feeds both fetchers unchanged.

**Per-file errors never pause the pass (the second M1 defect).** Only a TYPED transport loss pauses
(`VolumeError::DeviceDisconnected`/`ConnectionTimeout` on the direct path; a transport-loss errno set or the read
timeout on the mount path — `classify_io_error`). Everything else per-file (permission denied, `EIO`, `EISDIR`) is
`FetchError::Unreadable`: skip it, count it (`PassSummary.skipped_unreadable`), log "N skipped: unreadable" at pass end,
write NO row (`Failed` stays reserved for a good read with a bad decode). Bias documented in `classify_io_error`: a
misread dead mount completes honestly and re-enriches next scan; a misread per-file fault would pause the pass against a
condition that never clears — exactly the TCC-EPERM stall this fixes. Without this line, an all-EPERM mount would either
stall forever or silently "complete"; the skip count keeps it loud.

**Path mapping.** An SMB index's `ROOT_ID` is the mount root, so `walk_image_entries` reconstructs MOUNT-RELATIVE paths
(`/DCIM/x.jpg`). `os_join(mount_root, rel)` prepends the mount root to reach the real file
(`/Volumes/naspi/DCIM/x.jpg`); for the `root`/local volume the mount root is `/`, so the path passes through unchanged.
The stored `media.db` row keeps the index-relative identity (matching the index + GC set); the network-enrichment UI
reconstructs the display/open path via the mount root.

**Non-blocking discipline (the crux).** A network read can block indefinitely on a hung transport. `FsByteFetcher` runs
its `std::fs` read on a throwaway thread and waits with `recv_timeout`; `VolumeByteFetcher` bounds the whole async read
with `tokio::time::timeout`. Either timeout returns `FetchError::Disconnected` (pause), never a wedge. Critically, the
fetch happens in the ENRICH layer, not on the serialized Vision OCR worker thread — the backend receives the
already-fetched bytes via `ImageInput.bytes` (`Some` = network, `None` = local read-it-yourself), so a hung transport
can never stall OCR of other (local) volumes. A `MAX_FETCH_BYTES` cap skips a pathological file rather than OOMing (the
direct fetcher also short-circuits on an over-cap size hint, without touching the wire).

## The conservative-fetch policy with teeth (`policy.rs`)

Typed knobs (`ConservativeFetchPolicy`), each a real gate, not a comment:

- **Priority-gated.** The pass proceeds only while the volume is CLEAR of higher-priority work
  (`volume_clear_for_enrichment`, pure and tested — the host's order: interactive > transfers > indexing): the app has
  been foreground-idle for `idle_threshold` (default 5 s) AND no user-initiated transfer is touching this volume
  (`priority::transfers`). `priority::foreground` holds the process-global "last foreground activity" timestamps,
  stamped by the hot foreground filesystem IPC (directory listing = every navigation); the pure
  `is_idle(now, last, threshold)` is unit-tested over a fake clock. Enrichment reads the **app-wide** foreground scope,
  not the per-volume one the index scan and SMB transfers use: this is heavy on-device ML with no deadline, so
  foreground work anywhere is reason enough to wait — while the transfer check is per-volume (a copy elsewhere is no
  reason to wait). A busy volume pauses the pass (`PauseReason::NotIdle`) so a NAS is never dragged over the wire while
  the user browses or a copy runs. A `NotIdle` pause is TRANSIENT, not terminal: `run_network_pass_blocking` returns
  `PassOutcome::RetryWhenIdle`, and `spawn_pass` keeps the volume's coordinator slot and re-runs the pass (from the
  store, skipping done rows) once the volume is clear again (`wait_until_idle_to_resume(volume_id)`, polling every 2 s
  over the SAME composed condition, ending on clear OR `gate::should_stop`). Without this resume the enrichment would
  stall permanently after the first pause — a NAS that the user keeps browsing near would freeze mid-sweep and never
  finish. The `should_retry_when_idle` gate is `NotIdle` ONLY: `Disconnected` resumes via the registration bus on
  remount, `Cancelled` via the next scan or user kick, so looping on either would spin the idle-wait against a condition
  this loop can't clear.
- **Bandwidth-bounded.** After each image, `throttle_delay(bytes, max_bytes_per_sec)` sleeps so the sustained fetch rate
  stays under the cap (default 8 MB/s). Pure and tested; it deliberately over-throttles slightly (ignores OCR time) —
  the conservative direction. (The parallel pass paces at dispatch on the index's last-known size, since the actual
  count lands on a fetch worker; a stale size self-corrects over the pass.)
- **Bounded concurrency.** `max_concurrency` (default 3) is the PARALLEL pass's prefetch fan-out width (§ Network
  parallelism); the sequential pass is inherently 1.
- **Resumable.** Each completed image persists immediately (path-keyed upsert), so an interrupted pass resumes from the
  store on the next scan; unchanged images skip via `needs_enrichment`.

## The "always index" override (`config.rs`) — why it's load-bearing

Navigation-based importance scores a rarely-browsed NAS archive LOW everywhere, so importance-first ordering would defer
the user's photos forever (plan Decision 6). The override forces enrichment regardless of importance.
`should_enrich_image(covered_by_override, importance, threshold)` = `covered || importance ≥ threshold`. The importance
slider is present, but for network volumes the production importance oracle yields `None` and **the override is the
load-bearing input**: only override-covered volumes/folders enrich. The gate seam keeps the importance path drop-in.

**Storage: a settings-seeded global, not a fourth per-volume store.** The opt-in and overrides are user config (a
handful of volumes/folders), not per-image data, so they ride the sparse settings store (`mediaIndex.networkVolumes`,
`mediaIndex.alwaysIndexVolumes`, `mediaIndex.alwaysIndexFolders` — FE-owned) rather than a new SQLite DB with its own
writer thread (the standing-cost note already flags per-volume thread growth). The scheduler runs off the IPC thread and
consults `network::config` (a process-global `RwLock`) each pass, seeded from `load_settings` at startup and
live-applied through the `media_index_set_*` commands. Folder overrides store absolute OS-mount paths; `path_is_within`
is a trailing-slash-safe prefix so `/Photos2` isn't "within" `/Photos`.

**Two coverage questions, one prefix test.** `covers(volume_id, os_path)` answers it for a FILE.
`may_cover_within(volume_id, dir)` answers it for a DIRECTORY, for a caller deciding whether a directory is worth
walking at all (the media live tick's filter — `scheduler/DETAILS.md` § The coverage filter). It is deliberately a
SUPERSET: on top of `covers(dir)` it also keeps any directory an override entry names something at or under. Overrides
are folders in practice, but `covers` is a plain prefix test, so an entry that happened to name a FILE would cover that
file while covering neither its parent nor anything else — and the extra term is exactly what stops a directory filter
from dropping that file's parent. It costs one more pass over the same small set.

`config` also holds `excluded_folders`, the privacy veto — the ONE live-read part of the config (the retro-delete that
rides it is in `../DETAILS.md` § Per-folder photo-search exclude).

_Non-load-bearing candidate (NOT built):_ a photo-density importance input (a folder that's mostly images is likely an
archive regardless of visit count) could feed the importance oracle. Deliberately deferred; the manual override is the
current mechanism.

## Resumability across unmount + the disconnect data-safety lines

A mid-pass unmount is not a crash and not a bad file. On `FetchError::Disconnected` the pass returns
`NetworkPassOutcome::Paused { reason: Disconnected }`: it flushes every completed row (they survive), writes NO `Failed`
row for the in-flight image, and does NOT GC. The scheduler marks the volume paused (`network::config::mark_paused`),
which the coverage signal surfaces; resume happens via the registration bus on remount (the next completed scan re-runs
the pass, skipping already-Done rows). This is distinct from the `Failed` state, which is reserved for a genuinely bad
file (a GOOD read but a decode/OCR failure) — a transport fault must never masquerade as one.

**GC vs a mere disconnect.** Only a pass that ran to COMPLETION reaches GC (the same completed-scan edge the local pass
gates on). A paused/cancelled pass returns before GC, so a disconnect can NEVER wipe a volume's coverage — a paused
volume's rows survive intact until reconnect.

## Offline search after unmount (Decision 8)

`media.db` is keyed by `volume_id` (`media-{volume_id}.db`) and the `MediaIndex` read API opens it directly, so an SMB
volume's photos stay searchable with the NAS unplugged.

## MTP stays on-demand, never background

`wire_volume` skips MTP with a log: a phone/camera on MTP is transient and slow, so enrichment is on-demand-per-visit,
not a background sweep (keeping `importance/`'s `ScoringPolicy::for_kind` MTP exclusion). The never-background-sweep
gate is real; the on-demand-per-visit trigger itself is a later slice (a clear TODO — nothing wires it yet).

## Network parallelism + the byte-bounded prefetch (`enrich.rs`, `budget.rs`)

The parallel network pass is a three-stage pipeline over the same worker pool the local pass uses
(`../scheduler/DETAILS.md` § Parallel enrichment). ONE dispatcher thread keeps every conservative fetch-side DECISION
(priority gate, coverage gates, byte-budget admission, bandwidth pacing, progress); K fetch workers (`max_concurrency`,
plan M1) perform the byte-reads in parallel — over SMB they spread across the scan-session connection pool (§ The
byte-fetch decision; the pass brackets `begin`/`end_scan_session` when direct AND parallel, refcounted on `SmbVolume` so
an overlapping index rescan shares the pool), which is what lets the reads genuinely overlap (ksmbd serializes per
connection); N compute workers (each its own backend) analyze and write.

Prefetch admission is bounded by BYTES, not file count (`ByteBudget`): the dispatcher acquires an image's size before
handing it to a fetch worker and a compute worker releases it after the decode, so the whole fan-out can't blow the
memory ceiling on a RAW-heavy corpus (256 MB/file cap × ~36 MB/decode would otherwise let a count-based queue buffer
gigabytes). An over-cap file is admitted alone (never deadlocks); a stop wakes a blocked acquire. The data-safety lines
hold: a typed disconnect on ANY fetch worker stops the dispatcher, queued jobs drain-release their reservations, compute
workers drain the already-fetched jobs, and NO GC runs (§ Resumability); the disconnect wins the pause-reason merge (a
`NotIdle` retry would only re-hit the dead transport).

**Expected NAS-side effect — still unmeasured.** M1's direct-session read path removed the dev-mount `EPERM` blocker,
but a real-NAS throughput number hasn't been produced yet (M1 validated correctness on the Docker SMB fixtures; a
bounded re-enrich of the ~9k-image NAS corpus is the intended measurement, deliberately not run as part of the M1
worktree). Reasoned expectation from the M2 spike + the design: prefetch fan-out hides per-file SMB latency behind
neighboring reads and behind compute, so the pass should approach the local ~1.25x-at-N=2 ANE ceiling instead of adding
wire latency on top. To measure: opt the NAS in, set `mediaIndex.parallelism` to 2, run a bounded re-enrich over the
existing corpus (bump a folder's mtimes or clear its rows), and read images/min off the `media-enrich-progress` log
against the 2026-07-16 ~60–80 img/min baseline. The byte budget is what makes the overlap SAFE (bounded buffer), never a
multiplier.

## Backend commands + typed state for the network-enrichment UI

The backend provides three setters + the extended state. They live in
`apps/desktop/src-tauri/src/commands/media_index/policy.rs` with the other coverage-changing commands (the scope, the
threshold, the privacy exclusion), split from the read/query modules beside them in `../commands/`: each mutates live
`gate` / `network::config` state and has to decide whether the change BROADENS coverage and needs an immediate pass, and
each of those decisions is a pure `*_should_kick` fn tested in
`apps/desktop/src-tauri/src/commands/media_index/tests.rs`.

- `media_index_set_network_volume_enabled(volume_id, enabled)` — the per-volume SMB opt-in (live-applied; enabling kicks
  a pass).
- `media_index_set_always_index_volume(volume_id, always)` / `media_index_set_always_index_folder(folder, always)` — the
  overrides (live-applied). ADDING kicks a pass, removing doesn't: the volume setter kicks that volume's network pass
  when it's opted in, the folder setter kicks every ready volume (a path doesn't say which volume it's on).
- `media_index_volume_state` extended with `network_opt_in`, `always_indexed`, `paused` (the "paused, resumes on
  reconnect" honesty).

**The FE surface (shipped).** The opt-in + volume override live in Settings > AI > Image search > "Image search" card,
below the master toggle, rendered by `src/lib/settings/sections/MediaIndexNetworkVolumes.svelte` (only when
`mediaIndex.enabled` is on). It lists each mounted network (SMB) volume with an opt-in switch and, once opted in, an
"always index this drive" switch plus a live status line (indexing / paused-because-disconnected / count) polled off
`media_index_volume_state`. Persistence + live-apply are co-located in `src/lib/media-index/network-volume-prefs.ts`:
each toggle writes the FE-owned array setting (`mediaIndex.networkVolumes` / `mediaIndex.alwaysIndexVolumes`, persisted
as a REAL JSON array so the Rust loader's `Vec<String>` reads it — NOT the double-encoded JSON-string shape
`indexing.silencedDrives` uses) AND calls the matching setter, rolling the persisted value back if the IPC call rejects.
These three settings needed a new `'string-array'` `SettingType` (the store was scalar-only before). Cross-window edits
re-seed the switches via `onSpecificSettingChange`; startup seeding is the Rust `load_settings` path, so no
`settings-applier.ts` entry (the per-item setters don't fit its key→value passthrough table).

Coverage-honesty for a network volume lives in `src/lib/search/ImageSearchResults.svelte`: it takes an optional
`mountRoot` + `isNetwork` and reconstructs an openable OS path from each index-relative hit via the pure
`src/lib/search/media-path.ts` (`resolveMediaHitPath`, mirroring the backend `os_join`), and voices the network states
("turn on indexing for this drive" when not opted in, "disconnected, showing what's indexed" when paused). The Search
dialog reaches these states by following the FOCUSED PANE's volume: `+page.svelte` passes
`getFocusedPaneImageSearchVolume()` (the pure `resolveImageSearchVolume` over the volume store) as `imageSearchVolume`,
so browsing a NAS pane searches that NAS's `media.db` and hits resolve under its mount root; a non-filesystem pane (a
search-results snapshot) falls back to the local root. Filename search stays deliberately root-scoped (it reads the
local whole-drive index) — only the image grid follows the pane.

**Per-folder override — the chosen folders.** `media_index_set_always_index_folder` + `mediaIndex.alwaysIndexFolders`
back the chosen-folders list in Settings > AI > Image search (`MediaIndexChosenFolders.svelte`, adding via the native
folder picker). In the `ChosenFolders` scope these folders ARE the coverage (`../DETAILS.md` § The indexing scope). A
folder's context menu drives the SAME setter through the same FE helper (`always-index-folders.ts`), so a folder added
by right-click shows up in the Settings list and vice versa; the menu's label/enabled decision lives in
`menu/media_index_items.rs` (§ Image-search group in `menu/DETAILS.md`).

## Testing

`tests.rs` covers, over the fake fetcher: `gc_does_not_fire_on_a_disconnect` and
`disconnect_mid_pass_keeps_completed_rows_and_writes_no_failure` (both real red→green — the disconnect data-safety
lines), `search_answers_offline_after_the_volume_unmounts` (enrich, drop the writer to simulate unmount, assert search
still answers), and the live privacy veto in the network core (exclusion beats an override-covered image; an exclusion
landing mid-`analyze` persists NO row). The pure policy pieces (`volume_clear_for_enrichment`, `is_idle`,
`throttle_delay`, `should_enrich_image`, `path_is_within`) are unit-tested in place, as is the OS-folder → index-prefix
mapping in `fetch.rs`.
