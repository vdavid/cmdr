# Media index subsystem — details

Image-ML enrichment: makes a volume's images searchable by their content. Full design and milestone plan:
`docs/specs/media-ml-index-plan.md`. This doc covers what the OCR slice shipped, the port-from-`importance/` rationale,
the GC safety argument, and the schema.

The OCR slice ships the plumbing + OCR-text search with **no model download and no vector math**: a per-volume
disposable `media.db`, a lifecycle-bus-driven scheduler, an OCR pipeline behind a fakeable `VisionBackend` seam,
deletion-driven GC, and the `MediaIndex` read API. It's **local-only** (SMB/MTP comes with network enrichment) and **off by default**.

## Why a port of `importance/`, not a re-derivation

`importance/` already solved this plan's hardest plumbing (verified against the shipped code): a per-volume disposable
store carrying the index's cache discipline, a scheduler driven by the neutral lifecycle bus plus a startup registry
sweep plus a coalescing coordinator, and an offline-capable consumer read API. `media_index` copies those patterns
file-for-file in spirit (`store/`, `writer.rs`, `writer_registry.rs`, `scheduler/`'s `PassCoordinator`, `read/`), so a
maintainer who knows `importance/` already knows this. Read `importance/DETAILS.md` for the shared rationale (one writer
thread per DB, `platform_case` on every connection, delete-and-recreate on a schema bump, path-keyed rows, subscribe →
sweep → wire ordering, edge-triggered bus consumption).

### Two deliberate divergences from `importance/`

- **No per-row scan-generation column.** `importance` stamps every row with its as-of `recompute_generation` (its OWN
  persisted meta counter) because a full pass replaces the whole table. `media_index` doesn't rewrite the table each
  scan; its staleness is `(path, mtime, size)` + the OS/Vision engine stamp, which makes a generation column redundant.
  Crucially, this is NOT the lifecycle-bus `generation` — that one is a transient in-memory wake counter that resets to 1
  every launch and must NEVER be persisted (plan Decision 3). If a durable "as-of" marker is ever needed, mint a separate
  persisted counter à la `importance::next_generation`; never stamp the bus value.
- **A real GC instead of wholesale table replacement.** Media enrichment is expensive and incremental, so a pass enriches
  only stale images and GCs vanished rows, rather than clearing + rewriting the whole table.

## Scheduler file layout

`scheduler/` splits the coalesced-pass machinery by responsibility: `mod.rs` holds the `MediaScheduler` struct + its
pass bodies (`run_pass_blocking`, `run_network_pass_blocking`, `folder_scores`, `retro_delete_excluded_folder`);
`coordinator.rs` holds the pure, testable `PassCoordinator` (one pass per volume, coalesced re-run — covered by
`coalescing_tests`); `lifecycle.rs` holds the free-function scheduling/wiring layer (`start`, `kick_all_ready_passes`,
`kick_network_pass`, `wire_volume`, `spawn_pass`, `local_should_enrich`, `PassKind`); `live.rs` holds the live-follow
tick. The `kick_*`/`start` entry points re-export through `mod.rs` so their public paths stay `scheduler::start` etc.

## The lifecycle bus

`media_index`'s scheduler subscribes to `indexing/lifecycle/lifecycle_bus.rs` exactly as `importance`'s does — its OWN `start()`
mirrors the ordering (subscribe to registrations → sweep `ready_volumes_with_kind()` → wire per-volume subscriptions).
It can't piggyback `importance`'s subscription; because `app.manage` is keyed by type, an `Arc<MediaScheduler>` coexists
fine alongside `importance`'s scheduler. The bus mechanism (watch vs broadcast, late-subscriber replay, the registration
bus, why the sender outlives the registry) is documented once in `../indexing/DETAILS.md` — not re-documented here
(single-source).

`wire_volume` routes by typed kind: LOCAL enriches by default (when the master toggle is on); an opted-in SMB volume
runs the conservative network pass (§ "Network-volume enrichment"); MTP is NEVER background-swept. Both local and
SMB subscribe to the SAME bus the same way; only which pass method runs differs. The opt-in is checked INSIDE the network
pass, so flipping it on takes effect on the next scan completion (and the opt-in command kicks an immediate pass).

## The GC safety argument (data-safety)

GC deletes a stored `media.db` row when its source path no longer appears as a qualifying image in the CURRENT index
walk. The safety comes entirely from **when** a pass (and thus its GC) runs, not from generation arithmetic:

- A pass runs ONLY on a `Completed` bus edge or the Fresh registry sweep. The `Completed` signal fires AFTER the index
  writer flushes the truncate + repopulate (`indexing/lifecycle/scan_completion.rs`), so a triggered sweep always observes a
  COMPLETE tree — never the mid-scan truncate window where every path transiently vanishes.
- The edge is consumed via `borrow_and_update` / `has_changed`, NEVER a `borrow()` poll. The `watch` retains the last
  `Completed` across a new scan's truncate window; a poll could observe that stale `Completed` mid-truncate and GC live
  rows. Edge-triggered consumption fires exactly once per real completion, so a truncate window can't re-trigger a sweep.
  `enrich_tests::gc_fires_on_a_completed_edge_never_a_retained_poll` pins this.
- The startup sweep is safe for the same reason: `ready_volumes_with_kind()` filters to `Fresh` only (excludes
  `Scanning`/`Stale`), so a mid-scan volume is never swept at launch.
- `gc_over_an_empty_index_would_delete_everything_which_is_why_it_gates_on_completed` pins the hazard the edge-gate
  defends against: `gc_targets` over an empty walk would target every row — which is exactly why GC must never run
  mid-truncate.
- A cancelled pass (memory watchdog) skips GC entirely, yielding fully; vanished rows are collected on the next completed
  scan.

Three deletion paths bypass this completed-scan edge, each for a reason the edge doesn't cover: the privacy retro-delete
and the reclaim prune (both USER-EXPLICIT, settings-derived — § Per-folder photo-search exclude, § Reclaim space), and
the live-tick scoped GC (INDEX-CONFIRMED, scoped to the touched dirs — § Live enrichment). None may run the whole-store
`gc_targets` outside a completed pass.

## Network-volume enrichment

Making an opted-in NAS's images searchable by content is the headline use case (`/Volumes/naspi` over SMB). This is the
ONE part of the plan with no `importance/` sibling to copy: `importance` follows a hard rule ("never a filesystem syscall
against an SMB/MTP mount"), but media enrichment MUST read image bytes off the wire. Everything lives under `network/`;
the scheduler routes SMB volumes to it (§ "The lifecycle bus"). Scoped to OCR (it inherits the OCR slice's Vision backend — no new
models).

### The byte-fetch decision (`network/fetch.rs`) — the app's own session first, OS mount as fallback

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
  compound read, from the scan-connection pool when one is up — § Parallel enrichment) and bridges async→sync with a
  captured runtime handle + `block_on`, sound because enrichment fetch runs on `spawn_blocking`/plain worker threads,
  never a runtime worker (the archive backend's `VolumeByteSource` bridge). The whole read sits under
  `tokio::time::timeout` ⇒ a hung transport is a `Disconnected` pause, never a wedge.
- The mount root still comes from `VolumeManager::get(volume_id).root()` — the same source
  `indexing::paths::routing::index_read_path` uses for its read-side mount strip — and `Volume` impls accept
  mount-absolute display paths, so the os-joined path feeds both fetchers unchanged.

**Per-file errors never pause the pass (the second M1 defect).** Only a TYPED transport loss pauses
(`VolumeError::DeviceDisconnected`/`ConnectionTimeout` on the direct path; a transport-loss errno set or the read
timeout on the mount path — `classify_io_error`). Everything else per-file (permission denied, `EIO`, `EISDIR`) is
`FetchError::Unreadable`: skip it, count it (`PassSummary.skipped_unreadable`), log "N skipped: unreadable" at pass end,
write NO row (`Failed` stays reserved for a good read with a bad decode). Bias documented in `classify_io_error`: a
misread dead mount completes honestly and re-enriches next scan; a misread per-file fault would pause the pass against
a condition that never clears — exactly the TCC-EPERM stall this fixes. Without this line, an all-EPERM mount would
either stall forever (old behavior) or silently "complete"; the skip count keeps it loud.

**Path mapping.** An SMB index's `ROOT_ID` is the mount root, so `walk_image_entries` reconstructs MOUNT-RELATIVE paths
(`/DCIM/x.jpg`). `os_join(mount_root, rel)` prepends the mount root to reach the real file (`/Volumes/naspi/DCIM/x.jpg`);
for the `root`/local volume the mount root is `/`, so the path passes through unchanged. The stored `media.db` row keeps
the index-relative identity (matching the index + GC set); the network-enrichment UI reconstructs the display/open path via the mount
root.

**Non-blocking discipline (the crux).** A network read can block indefinitely on a hung transport. `FsByteFetcher` runs
its `std::fs` read on a throwaway thread and waits with `recv_timeout`; `VolumeByteFetcher` bounds the whole async read
with `tokio::time::timeout`. Either timeout returns `FetchError::Disconnected` (pause), never a wedge. Critically, the
fetch happens in the ENRICH layer, not on the serialized Vision OCR worker thread — the backend receives the
already-fetched bytes via `ImageInput.bytes` (`Some` = network, `None` = local read-it-yourself), so a hung transport
can never stall OCR of other (local) volumes. A `MAX_FETCH_BYTES` cap skips a pathological file rather than OOMing (the
direct fetcher also short-circuits on an over-cap size hint, without touching the wire).

### The conservative-fetch policy with teeth (`network/policy.rs`)

Typed knobs (`ConservativeFetchPolicy`), each a real gate, not a comment:

- **Priority-gated.** The pass proceeds only while the volume is CLEAR of higher-priority work
  (`volume_clear_for_enrichment`, pure and tested — `crate::priority`'s order: interactive > transfers > indexing): the
  app has been foreground-idle for `idle_threshold` (default 5 s) AND no user-initiated transfer is touching this
  volume (`priority::transfers`). `priority::foreground` holds the process-global "last foreground activity"
  timestamps, stamped by the hot foreground filesystem IPC (directory listing = every navigation); the pure
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
  remount, `Cancelled` via the next scan or user kick, so looping on either would spin the idle-wait against a
  condition this loop can't clear.
- **Bandwidth-bounded.** After each image, `throttle_delay(bytes, max_bytes_per_sec)` sleeps so the sustained fetch rate
  stays under the cap (default 8 MB/s). Pure and tested; it deliberately over-throttles slightly (ignores OCR time) — the
  conservative direction. (The parallel pass paces at dispatch on the index's last-known size, since the actual count
  lands on a fetch worker; a stale size self-corrects over the pass.)
- **Bounded concurrency.** `max_concurrency` (default 3) is the PARALLEL pass's prefetch fan-out width (§ Parallel
  enrichment); the sequential pass is inherently 1.
- **Resumable.** Each completed image persists immediately (path-keyed upsert), so an interrupted pass resumes from the
  store on the next scan; unchanged images skip via `needs_enrichment`.

### The "always index" override (`network/config.rs`) — why it's load-bearing

Navigation-based importance scores a rarely-browsed NAS archive LOW everywhere, so importance-first ordering would defer
the user's photos forever (plan Decision 6). The override forces enrichment regardless of importance. `should_enrich_image(covered_by_override, importance, threshold)` = `covered || importance ≥ threshold`. The importance
slider is present, but for network volumes the production importance oracle yields `None` and **the override is the load-bearing
input**: only override-covered volumes/folders enrich. The gate seam keeps the importance path drop-in.

**Storage: a settings-seeded global, not a fourth per-volume store.** The opt-in and overrides are user config (a handful
of volumes/folders), not per-image data, so they ride the sparse settings store (`mediaIndex.networkVolumes`,
`mediaIndex.alwaysIndexVolumes`, `mediaIndex.alwaysIndexFolders` — FE-owned) rather than a new SQLite DB with its own
writer thread (the standing-cost note already flags per-volume thread growth). The scheduler runs off the IPC thread and
consults `network::config` (a process-global `RwLock`) each pass, seeded from `load_settings` at startup and live-applied
through the `media_index_set_*` commands. Folder overrides store absolute OS-mount paths; `path_is_within` is a
trailing-slash-safe prefix so `/Photos2` isn't "within" `/Photos`.

*Non-load-bearing candidate (NOT built):* a photo-density importance input (a folder that's mostly images is likely an
archive regardless of visit count) could feed the importance oracle. Deliberately deferred; the manual override is the
current mechanism.

### Resumability across unmount + the disconnect data-safety lines

A mid-pass unmount is not a crash and not a bad file. On `FetchError::Disconnected` the pass returns
`NetworkPassOutcome::Paused { reason: Disconnected }`: it flushes every completed row (they survive), writes NO `Failed`
row for the in-flight image, and does NOT GC. The scheduler marks the volume paused (`network::config::mark_paused`),
which the coverage signal surfaces; resume happens via the registration bus on remount (the next completed scan re-runs
the pass, skipping already-Done rows). This is distinct from the `Failed` state, which is reserved for a genuinely bad
file (a GOOD read but a decode/OCR failure) — a transport fault must never masquerade as one.

**GC vs a mere disconnect.** Only a pass that ran to COMPLETION reaches GC (the same completed-scan edge the local pass
gates on). A paused/cancelled pass returns before GC, so a disconnect can NEVER wipe a volume's coverage — a paused
volume's rows survive intact until reconnect. Pinned by `network::tests::gc_does_not_fire_on_a_disconnect` and
`disconnect_mid_pass_keeps_completed_rows_and_writes_no_failure` (both verified red→green).

### Offline search after unmount (Decision 8)

`media.db` is keyed by `volume_id` (`media-{volume_id}.db`) and the `MediaIndex` read API opens it directly, so an SMB
volume's photos stay searchable with the NAS unplugged. `network::tests::search_answers_offline_after_the_volume_unmounts`
enriches over the fake fetcher, drops the writer (simulating unmount), and asserts the search still answers.

### MTP stays on-demand, never background

`wire_volume` skips MTP with a log: a phone/camera on MTP is transient and slow, so enrichment is on-demand-per-visit, not
a background sweep (keeping `importance/`'s `ScoringPolicy::for_kind` MTP exclusion). The never-background-sweep gate is
real now; the on-demand-per-visit trigger itself is a later slice (a clear TODO — nothing wires it yet).

### Backend commands + typed state for the network-enrichment UI

The backend provides three setters + the extended state. They live in `commands/policy.rs` with the other
coverage-changing commands (the scope, the threshold, the privacy exclusion), split from the read/query surface in
`commands.rs`: each mutates live `gate` / `network::config` state and has to decide whether the change BROADENS coverage
and needs an immediate pass, and each of those decisions is a pure `*_should_kick` fn tested in `commands/tests.rs`.

- `media_index_set_network_volume_enabled(volume_id, enabled)` — the per-volume SMB opt-in (live-applied; enabling kicks a
  pass).
- `media_index_set_always_index_volume(volume_id, always)` / `media_index_set_always_index_folder(folder, always)` — the
  overrides (live-applied). ADDING kicks a pass, removing doesn't: the volume setter kicks that volume's network pass
  when it's opted in, the folder setter kicks every ready volume (a path doesn't say which volume it's on) — § The
  indexing scope.
- `media_index_volume_state` extended with `network_opt_in`, `always_indexed`, `paused` (the "paused, resumes on
  reconnect" honesty).

**The FE surface (shipped).** The opt-in + volume override live in Settings > AI > Image search >
"Image search" card, below the master toggle, rendered by
`src/lib/settings/sections/MediaIndexNetworkVolumes.svelte` (only when `mediaIndex.enabled` is on). It lists each mounted
network (SMB) volume with an opt-in switch and, once opted in, an "always index this drive" switch plus a live status
line (indexing / paused-because-disconnected / count) polled off `media_index_volume_state`. Persistence + live-apply are
co-located in `src/lib/media-index/network-volume-prefs.ts`: each toggle writes the FE-owned array setting
(`mediaIndex.networkVolumes` / `mediaIndex.alwaysIndexVolumes`, persisted as a REAL JSON array so the Rust loader's
`Vec<String>` reads it — NOT the double-encoded JSON-string shape `indexing.silencedDrives` uses) AND calls the matching
setter, rolling the persisted value back if the IPC call rejects. These three settings needed a new `'string-array'`
`SettingType` (the store was scalar-only before). Cross-window edits re-seed the switches via
`onSpecificSettingChange`; startup seeding is the Rust `load_settings` path, so no `settings-applier.ts` entry (the
per-item setters don't fit its key→value passthrough table).

Coverage-honesty for a network volume lives in `src/lib/search/ImageSearchResults.svelte`: it takes an optional
`mountRoot` + `isNetwork` and reconstructs an openable OS path from each index-relative hit via the pure
`src/lib/search/media-path.ts` (`resolveMediaHitPath`, mirroring the backend `os_join`), and voices the network states
("turn on indexing for this drive" when not opted in, "disconnected, showing what's indexed" when paused). The Search
dialog reaches these states by following the FOCUSED PANE's volume: `+page.svelte` passes
`getFocusedPaneImageSearchVolume()` (the pure `resolveImageSearchVolume` over the volume store) as `imageSearchVolume`,
so browsing a NAS pane searches that NAS's `media.db` and hits resolve under its mount root; a non-filesystem pane (a
search-results snapshot) falls back to the local root. Filename search stays deliberately root-scoped (it reads the
local whole-drive index) — only the image grid follows the pane.

**Per-folder override — the chosen folders.** `media_index_set_always_index_folder` +
`mediaIndex.alwaysIndexFolders` back the chosen-folders list in Settings > AI > Image search
(`MediaIndexChosenFolders.svelte`, adding via the native folder picker). In the `ChosenFolders` scope these folders ARE
the coverage (§ The indexing scope). A folder's context menu drives the SAME setter through the same FE helper
(`always-index-folders.ts`), so a folder added by right-click shows up in the Settings list and vice versa; the menu's
label/enabled decision lives in `menu/media_index_items.rs` (§ Image-search group in `menu/DETAILS.md`).

## Schema (`store/`)

`SCHEMA_VERSION` is a disposable-cache version: a mismatch delete-and-recreates `media-{volume_id}.db`. It's now `4`
(v2 added the tag + embedding tables and the FTS `source` column; v3 added the `media_clip_embedding` table + the
`media_status.clip_stamp` column for CLIP semantic search — § CLIP semantic search; v4 is the "small at NAS scale" bump —
f16 embeddings + integer-id keying, § The f16-embedding + integer-id decisions). A bump re-enriches every beta user's
cache on next launch (Vision recompute only, no re-download — an accepted disposable-cache cost). Objects:

- `media_file` — `(id INTEGER PRIMARY KEY, path TEXT UNIQUE COLLATE platform_case)`: the identity table (plan M4). Each
  path is stored ONCE here; every other table keys on the integer `file_id`, and the reads join back to a path so the
  Rust layer stays path-addressed. A rename is a one-row `UPDATE media_file.path` (`MediaWriter::rename_path`).
- `media_status` — `WITHOUT ROWID`, `file_id INTEGER PRIMARY KEY`; `mtime`, `size` (with the path, the `(path, mtime,
  size)` staleness key); `media_kind` + `state` (typed TEXT tokens, `sqlite3`-inspectable, parsed back to typed enums —
  `no-string-matching`); `engine_version` (the combined **analyze provenance stamp** (§ The analyze provenance stamp) so an OS upgrade to the OCR
  engine, tag taxonomy, or feature-print model re-runs analysis even on an unchanged file — data-COVERAGE, not
  data-safety, since the derived data is disposable).
- `media_ocr` — a **standalone** FTS5 table (`file_id UNINDEXED, source UNINDEXED, text`). Not external-content:
  external content would sync via triggers off another table's integer rowid; a standalone table keyed by an UNINDEXED
  `file_id` keeps enrichment and GC a simple `WHERE file_id = ?` delete with no trigger machinery to desync. It holds up
  to two rows per file: the OCR text (`source='ocr'`) and the space-joined tag labels (`source='tag'`), so a keyword
  search matches **tags alongside OCR**. Created via `CREATE VIRTUAL TABLE … USING fts5`, which doubles as the FTS5
  availability guard (a `bundled` build without FTS5 fails there — Decision 2's build-flag worry is closed, `agent/store`
  proves it).
- `media_tags` — `(file_id, label, score)` with an index on `file_id` and on `label`: the STRUCTURED tags for tag-score
  filtering (`images_with_tag(label, min_score)`), distinct from the folded FTS keyword index above.
- `media_embedding` — `WITHOUT ROWID`, `(file_id PRIMARY KEY, dims, vector BLOB)`: the image feature-print embedding as a
  little-endian **`f16`** BLOB (`encode_embedding`/`decode_embedding`; `dims` = element count). The vector store's load
  source (§ The vector store + resident cache).
- `media_clip_embedding` — same shape (`file_id`, `dims`, `f16` `vector`), the CLIP image embedding in its SEPARATE vector
  space (§ CLIP semantic search).
- `meta` — `schema_version` only.

The `needs_enrichment` staleness predicate is `(path, mtime, size)` + the analyze stamp: stale when there's no row, or
when `(mtime, size)` changed, or when the stamp changed. State is deliberately excluded from the key so a failed file
isn't re-hammered every completed scan; a real file change re-tries it. A successful `upsert` resolves the path to its
`media_file` id (creating it if new), then writes `media_status` + the OCR/tag FTS rows + `media_tags` + `media_embedding`
in ONE transaction (clearing each prior row first, so a re-enrichment leaves nothing stale); a failure clears them all and
records only the `Failed` status. GC/prune delete every `file_id`-keyed child plus the `media_file` row.

### The f16-embedding + integer-id decisions (plan M3 + M4)

Both land in the ONE `SCHEMA_VERSION = 4` bump so a corpus re-enriches exactly once (the coordination invariant from the
plan). At NAS scale (~2M images) the two together roughly halve the per-image disk and the resident search RAM.

**Decision (M3): embeddings are `f16`, not `f32`, on disk AND in the resident cache.** The CLIP (512-d) and Vision
feature-print (768-d) vectors are the biggest per-image storage item (5 KB of f32 → 2.5 KB of f16). `encode_embedding`
writes f16 le bytes; `decode_embedding` widens to f32 (the query direction — a find-similar source vector), while
`decode_embedding_f16` loads f16 as-is for the resident `BruteForceVectorStore`, so the cache is half the RAM too.
**Why score against f16 directly (widen per element), not widen-on-load:** widening on load would keep the cache f32 and
forfeit the RAM halving; the brute-force scan is memory-bandwidth-bound, so f16 entries (half the bytes) keep or improve
query latency while halving RAM — the plan's "measure, pick the simpler one that keeps latency" resolved in f16's favor
by that bandwidth argument. `cosine_f16(query_f32, stored_f16)` widens each stored element inline (no temp `Vec`); dedup
widens each vector to f32 ONCE (O(n), not O(n²) per pair). Precision: f16 shifts a realistic embedding's direction by
cosine < 1e-3 (tested), far below ranking noise, and top-k order is preserved vs the f32 reference (tested on a
100-vector fixture with 0.008 score gaps).

**Decision (M4): one `media_file(id, path)` identity table; every other table keys on `file_id`.** NAS paths average ~80
B and previously repeated in every table (`media_status`, `media_ocr`, `media_tags`, both embedding tables) — gigabytes
of pure duplication at 2M, plus string-compare joins. Now the path lives once in `media_file`; children carry the 8-byte
integer. **The Rust layer stays path-addressed** (the scheduler's `statuses` map, the read API's `ImageFacts`, the vector
store's `SimilarImage` all key on `String` paths): the store's reads join `media_file` back to a path, so nothing above
the store learned about ids. **Why not merge `media_status` into `media_file`:** they're 1:1, but keeping identity
(`media_file`) separate from enrichment-state (`media_status`) makes a rename a tiny one-row update and matches the plan's
explicit shape. **Rename** (`rename_path`) is the payoff the keying buys — a single `UPDATE media_file.path` and every
child follows via the unchanged `file_id`; it's the seam a future rename-following hook calls (until one is wired, a
rename still manifests as GC(old) + enrich(new), which this replaces with an O(1) update). The full read-path audit
(every raw `path =` query against a media table became a `media_file` join): `read_status`, `read_all_status`,
`read_status_paths`, `sum_bytes_for_paths`, `read_all_embeddings_from`, `read_embedding_for`, `read_tag_matches` (store);
`search_ocr`, `facts_for_paths`, `images_with_tag` (read API); `scan_accounted` (coverage); the writer's upsert / GC /
prune / prune-prefix / purge.

## The image-qualification predicate (`predicate.rs`)

Pure over a directory's file names (sibling-aware): images enrich (JPEG/PNG/HEIC/…); videos skip (out of scope,
also a Live Photo's motion `.mov`); an image with a same-stem `.mov` is tagged `LivePhotoStill`; `.aae` edit sidecars
skip; a RAW beside a same-stem JPEG defers to the JPEG (cheaper decode), a lone RAW enriches. Classification is typed
(`Qualification`/`MediaKind`/`SkipReason`), never a substring branch. The scheduler groups the index walk by parent
directory and runs `qualify_dir` per group.

## The `VisionBackend` seam (`backend/`)

The inference boundary the scheduler, store, and GC sit behind, so all of that is testable with no GPU/ANE/FFI. The
trait is `VisionBackend`: `ocr` (OCR only, for the focused OCR tests), `analyze` (the enrichment entry point — OCR +
tags + feature print from ONE decode), and the provenance stamps `engine_version` / `taxonomy_version` /
`analysis_stamp` (§ The analyze provenance stamp). Two impls:

- `fake::FakeVisionBackend` — deterministic, zero-FFI (scripted/derived OCR text, tags, and a stem-derived unit
  embedding). Every test injects it via `MediaScheduler::new`; it's also the production fallback off-macOS.
- `vision::VisionOcrBackend` (macOS only) — the real OCR + classify + feature print. `scheduler::start` selects it on
  macOS.

CLIP embeddings and faces become sibling methods on this trait as later work lands, each returning its own typed result, each
fakeable the same way.

### The real Vision OCR backend (`backend/vision.rs`, macOS)

`ocr` decodes and recognizes text through Apple frameworks:

1. **Decode downscaled, in-memory (Decision 5 — no thumbnail files).** Read the compressed bytes, wrap in a `CFData`,
   open a `CGImageSource`, and `CGImageSourceCreateThumbnailAtIndex` with `kCGImageSourceThumbnailMaxPixelSize` = 3072
   (long edge) + `…FromImageAlways` + `…WithTransform` (EXIF-upright). This caps the decoded bitmap (~36 MB worst case)
   instead of letting Vision decode a 48-megapixel original (~190 MB). The compressed read is bounded; the decoded
   bitmap is the memory hazard the cap defends.
2. **Recognize.** `VNImageRequestHandler(cgImage:)` + `VNRecognizeTextRequest` (`.accurate`, language correction on),
   `performRequests`, then the top candidate per `VNRecognizedTextObservation`, newline-joined.

**Threading + the 8 MB stack.** Vision/ImageIO do synchronous XPC round-trips into system daemons (ANE) that can overrun
a small worker stack — the same hazard as calling AppKit off rayon (`src-tauri/CLAUDE.md`). So each backend owns a
dedicated OS thread with an 8 MB stack; `ocr`/`analyze_media` dispatch each image to it over a channel and block for the
reply. The single thread SERIALIZES that backend's Vision calls (Apple's recommendation for pooled inference) and
confines every `Retained`/`CFRetained` object to it (nothing `!Send` crosses a boundary — only the path `String` +
bytes in and the analysis out). Each job runs inside `objc2::rc::autoreleasepool`, so framework temporaries free per
image, not per pass.

### Parallel enrichment (plan M2)

**Decision: parallelism is N INDEPENDENT backends, not concurrent calls into one.** A `VisionBackend` is single-threaded
by construction (the CF confinement above), so N-way parallelism means N whole backends — each its own thread, stack,
autoreleasepool, and request handlers — driven by N worker threads in `scheduler/pool.rs`. Worker 0 rides the
scheduler's long-lived backend; workers 1..N are built on demand from a `BackendFactory` and dropped when the pool
shrinks, so a steady N=1 pass builds nothing extra and behaves byte-for-byte like the pre-M2 loop (the serial
`enrich_and_gc_scoped` is now a thin wrapper over the pool at width 1, so every enrich test exercises the pool core).

**Why measured, not asserted.** The M2 spike (`backend/vision/spike.rs`, recorded in `docs/specs/resource-use-plan.md`
§ M2) measured throughput topping out at ~1.25x by N=2 on an M3 Max (verified 2026-07-23, decode-vs-full-analyze scaling
at N ∈ {1,2,4,8} over 200 local images): the ANE serializes inference (~89% of per-image wall time) so it doesn't
parallelize; only decode (~11%, CPU) scales (to 5.4x at N=8). So the default is 1 (never take more machine unasked —
principle 5), the slider is explicit consent, and the microcopy doesn't over-promise.

**The win is decode↔inference OVERLAP, captured by symmetric workers.** The ~25% at N=2 is not "two inferences at once"
(the ANE won't) — it's that while worker A blocks in ANE inference, worker B decodes the next image on the CPU, so the
decode stage runs AHEAD of and overlaps the serialized inference stage. N independent full workers yield exactly this
pipelining without an explicit decode-stage/inference-stage split: the ANE is the single bottleneck, so a symmetric pool
feeding it keeps it saturated just as a dedicated decode-fan-in would, and is far simpler (no cross-stage `!Send`
handoff of `CGImage`s). Past N=2 the extra workers only pile up behind the ANE, hence the plateau/regression.

**Decision anchor — what would lift the ~1.25x ceiling (and what would NOT).** ❌ Do not "clean up" the slider as
useless because N=8 ≈ N=2 today: the cap is the ANE serializing per-request inference, NOT the thread model. What lifts
it is a BACKEND change — batched Vision requests (one `performRequests` over many images), an explicit
`MLComputeUnits`/compute-unit configuration, or a GPU/CPU inference fallback that adds a second parallel compute unit —
at which point the SAME pool scales further with no rework. More threads never will. The slider's max is the CPU count
by design (David's call), and the backend clamps to it; the honest low-gain-past-2 shape lives in the microcopy, not in
a shrunk range.

**How the pool honors N, live.** `run_enrich_pool` runs in batches: workers pull image indices off ONE shared atomic
cursor (each index taken once ⇒ no double-enrichment is structural, not a lock), re-reading the effective worker count
(`gate::parallelism` capped by `thermal::current_pressure`) between images. A SHRINK retires the excess worker slots
within the running batch; a GROW ends the batch so the outer loop re-spawns wider. The cursor never rewinds, so a
mid-pass slider move or a thermal event applies within ~one image with no pass restart. The single `MediaWriter` thread
is untouched (parallelize compute, never DB writes), and `gate::should_stop` (watchdog OR master toggle off) still stops
the pass promptly and skips GC.

**Thermal backoff** (`thermal.rs`, new M2 scope): `NSProcessInfo.thermalState` read as a TYPED enum (never a string —
`no-string-matching`) caps the EFFECTIVE workers — halved at `serious`, dropped to 1 at `critical` — so N workers
pounding the ANE can't cook the machine into a system-wide throttle that hurts the foreground app more than it helps
enrichment. It only ever lowers the user's chosen count.

**Network parallelism + the byte-bounded prefetch** (`network/enrich.rs`, `network/budget.rs`): the parallel network
pass is a three-stage pipeline. ONE dispatcher thread keeps every conservative fetch-side DECISION (priority gate,
coverage gates, byte-budget admission, bandwidth pacing, progress); K fetch workers (`max_concurrency`, plan M1)
perform the byte-reads in parallel — over SMB they spread across the scan-session connection pool (§ The byte-fetch
decision; the pass brackets `begin`/`end_scan_session` when direct AND parallel, refcounted on `SmbVolume` so an
overlapping index rescan shares the pool), which is what lets the reads genuinely overlap (ksmbd serializes per
connection); N compute workers (each its own backend) analyze and write. Prefetch admission is bounded by BYTES, not
file count (`ByteBudget`): the dispatcher acquires an image's size before handing it to a fetch worker and a compute
worker releases it after the decode, so the whole fan-out can't blow the memory ceiling on a RAW-heavy corpus
(256 MB/file cap × ~36 MB/decode would otherwise let a count-based queue buffer gigabytes). An over-cap file is
admitted alone (never deadlocks); a stop wakes a blocked acquire. The data-safety lines hold: a typed disconnect on ANY
fetch worker stops the dispatcher, queued jobs drain-release their reservations, compute workers drain the
already-fetched jobs, and NO GC runs (§ Resumability); the disconnect wins the pause-reason merge (a `NotIdle` retry
would only re-hit the dead transport).

**Expected NAS-side effect — still unmeasured.** M1's direct-session read path removed the dev-mount `EPERM` blocker,
but a real-NAS throughput number hasn't been produced yet (M1 validated correctness on the Docker SMB fixtures; a
bounded re-enrich of the ~9k-image NAS corpus is the intended measurement, deliberately not run as part of the M1
worktree). Reasoned expectation from the M2 spike + the design: prefetch fan-out hides per-file SMB latency behind
neighboring reads and behind compute, so the pass should approach the local ~1.25x-at-N=2 ANE ceiling instead of adding
wire latency on top. To measure: opt the NAS in, set `mediaIndex.parallelism` to 2, run a bounded re-enrich over the
existing corpus (bump a folder's mtimes or clear its rows), and read images/min off the `media-enrich-progress` log
against the 2026-07-16 ~60–80 img/min baseline. The byte budget is what makes the overlap SAFE (bounded buffer), never
a multiplier.

**FFI discipline.** Every `unsafe` block carries a per-site `// SAFETY:` naming the concrete invariant — pointer/buffer
validity for `CFData`/`CFNumber`/`CFDictionary` creation, Create-vs-Get ownership (the `+1 CFRetained` on every CF
`Create`), the extern-static reads for the ImageIO/CF constant keys, and the success-gate `Option`/`Result` on each
framework call — never a blanket file allow (`clippy::undocumented_unsafe_blocks`).

**Hostile input fails closed to a typed `VisionError`, never a panic/hang:** an unreadable/empty/non-image/undecodable
file returns `Decode`; a request failure returns `Ocr`. The pass logs it and marks the row `Failed`.

**`engine_version`** is `vision-ocr;os={major}.{minor}.{patch};rev={N}`: the macOS version (`NSProcessInfo`) plus the
current `VNRecognizeTextRequest` revision (read off a fresh instance). The `analyze` path additionally computes
`taxonomy_version` (the `VNClassifyImageRequest` revision) and folds all three revisions into `analysis_stamp` (§ The analyze provenance stamp).

The fixture for the macOS-gated real tests lives at `backend/test-fixtures/ocr-sample.png` (a tiny PNG rendering
"CMDR OCR" / "hello 2026", generated once via CoreGraphics text drawing).

## The read API (`read/`)

`MediaIndex` is the ONE consumer entry point (plan Decision 8), modeled on `importance`'s `ImportanceIndex`: it owns a
`platform_case`-registered read connection over `media.db` and reads directly, so it answers OFFLINE after a volume
unmounts. `search_ocr` returns `OcrHit`s (path + a highlighted `snippet` — the "why matched" reason). `build_ocr_match_query`
is the fts5 sanitizer: raw user input must NEVER hit `MATCH ?` (parens, colons, bareword `AND`/`OR` throw a syntax
error, and binding doesn't help — the string is parsed as query syntax), so each whitespace token is quoted into a
literal. Same gotcha as `agent/store`'s `sanitize_fts_query`.

### The lookup direction (`facts_for_paths`)

Every other read is query-direction (a query in, matching paths out). `facts_for_paths(&[&str]) -> Vec<ImageFacts>` is
the opposite: the caller already has the paths (the user navigated to a folder) and asks what's stored for each. It
backs the `image_facts` MCP tool and the natural-language bulk-rename flow that needs to know what's IN an image before
proposing a name. Four properties it exists to guarantee:

- **The FULL stored text, not a snippet.** `search_ocr` returns `snippet(media_ocr, 2, …)` because a UI highlights a
  match; a model naming a file has to read the whole thing.
- **OCR text and tags stay DISTINCT.** `media_ocr` holds up to two rows per path behind an UNINDEXED `source` column
  (`'ocr'` = recognized text, `'tag'` = the space-joined tag labels folded in for keyword search), so the text read
  filters `source = 'ocr'`; without that filter the tag labels come back dressed as recognized text. Tags are read from
  the STRUCTURED `media_tags` table instead, so each keeps its own label and score rather than the folded, score-less
  FTS row.
- **One row per requested path, in request order, keyed by the path AS REQUESTED.** A never-enriched file is
  representable (`indexed: false`), never silently dropped, so a caller can tell "ask again once indexing catches up"
  from "indexed, and there's genuinely no text in it" (`indexed: true`, `ocr_text: None`). A missing `media.db` answers
  every path as not-indexed rather than erroring, keeping the module's empty-not-error convention while still honoring
  the one-row-per-path contract.
- **Chunked at 900 paths per `IN (…)`.** SQLite's default host-parameter ceiling is 999 and a rename over a big folder
  clears that immediately.

Gotcha: `media_status.path` and `media_tags.path` carry `COLLATE platform_case`, but `media_ocr.path` is an fts5
UNINDEXED column and compares BINARY. Rows are matched back to the request string exactly, so a caller passing a
differently-cased spelling than the indexer stored reads as not-indexed. Callers pass paths from the same index/UI the
enrichment pass saw, so this doesn't bite in practice; don't "fix" it by lowercasing, which would break
case-sensitive volumes.

### The OCR search command (`commands.rs`)

`media_index_search_ocr(volume_id, query, limit?)` is the IPC door onto the read API (plan Decision 8): it resolves the
app data dir, opens `MediaIndex` for the volume, and runs `search_ocr` on a `spawn_blocking` worker (a sync
`#[tauri::command]` would block the IPC thread). `limit` defaults to 200 and is clamped to 1000. It returns
`Vec<OcrHit>` (path + highlighted snippet — the "why matched" reason); an empty query, an un-enriched volume, or an
offline/purged `media.db` returns an empty list, never an error. When the master toggle is off it short-circuits to an
empty list before opening `media.db` (defense in depth, mirroring `media_index_covered_count`; the frontend also hides
the OCR section entirely when off, so the command never fires from there). Because the read API reads `media.db` directly, the
command still answers with the volume offline (a NAS unplugged). Registered in BOTH `ipc.rs` and `ipc_collectors.rs`;
regen the typed bindings with `pnpm bindings:regen` after any command change. The frontend query-ui + thumbnail grid
that consumes it is a later slice.

## Settings + memory (`gate.rs`, wiring)

The master toggle `mediaIndex.enabled` (off by default, sparse-persisted) seeds `gate` at startup and live-applies via
`set_image_index_enabled`. The scheduler no-ops when off. Cancellation hooks into the EXISTING indexing memory watchdog
through `indexing::register_subsystem_stop_hook` (a new tiny hook registry `stop_all_indexing` runs), so `media_index`
yields to the SAME 16 GB resident-memory ceiling rather than a second independent one that would let two ceilings sum to
~2× over one pool. The gate's emergency-stop atomic is checked between images; enabling the feature clears it.

### Disabling stops the running pass (not just future ones)

Every pass's between-images cancel hook is `gate::should_stop`, the ONE predicate all three pass types check (the local
full pass at `run_pass_blocking`, the SMB network pass at `run_network_pass_blocking`, and the live tick at
`run_live_tick_blocking`). It's true on EITHER of two independent reasons: the watchdog's `is_cancelled` emergency stop,
OR the master toggle being OFF (`!is_enabled()`). So turning "Index image contents" off halts an in-flight pass (e.g. a
NAS pass at image 74 of 31,890) within a few images, reusing the SAME safe cancel exit the watchdog uses — the loop
breaks with `cancelled: true`, which SKIPS GC and keeps every already-enriched row. Disabling is "stop processing",
never "erase": no GC, no prune (the privacy retro-delete is the separate, explicit erase path).

**Decision: fold the disable into the cancel predicate, don't overload `CANCELLED`.** The two stop reasons stay SEPARATE
at the atomic level — disabling sets no flag, it's observed live off `is_enabled()`, so `is_cancelled` / `request_cancel`
keep their exact watchdog-only meaning. This also means re-enable can never leave a stuck flag: `set_enabled(true)`
clears `CANCELLED` and makes `is_enabled()` true, so `should_stop()` is false again, and `kick_all_ready_passes` starts
fresh passes (`a_pass_no_ops_while_disabled_and_enriches_once_enabled` pins the disable → no-op → re-enable → enrich
cycle). A distinct third atomic for disable would add state to reset for no gain. The between-images granularity is right
for a NON-destructive stop: unlike the exclusion veto (privacy, which re-checks before each upsert to close the
in-flight-analyze TOCTOU), one more image finishing after a disable just writes one more KEPT row, so the per-image loop
check is enough. `disabling_the_master_toggle_stops_a_running_pass_and_keeps_rows` (real red→green) pins the running pass
stopping early with rows preserved.

## Standing cost

`media_index` adds a THIRD long-lived writer thread per volume (index + importance + media) plus a per-volume `watch`
listener. Fine at a few-volumes scale, but it scales per mounted volume — note it before adding more per-volume threads.

## The frontend surface

Three IPC doors feed the UI; all live in `commands.rs` and are registered in BOTH `ipc.rs`
and `ipc_collectors.rs` (regen the typed bindings with `pnpm bindings:regen`).

- **`media_index_search_ocr`** — the OCR search (above). Consumed by the Search dialog's
  "Text in images" grid (`src/lib/search/ImageSearchResults.svelte`), which QueryDialog
  renders via its `resultsExtra` snippet slot (Search-only; Selection passes none). The
  grid reuses the SAME live query text as the filename results.
- **`media_index_volume_state`** → `MediaIndexVolumeState { enabled, indexing,
  enriched_count }` — the honest per-volume coverage signal (plan § Coverage honesty +
  per-volume state). `indexing` is a cheap in-memory snapshot off the scheduler's
  `PassCoordinator::is_running` (`MediaScheduler::is_enriching`); `enriched_count` is a
  `COUNT(*)` over `media_status` read off the IPC thread. Deliberately NOT a progress
  percentage or ETA — those come later. It lets the UI tell apart four states rather than ever
  showing a confident-looking empty result that's really "not indexed yet": off (hint to
  enable), still indexing ("results may be incomplete"), enriched-but-no-match (a genuine
  miss), and not-indexed-yet. It's polled per search (no event subscription yet; a
  subscription is a reasonable later upgrade).
- **`media_index_thumbnail_token` / `media_index_drop_thumbnail_tokens`** — the grid's
  thumbnails REUSE the existing viewer preview scheme (`cmdr-media://` via the viewer's
  `file_viewer::media` token registry), never a media_index-produced thumbnail file (plan
  Decision 5). `media_index_thumbnail_token` classifies a path by magic bytes and, for an
  image, mints a `cmdr-media://` token; the frontend builds the URL via the viewer's
  `mediaUrl` (single-source). **Token lifetime is the CALLER's here** — a viewer session
  drops its token at the window-close choke point, but the grid has none, so
  `ImageSearchResults.svelte` drops every token it minted when the result set changes or
  the component unmounts (`media_index_drop_thumbnail_tokens`), or the token map leaks path
  mappings. The scheme serves the FULL original bytes (browser-downscaled for the tile);
  that's the accepted cost of reusing the preview path rather than producing a
  downscaled thumbnail — a real thumbnail cache would be a media_index-produced file
  Decision 5 defers.

The "why matched" snippet (`[`/`]`-wrapped matched terms) is parsed to structured
segments by the pure `src/lib/search/ocr-snippet.ts` and rendered with `<mark>`, NEVER via
`{@html}` — a document whose OCR text contains markup can't inject anything.

The master toggle `mediaIndex.enabled` renders in Settings > AI > Image search
(a dedicated "Image search" card, `ImageSearchSection.svelte`), off by
default. It live-applies through `settings-applier.ts` → `setImageIndexEnabled` (no
restart), the standard backend-affecting-setting pattern.

## Tags, image-similarity embeddings, vector search, importance-prioritization

This slice adds Vision tags + image feature-print embeddings, a brute-force vector store, real importance-prioritized
scheduling, the settings-slider covered-count preview, the per-folder photo-search exclude, and an honest per-volume
progress denominator. Still zero model download (Vision-only); local + opted-in SMB both get tags + embeddings.

### `analyze`: one decode, three outputs (`backend/vision.rs`)

The enrichment path calls `VisionBackend::analyze`, not `ocr`. The real backend decodes the thumbnail ONCE
(`decode_thumbnail`, the shared downscale) and performs THREE Vision requests on a single `VNImageRequestHandler`:
`VNRecognizeTextRequest` (OCR), `VNClassifyImageRequest` (scene/object tags), and `VNGenerateImageFeaturePrintRequest`
(the image↔image feature print). Reusing one decode + one handler is the Decision-5 "decode once" applied across all
three — decoding the original three times would dominate cost.

- **Tags** (`read_tags`): the top `MAX_TAGS` (12) classifications above `MIN_TAG_SCORE` (0.1), highest confidence first
  (Vision returns them sorted, so the read breaks at the floor). The taxonomy is FIXED by the OS — **1,303 identifiers on
  macOS 26.5.1** (verified 2026-07-13 via `VNClassifyImageRequest::supportedIdentifiersAndReturnError().len()`). A
  taxonomy change on an OS upgrade re-tags via the provenance stamp below.
- **Feature print** (`read_feature_print`): the first `VNFeaturePrintObservation`'s raw bytes decoded per `elementType`
  (`Float` → `f32`, `Double` → `f64`→`f32`), length-checked against `elementCount` (a mismatch drops it rather than
  storing garbage). Vision's feature print is image↔image only (no text encoder — that's the later CLIP work).
- Every new `unsafe` block carries a per-site `// SAFETY:` (the request `new()`s, the observation accessors, the
  `NSData` byte read is the safe `to_vec`), same discipline as the OCR path.

### The analyze provenance stamp (plan Decision 4)

`analysis_stamp` folds the OCR engine revision, the tag-taxonomy (classify) revision, and the feature-print revision
into ONE stamp stored in the `media_status.engine_version` column and used by `needs_enrichment`. Because one decode
produces all three outputs, re-running the whole analysis when ANY component changes costs nothing extra, so a single
combined stamp is simpler than three per-output stamps and still satisfies "an OS taxonomy change re-tags" (the
taxonomy-version component bumps → the row goes stale → analyze re-runs → tags refresh). The fake exposes
`with_engine_version` / `with_taxonomy_version` to simulate either bump.

### The vector store + resident cache (`vector/`, plan Decision 2)

Brute-force cosine in Rust, NO `sqlite-vec` (a loadable extension our `rusqlite` isn't built for; a real build+signing
project adopted only if a library outgrows brute force, behind this same `VectorStore` trait). The store holds vectors as
**`f16`** (plan M3 — half the RAM of an f32 cache; § The f16-embedding + integer-id decisions). `cosine` (f32↔f32, the
query↔query case) and `cosine_f16` (an f32 query vs an f16 stored vector, widened per element) both guard degenerate
inputs (zero magnitude / length mismatch → `0.0`, never `NaN`). `BruteForceVectorStore::top_k` linearly ranks by
`cosine_f16` (source excluded, ties by path); `dedup_clusters` groups near-duplicates by single-linkage union-find over
pairs at/above a cosine threshold (default 0.9), widening each vector to f32 once (O(n), not per pair) then comparing via
`cosine_f16`, returning clusters of two or more.

`vector::cache` keeps a load-once `BruteForceVectorStore` per volume (keyed by `media.db` path), mirroring `search/`'s
warm `SEARCH_INDEX` arena, so a find-similar/dedup query doesn't reload the BLOBs each call (all query-time work runs OFF
the IPC thread via `spawn_blocking`). Invalidated per COMPLETED enrichment pass (not per write — that would thrash-reload
mid-pass; the plan accepts eventual consistency until a pass completes) and DROPPED wholesale by `clear_all` from the
memory-watchdog stop hook, so the resident vectors are counted against the ONE shared resident-memory ceiling.

### Importance-prioritized scheduling (the headline — plan Cross-cutting)

The local `run_pass_blocking` and the network `should_enrich` now read `importance/`'s `ImportanceIndex`
(`MediaScheduler::folder_scores` → `above_threshold(threshold)`), the SAME signal the importance slider sets. The scheduler:

- **orders** the walk by folder importance descending (`enrich::prioritized`), so high-importance folders enrich first;
- **filters** via a `should_enrich(path)` closure: an EXCLUDED folder never enriches (hard privacy veto, checked first);
  otherwise enrich when an "always index" override covers it OR its folder importance meets the threshold. A deferred
  image stays in the GC `current` set, so a below-threshold folder's rows are never wiped — only vanished files are GC'd.
- **`folder_scores` returns `Option`** — `None` when importance genuinely has no data for the volume (fresh, offline,
  importance disabled). "Has data" is `coverage::importance_scored`: a stamped `recompute_generation` OR any live weight
  row. Keying on the generation alone is wrong — an incrementally-maintained or schema-recreated store carries usable
  weights at generation 0 (see § The importance "has scored" detection). Floored junk (`node_modules`, caches,
  hidden/system) has no importance row at all, so it's excluded at any threshold.

Threshold lives in `gate` as an `f64`-bits atomic (`set_importance_threshold` / `importance_threshold`, clamped
`0.0..=1.0`), seeded from `mediaIndex.importanceThreshold` and live-applied by `media_index_set_importance_threshold`.
Default `0.0` (`DEFAULT_IMPORTANCE_THRESHOLD`): enrich every scored folder, the slider raises it to defer low-importance
folders. Importance keys on the INDEX identity, so the network gate strips the mount root off the OS path before the lookup.

### The indexing scope: chosen folders vs automatic (`gate::IndexScope`)

WHICH folders indexing may cover is an explicit user choice, not something inferred from a number:

- **`ChosenFolders`** (the default): coverage is the "always index" overrides and nothing else. Importance is never
  READ, so the threshold has no effect at any position.
- **`ByImportance`**: the overrides PLUS every folder at or above the threshold — the automatic behavior, and the only
  scope where the slider is shown.

**Decision: an explicit enum, not a sentinel threshold.** A "threshold 1.01 means chosen-only" encoding would put the
model back where this feature came from — inferred, undocumented, and impossible to read off the settings file. The
scope lives in `gate` as an `AtomicU8` beside the threshold, seeded from `mediaIndex.scope` and live-applied by
`media_index_set_scope`.

**Decision: the narrow scope REUSES the override-only path, it doesn't add a second gate.** `local_should_enrich`
already treats `scores: None` as override-only (the unscored-volume fallback, § Defer-until-scored), which is exactly
what "only folders I choose" means. `lifecycle::pass_coverage(scope, load_scores)` is the one place that resolves it: in
the narrow scope it returns `scores: None` WITHOUT calling `load_scores`, and — the part that matters — WITHOUT marking
the volume deferred-on-importance. Marking it would have the unscored → scored bridge re-kick a pass that has nothing
new to enrich. `coverage::stored_row_survives` takes the same scope so the reclaim partition can never propose deleting
a row a pass would keep, and `volume_state`'s `waiting_for_importance` is false in the narrow scope (there's no wait to
voice).

**Decision: narrowing the scope deletes nothing.** Switching to `ChosenFolders` re-partitions the stored rows — the
importance-covered ones become "doomed" — but nothing is written. Those rows stay searchable and surface through the
EXISTING kept-rows line and reclaim offer (§ Reclaim space), the same forward-only contract the slider has. There is no
new deletion path: reclaim's user-explicit prune is still the only way rows leave. `stored_coverage*` additionally
partitions without importance in the narrow scope (an empty score map rather than `None`), so the reclaim offer works on
a volume importance never scored — exactly the volume someone narrowing their scope is likely looking at.

**Decision: adding a chosen folder kicks a pass.** `media_index_set_always_index_folder` kicks every ready volume when
a folder is ADDED and the feature is on (the path alone doesn't say which volume it's on; a pass on an unrelated volume
is a fast staleness no-op and the coordinator coalesces). Removing kicks nothing. This mirrors the SMB opt-in and the
threshold-decrease kicks: without it a chosen folder would sit unindexed until the next scan completion, which on a
quiet local drive can be hours, and the feature would look inert at the exact moment the user acts.

**Migration.** `gate::scope_from_settings(scope_token, was_enabled)`: a stated scope wins; with none, an install that
already had image indexing ON resolves to `ByImportance` (someone running it today is running the automatic behavior,
and narrowing their indexed set at launch is a change they never asked for), everyone else to the default. Image
indexing is off by default, so that second group is nearly everyone. The frontend `migrateSettings` (schema 3) writes
the key once with the same rule; the Rust fallback covers the launch before that migration runs, so the two can't
disagree.

### Defer-until-scored

When `folder_scores` is `None` (importance unavailable), BOTH the local and network passes DEFER their
importance-gated remainder while still honoring an explicit `config.covers` override — `local_should_enrich` and the
network `should_enrich` share this shape. The local pass does NOT fall back to enrich-all: importance's recompute over a
big volume takes seconds, and a pass that read `None` and enriched everything would over-index the whole volume
permanently, because the slider is forward-only (a below-threshold row is never deleted by moving the slider; only an
explicit reclaim or the privacy veto deletes). A visible, recoverable wait beats permanent over-indexing.

The **unscored → scored bridge** re-kicks the deferred remainder once importance lands:

- `wire_volume` subscribes to `importance::read::subscribe(volume_id)` SYNCHRONOUSLY, before the first pass. Watch-channel
  semantics: a receiver is caught up to the current version at subscribe time, so `changed()` fires only on the NEXT
  bump. A lazy "pass reads `None` → then subscribe" flow has a hole — importance can complete in the gap, the receiver
  comes up already-caught-up, and the volume defers forever. Subscribing up front (mirroring `search`'s
  `start_importance_weight_subscriber`) closes it.
- A pass that deferred sets a per-volume flag (`mark_deferred_for_importance`); the subscriber's
  `take_deferred_for_importance` reads-and-clears it and re-kicks a pass ONLY on that unscored → scored transition. Both
  the lifecycle bus and incremental rescores bump the recompute watch, so scoping the re-kick to the flag keeps a normal
  (already-scored) volume from re-kicking and a later incremental bump from re-walking the index for nothing.

The residual risk is made VISIBLE, never silent: the importance "has scored" detection guarantees the recompute *trigger*, not its *success* (a read-pool
or write error leaves generation 0 with no notify). Under defer-until-scored that would mean image indexing silently
never starts. So `media_index_volume_state` exposes `waiting_for_importance` (enabled + index ready + not scored), and the
settings slider voices it ("Working out which folders matter…") REPLACING the generic covered-count spinner — one honest
line for one wait, never two spinners. There is deliberately NO silent fallback to enrich-all on timeout: a persistently
failing recompute is an importance bug to surface, not to paper over.

### The importance "has scored" detection

`media_index` decides "has importance scored this volume?" via `coverage::importance_scored` — used by BOTH
`MediaScheduler::folder_scores` and `coverage::importance_scores`. It returns `true` when a full pass stamped a
`recompute_generation` OR any weight row exists (`ImportanceIndex::scored_folder_count() > 0`, a cheap `COUNT(*)`). The
generation-only check that predated this reported "never scored" for two real stores that carry perfectly usable
weights: a store maintained only by INCREMENTAL rescores (the incremental path never bumps the generation), and a
schema-recreated store between its recreate and its first full pass. Both then showed "0 covered" at every threshold. The
matching importance-side fix (a fresh/recreated store actually GETS a full pass at startup) lives in
`importance/DETAILS.md` § The initial full pass; fixing both means media's read-side check is defense in depth, not the
only guard.

### Covered-count preview + honest progress (`coverage.rs`, `commands.rs`)

`media_index_covered_count(threshold, volume_ids)` powers the slider's live preview: across the ENABLED volumes
(master on AND (local, or SMB opted-in); MTP never), how many folders score `≥ threshold` and how many images they hold
— exactly `(importance ≥ threshold) AND opted-in`, never a non-opted-in SMB/MTP volume. The qualifying-image count per
folder is an O(entries) index walk, so it's cached per volume (`coverage::get_or_build`, a `folder → count` map) and the
threshold is applied cheaply by intersecting with `above_threshold` — a debounced drag
only re-runs the cheap importance read + `covered_in_scope` (pure, unit-tested; it dispatches on the scope, so the
count follows the same rule the enrichment gate does — § The indexing scope). `pending` is `true` when any enabled
requested volume isn't ready (still scanning / not yet scored), so the UI voices "naspi still scanning" rather than a
confident wrong number. `media_index_volume_state` gained `qualifying_count: Option<u64>` (the honest denominator for
"12,000 of 38,900 images", `None` when offline/scanning); ETA math lives UI-side off `(enriched_count, qualifying_count)`.

**Keeping the cache warm, not cold.** The cache would go cold on every pass if a pass just invalidated it, so the next
slider preview would pay the full O(entries) walk again (tens of seconds to minutes on a multi-million-entry root index)
— even though the pass had just run that exact walk and thrown it away. So the pass that owns a walk refills from it
instead: a full/network pass calls `coverage::replace_from_entries` with its whole-volume `walk_image_entries` result,
and a live tick calls `coverage::patch_touched_dirs` to replace just the counts for the dirs it re-walked (a tick can't
rebuild the whole cache — it only walked the touched dirs). `patch_touched_dirs` runs on the SAME
`enriched > 0 || gc_count > 0` condition the live tick's vector-cache invalidate does: both a GC'd deletion and a
new/changed image move a touched dir's qualifying count. The only remaining cold walk is the first preview after launch
(or after a drive-index rescan, whose following pass refills). `coverage::invalidate` survives for the rare reclaim /
retro-delete prunes: those change no index rows (only stored `media.db` rows), so the qualifying set is actually
unchanged and invalidate is conservative — a cheap cold rebuild on a rare user action rather than a stale count. The
streaming `walk_image_entries` (ordered by `parent_id`, one dir-group in memory at a time) keeps even that cold rebuild
in constant memory instead of materializing every file row.

### The per-folder accounted aggregate + the index-status indicators (`coverage.rs`, `commands.rs`)

The covered-count cache above is the DENOMINATOR (`eligible`: images the drive index says qualify per folder). The quiet
per-image / per-folder / per-drive index indicators also need the NUMERATOR: how many of those are actually indexed. So
`coverage.rs` maintains a per-directory `accounted` count = images whose `media_status` row is `done` OR `failed` (both
count — a `failed` image can't progress, so completion is `accounted == eligible`, else one corrupt file keeps a folder
reading incomplete forever).

**Why a SEPARATE cache from `COUNTS`, not a field on it.** The two aggregates have different sources and update models:
`eligible` (`COUNTS`) is REBUILT from a whole-volume index walk each pass (`replace_from_entries`), reflecting the live
filesystem; `accounted` (`ACCOUNTED`) is maintained INCREMENTALLY from the stored rows. Folding accounted into `COUNTS`
would let the walk-driven `replace_from_entries` wipe the incrementally-maintained counts every pass. They're reported
together by the folder-coverage command but live apart.

**The maintenance invariants (mirroring how `eligible` is seeded and patched):**

- **Seed** once from a `SELECT path, state FROM media_status` scan bucketed by parent dir. This happens on the ONE writer
  thread as its FIRST action (`writer_loop` calls `coverage::seed_accounted_from_conn` before processing any message), OR
  lazily via `ensure_accounted_seeded` when the folder-coverage command runs before the writer spawned this session
  (feature just enabled / volume never enriched). Both go through `seed_accounted_if_absent` (insert-if-absent).
- **Increment** on a genuinely-new completion: `apply_upsert` does a cheap PK existence check (`SELECT EXISTS(…)`) inside
  its transaction and returns whether it INSERTED vs updated; the writer bumps `accounted[parent_dir] += 1` only on a new
  `done`/`failed` row. A `done`↔`failed` transition or a re-enrich of an existing path does NOT move it (the path was
  already counted).
- **Decrement** on deletion: GC / prune / retro-delete return the paths whose `media_status` row actually existed
  (`delete_rows_for_paths` collects the ones `DELETE` reported), and the writer `-1`s each parent dir (saturating, never
  negative). `PurgeVolume` resets the whole aggregate.
- **Subtree rollups**: `folder_coverage` returns each folder's `eligible` and `accounted` summed over the folder AND all
  descendant dirs (`build_subtree_rollup` adds each dir's count to itself and every ancestor). The rollup is cached
  alongside the per-dir map (`VolumeAccounted.subtree`, `ELIGIBLE_ROLLUP`) and invalidated on any change; a query is a
  cached-map lookup, NEVER a `media_status` scan.

**The concurrency line (why insert-if-absent is race-free).** The writer is the ONE mutator of both `media.db` and this
volume's `accounted`, and it seeds BEFORE its first commit. So whenever a committed row could exist, the entry is already
present, and a concurrent command-side seed either wins first (a complete on-disk baseline, since no writer delta can have
landed yet) or finds the entry present and discards its scan. Either way the writer's deltas compose onto exactly one
baseline. A delta on an unseeded volume is a no-op (never inserts a partial entry a later seed would wrongly trust).

**Staleness caveat (accepted first cut).** A `done` row whose file changed since indexing still counts as `accounted`
until it's re-enriched, so a folder / drive can briefly read "complete" while a changed file awaits re-work. Excluding
stale rows would need a per-row `(mtime, size)` compare against the live index, out of scope here (the per-FILE badge
does surface `stale` via `needs_enrichment`, but the folder/drive rollups don't subtract it).

**The two commands** (both `spawn_blocking`, both speak the volume's INDEX-path space — == the OS path for a local
volume; a network volume's mount-root mapping is a later slice, so the file overlay ships local-first):

- `media_index_file_status(volume_id, paths) -> Vec<FileIndexStatus>`, one per input path IN ORDER.
  `FileIndexStatus { path, state }` with `state` a camelCase enum: `indexed` (a `done` row, current per
  `needs_enrichment`), `stale` (a stored row the live `(mtime, size)` or the analyze engine stamp made stale), `failed`
  (a `failed` row), `pending` (an eligible image the coverage gate would enrich but which has no row yet), `excluded`
  (an indexable image the gate would NOT enrich — out of scope / below threshold / under an excluded folder),
  `notApplicable` (not a qualifying image → no badge). Backend does ALL classification: a bounded, dir-scoped
  `walk_image_entries_in_dirs` supplies each path's live `(mtime, size)` + sibling-aware qualification, `media.db`
  supplies the stored row, and `local_should_enrich` + the live exclusion veto split `pending` from `excluded` (only for
  an un-enriched image). A stored row WINS over the gate: an indexed image reads `indexed`/`stale`/`failed` even if the
  current setting no longer covers it (forward-only, the rows stay searchable). The `pending`/`excluded` scores are
  threshold-filtered exactly as `pass_coverage` sees them (`coverage_scores`). The staleness engine stamp comes from
  `MediaScheduler::current_analysis_stamp`; a missing scheduler falls back to each row's own stamp (only `(mtime, size)`
  staleness).
- `media_index_folder_coverage(volume_id, folder_paths) -> Vec<FolderCoverage>`, one per input folder in order.
  `FolderCoverage { path, eligible, accounted }` (subtree totals). The frontend derives the two-state folder badge
  (`accounted == eligible` vs `<`, no badge when `eligible == 0`) and the `accounted/eligible` tooltip. It calls
  `ensure_accounted_seeded` first (in case the writer hasn't spawned), then reads the cached rollups.

Both feature-off-short-circuit (`notApplicable` for every file, zeros for every folder), matching the other commands.

### Per-folder photo-search exclude + the privacy retro-delete

`network::config` gained `excluded_folders` (seeded from `mediaIndex.excludedFolders`, live-applied by
`media_index_set_excluded_folder`): an image at or under an excluded folder never enriches, a HARD veto that beats any
"always index" override — the privacy complement to the opt-in (protect a high-importance `~/Documents/IDs` the
threshold alone can't).

Excluding a folder does more than veto the future: it **retro-deletes** the folder's already-indexed rows so extracted
OCR text stops being searchable at once (privacy is a hard requirement, not "eventually on the next GC"). The pieces:

- **Why it's a new deletion path (vs the GC-safety doctrine).** GC's safety comes from *when* it runs (only a
  `Completed` edge, tree whole — § The GC safety argument). The retro-delete is USER-EXPLICIT and derives ONLY from
  settings state (the exclusion the user just set), never scan/bus/gate state, so it can't wipe live coverage by
  mistiming — it needs no edge. This is the same doctrine the reclaim prune rides. The slider stays forward-only;
  the ONLY row deletions are (a) vanished files via GC on a completed edge, (b) the reclaim prune, (c) this privacy
  retro-delete, and (d) the live-tick scoped GC (index-confirmed removals under the touched dirs — § Live enrichment).
- **Precedence + path mapping.** Exclusion beats coverage everywhere (enrichment gate AND retro-delete), same
  trailing-slash-safe `path_is_within` the veto uses. The exclusion config is OS-path keyed; local rows store index
  paths == OS paths, network rows store mount-stripped index paths — so the retro-delete maps the OS folder into each
  volume's index space via `network::fetch::os_folder_to_index_prefix` (the inverse of `os_join`: passes through on a
  local volume, strips the mount root on a network one, `None` when the folder isn't under that mount).
  `MediaScheduler::retro_delete_excluded_folder(folder, mounts)` iterates the reachable volumes, prunes each via its ONE
  writer, `VACUUM`s (privacy: the text leaves the disk), and drops the vector + coverage caches.
- **Two mid-pass races, both closed** (else the retro-delete is cosmetic). (1) A pass already running holds a
  start-of-pass config snapshot, so its coverage gate is stale — but exclusion is read LIVE
  (`network::config::is_excluded`, the ONLY live part; threshold/override stay snapshot), so the next image it looks at
  is vetoed. (2) The in-flight-analyze TOCTOU: a pass checks the veto, runs a SECONDS-long `analyze`, then upserts; an
  exclusion landing during the analyze would slip a row past the passed check, and a later pass won't collect it (the
  file is still in the GC `current` set). Closed by re-checking the live veto immediately before EACH upsert (both
  cores). Belt-and-suspenders: the command sequences config-set (live veto first) → retro-delete → retro-delete again
  (a double-tap; the blocking prune is its own barrier), so a straggler upsert that squeezed into the enqueue window is
  swept. Order matters — the config write MUST precede the first delete, or in-flight images re-check stale state.
- **Un-excluding** only clears the veto: NO re-delete and NO auto re-enrich — the next natural pass picks the folder up
  again.
- **Offline network volumes** aren't reachable when the exclusion is set (no mount root to map with), so the
  retro-delete skips them and RE-FIRES on reconnect: `wire_volume` (the registration hook) purges any currently-excluded
  folder under a volume as it (re)registers. Cheap when nothing is excluded.
- **The trigger** is a folder context-menu item ("Don't index images in this folder" / "Index images here again", shown
  only while image indexing is on, exactly one keyed on the current state). It's a NATIVE (Rust) menu, so the click
  emits a `MediaIndexFolderExclusion` event to the FE, which persists `mediaIndex.excludedFolders` and calls
  `media_index_set_excluded_folder` (the native menu can't write the FE settings store) — the persist + live-apply +
  rollback pattern from `network-volume-prefs.ts`, in `src/lib/media-index/excluded-folders.ts`, wired in the main
  route's `setupMenuListeners`.

### WAL checkpoint at pass completion (Decision/Why, plan M9)

**Decision:** the writer runs `PRAGMA wal_checkpoint(TRUNCATE)` (`writer::run_wal_checkpoint`, driven by
`MediaWriter::checkpoint_wal`) once an enrichment pass completes and actually wrote rows — at both the local
(`run_pass_blocking`) and network (`run_network_pass_blocking`) seams, inside the same `enriched > 0 || gc_count > 0`
guard that drops the vector cache. It runs on the writer thread's own connection (the single-writer invariant), in
autocommit. This mirrors `importance/writer.rs::run_wal_checkpoint` verbatim (this module ports importance's patterns);
the "why", busy tolerance, and the 250 ms bracket are documented there.

**Why here:** without a `wal_autocheckpoint` override, SQLite's default PASSIVE autocheckpoint never shrinks the WAL
file, so a per-image-upsert enrichment pass lets it creep up in place. A pass completion is the natural quiet point to
TRUNCATE it back down (target ≤ ~16 MB at rest). Best-effort: the callers `let _ =` the result, so a reader-blocked
checkpoint never fails a pass. Distinct from `VACUUM` (the reclaim/retro-delete path below), which reclaims free *pages*
in the main DB after deletes; the checkpoint reclaims the *WAL file* after writes.

### Reclaim space

Lowering the importance slider is forward-only: it never deletes rows, so a drive indexed at a broad setting keeps that
coverage after the user narrows the setting (the GC `current` set stays the full walked image set — § The GC safety
argument). The reclaim UI surfaces that leftover coverage and offers to delete it. Like the privacy retro-delete, the
prune is USER-EXPLICIT and derives ONLY from settings state, so it needs no `Completed` edge — it's deletion path (b) of
the four the slider's forward-only contract allows.

- **One arithmetic source, or the numbers don't add up.** `MediaScheduler::stored_coverage(volume_id, mount_root,
  threshold)` computes THREE quantities from ONE pass so the reclaim preview, the prune, and the per-volume `keptCount` can never
  disagree: `surviving_stored` (stored rows inside coverage), `doomed_stored` (outside it — the reclaim "delete N" AND the
  `keptCount`, the SAME set), and `covered_qualifying` (drive-index qualifying images in covered folders — the slider
  preview's number, a DIFFERENT thing: it counts what WOULD be indexed, not what IS). It guarantees `total_stored =
  surviving_stored + doomed_stored`, and reuses the `coverage.rs` cache path for `covered_qualifying` (never a second
  derivation). In the AUTOMATIC scope it returns `None` when importance hasn't scored the volume (importance's scoring
  makes that transient) — the partition can't be computed safely, so the command reports `pending` and the UI hides the
  reclaim line rather than proposing a destructive count off a lower bound. In the narrow scope importance isn't an
  input, so it partitions against an empty score map and stays answerable (§ The indexing scope).
- **The partition rule** (`coverage::partition_stored`, pure) reuses the SAME precedence enrichment does: a stored row
  survives when it's NOT under an excluded folder AND (covered by an "always index" override OR — in the automatic
  scope only — its parent folder scores at or above the threshold). Crucially it keys on score-MAP MEMBERSHIP, not a `>= 0.0` on a defaulted score: a folder
  with NO importance row (floored junk, or scored away since enrichment) is treated as below any threshold → doomed,
  even at threshold 0.0. Spell this out — otherwise a floored folder's rows leak into neither bucket. `is_override` /
  `is_excluded` take the stored (index) path; the wrapper wires the OS-mount mapping (`os_join`, identity on a local
  volume) so override/exclude config (OS-path keyed) and importance (index-keyed) both resolve.
- **The writer thread IS the race guarantee.** `prune_below_threshold` computes the doomed set up front and hands it to
  the volume's ONE writer thread (`prune_paths`) as a single serialized delete unit, then `VACUUM`s and drops the vector
  + coverage caches. A concurrent enrichment pass can't interleave mid-batch (both flow through the one writer), and it
  enriches only ABOVE-threshold or override-covered rows — a set disjoint from the doomed (below-threshold) set by
  definition — so a pass running NEW rows during the prune is fine. No snapshot-vs-live dance is needed here (unlike the
  exclusion veto): the doomed set is a concrete path list, not a live predicate.
- **Byte estimate.** `store::sum_bytes_for_paths` streams `media_ocr` + `media_tags` + `media_embedding` once each and
  sums the content bytes of the doomed paths (a set membership test, so no giant `IN (…)` for a 200k doomed set). It's a
  content estimate (excludes FTS-index + page overhead), so it's an honest "about" and a `VACUUM` reclaims at least it.
  The preview's "free about X" and the prune's "Freed X" use the SAME method, so the two numbers agree.
- **Commands** (both thin, `spawn_blocking`, offline-capable): `media_index_reclaim_preview(threshold, volume_ids) →
  ReclaimPreview { total_stored, covered_stored, doomed_count, estimated_bytes, pending }` and
  `media_index_prune_below_threshold(threshold, volume_ids) → ReclaimResult { deleted_rows, freed_bytes }`. Both resolve
  the enabled volumes (local root, mount `/`; opted-in SMB, its mount root; MTP and non-opted-in SMB dropped) and
  aggregate `stored_coverage` / `prune_below_threshold` per volume.
- **The FE surface** is `MediaIndexReclaim.svelte` under the slider (`getEnabledMediaIndexVolumeIds` shared with the
  slider preview). It shows the line + button only once counts settle (parent-passed `blocked` while waiting on
  importance / a scan, plus the backend `pending`) AND the leftover clears the pure `shouldOfferReclaim` floor (> 100
  rows AND > 5% of stored). The copy frames value first (the extra entries "stay searchable"), then the button offers
  the space-vs-reindex tradeoff — one narrative, composing with the kept-rows line, never two sentences in tension. A
  confirm dialog (recoverable, but re-reading costs time) precedes the prune; an honest toast reports the freed space.

### Progress events + vanished-file skip

A pass joins the top-right indexing indicator as a second publisher (the FE side is `lib/indexing/DETAILS.md` §
Image-enrichment publisher). `events.rs` defines two typed Tauri events + the emission machinery:

- **`MediaEnrichProgressEvent`** (`media-enrich-progress`): throttled progress. `total` / `bytes_total` are the
  ENRICHABLE-subset denominators (`enrichable_totals` / `network_enrichable_totals` = images passing `should_enrich` AND
  not `is_excluded`), NEVER the full walked set — a raw `images.len()` denominator rebuilds the never-finishes bug inside
  the indicator. `done` counts every subset image the pass finishes handling (enriched, already-current, or a quiet
  skip), so it reaches `total` on completion. Bytes ride `ImageEntry.size` (`Option`, `None` counts 0 — under-count,
  never lie). The pure `should_emit_progress` throttle (`progress.rs`) fires at pass start, then ≤ every 500 ms or 100
  images. Emission is a cheap counter + time check per image; the `EnrichProgressSink` seam keeps the registry-free
  cores testable (a recorder in tests, the throttled `TauriEnrichEmitter` in production).
- **`MediaEnrichTerminalEvent`** (`media-enrich-terminal`): exactly one per pass on EVERY exit path. The
  `EnrichTerminalGuard` (RAII, `events.rs`) guarantees it: it defaults to `Failed` and emits on `Drop`, so a `?`-error
  bubble (a writer-send failure) still reports a terminal; `run_pass_blocking` / `run_network_pass_blocking` override the
  reason (`Completed { enriched, gc_count }` / `Cancelled` / the two `Paused*`) before a clean exit. Without a terminal
  on every path the FE row sticks at "enriching" (the `index-scan-aborted` stuck-row bug). The local pass distinguishes
  cancel from completion via `PassSummary.cancelled`; the network pass maps its `NetworkPassOutcome`.

The scheduler holds an `Option<AppHandle>` (set in `start` via `new_with_app`, `None` in unit tests via `new`), so a
pass emits nothing under test. `pass_emitters` builds the sink + guard (both no-ops when the app is absent).

**Vanished / phantom files are DEBUG, never WARN.** A file deleted between the index walk and its analyze, or an
orphaned index row whose reconstructed path can never read, surfaces at analyze as a typed `VisionError::Missing` (the
real backend classifies the local `std::fs::read` ENOENT by io kind, never a message match; the fake scripts it via
`missing_for`). The local core skips it QUIETLY (DEBUG), writes NO row (not `Failed` — the file is gone, so a later
completed pass's GC collects any stale row), and counts it as processed so `done` still reaches `total`. The network
core already handles a vanished source via `FetchError::NotFound` (same quiet skip). The too-small-image skip is a
sibling quiet case: it writes an empty `Done` row instead. Pinned by
`enrich_tests::a_vanished_image_still_completes_the_pass_at_done_equals_total` and
`enrichable_totals_excludes_deferred_and_excluded_images`.

### Threshold-aware volume state

`media_index_volume_state` gained `covered_qualifying_count` + `kept_count`, from `MediaScheduler::stored_coverage_counts`
— a counts-only sibling of the reclaim `stored_coverage` that does NOT allocate the doomed-path `Vec` (the settings poll runs
it every few seconds). Both share the ONE canonical survival rule (`coverage::stored_row_survives`) and the `coverage`
cache, so they can never disagree with the reclaim preview. `covered_qualifying_count` drives the settings progress line
"N of M in your covered folders" (N = `enriched_count − kept_count`, capped); `kept_count` (= the reclaim doomed count) drives
the quiet "K more indexed from broader settings, still searchable" line, gated by the SAME `shouldOfferReclaim` floor so
it never duplicates the reclaim offer. Both `None` when the partition isn't safe (the automatic scope on an unscored
volume). `waiting_for_importance` is likewise false in the narrow scope — there's no wait to voice there.

### Live enrichment: follow the index

Without live enrichment, the only enrichment triggers are scan-completion edges, user kicks, and the importance bridge — so a NEW or
MODIFIED image would wait for the next completed scan, and a DELETED image's rows would linger until a later pass GC'd them. Live enrichment
follows the index live, mirroring importance's incremental rescore rather than inventing a new mechanism.

`scheduler/live.rs` subscribes each LOCAL volume to `indexing::lifecycle::lifecycle_bus::subscribe_dirs_changed` (the SAME per-volume
`watch<DirsChanged>` importance's `start_incremental` consumes) from `wire_volume`, AFTER its kind early-returns — so MTP
and `LocalExternal` are auto-skipped, and SMB (which never publishes dir-changed batches; its live path only enqueues index
writes) is left out too. Each batch's touched DIRECTORY paths accumulate into `pending_touched_dirs` and drive a coalesced,
throttled tick (`LIVE_THROTTLE_WINDOW`, leading-edge-immediate then trailing-edge-spaced — `live_debounce_wait`, copied
from importance's `INCREMENTAL_THROTTLE_WINDOW` / `incremental_debounce_wait`). `DirsChanged.paths` carries every changed
file's parent PLUS its ancestor chain up to the ever-present `/`; ancestor re-checks are harmless (staleness makes them
no-ops), and `/` resolves to a cheap direct-children walk, not a whole-index sweep. `watch` is last-value-wins, so a burst
can drop intermediate batches — the accumulator plus the next full pass heal it.

A tick (`run_live_tick_blocking`) walks ONLY the touched dirs (`walk_image_entries_in_dirs`: per dir, resolve its entry id
via `store::resolve_path` from `ROOT_ID`, fetch the COMPLETE file-child set, run the sibling-aware `qualify_dir` — fetching
only changed files would mis-qualify RAW+JPEG pairs and Live Photos; a dir gone from the index is skipped and its rows fall
to the scoped GC). It then runs the SAME per-image enrich loop as the full pass through the shared `enrich_and_gc_scoped`
core, honoring the coverage gates, the live exclusion veto, and the `(path, mtime, size)` + stamp staleness key.

**The GC data-safety line.** `enrich_and_gc`'s GC is a whole-store set-difference against the walked
set — correct for a full pass (whole index walked), CATASTROPHIC for a scoped walk (it would delete every stored row
OUTSIDE the touched dirs). So the GC target set is a parameter: `GcScope::WholeStore` (the full pass / Fresh sweep, via
`enrich_and_gc`) vs `GcScope::TouchedDirs` (the live tick, via `enrich_and_gc_scoped`), which GCs only rows whose parent dir
is one of this tick's touched dirs AND absent from the scoped walk. This makes the live tick the THIRD deletion path that
bypasses the completed-scan edge (§ The GC safety argument), alongside the privacy retro-delete and the reclaim
prune. Unlike those two (USER-EXPLICIT, settings-derived), the live tick's deletion is INDEX-CONFIRMED: a removal from the
live index is a fact about the tree (like importance's subtree clear), not a scan-state inference, so the complete-tree
doctrine isn't violated. A disconnect/unmount still never deletes: no read pool ⇒ the tick no-ops before any GC. The
sibling edge is where the whole-dir fetch earns its keep — deleting `DSC.jpg` promotes the lone `DSC.cr2` to enrich WHILE
scoped-GCing the `.jpg` row, in one tick.

**Guardrails.** The tick coalesces on a DISTINCT `#live` coordinator key (`live_key`), never the full-pass key — else a
`ScanCompleted` full pass coalescing into a tick's slot would silently downgrade to a scoped tick. Before running, it SKIPS
entirely if a full pass is running for the volume (the full pass covers the touched dirs). Progress honesty: a tick lights
the top-right indicator ONLY when its enrichable subset exceeds `LIVE_INDICATOR_THRESHOLD` (25) AND no full pass runs
(`tick_is_loud`); below that BOTH the progress sink and the terminal guard are suppressed together (a lone row-clearing
terminal on a silent tick would clear a visible full-pass row). A tick does NOT `mark_deferred_for_importance` on an
unscored volume — the full-pass bridge covers that, and marking would trigger a full re-walk on the next importance bump.
Tests: `scheduler/live.rs` (pure `tick_is_loud` + `live_debounce_wait` + distinct key), `enrich_tests` (the scoped walk,
the scoped GC vs the whole-store trap, the sibling re-qualify), `kick_tests` (the tick end to end over a
registered read pool: re-enrich-on-modify, below-threshold defer, exclusion veto, index-confirmed GC, unmount deletes
nothing).

### Read API + commands (tags, similarity, coverage)

`MediaIndex` gained `find_similar(source_path, k)` (source embedding → `top_k` over the resident cache, source
excluded), `dedup_clusters(threshold)`, and `images_with_tag(label, min_score)` (structured tag-score filter). New async
commands (all `spawn_blocking`, offline-capable, registered in BOTH `ipc.rs` + `ipc_collectors.rs`; regen bindings):
`media_index_find_similar`, `media_index_dedup_clusters`, `media_index_search_tag`, `media_index_covered_count`,
`media_index_set_importance_threshold`, `media_index_set_excluded_folder`. **Shapes for the frontend:**
`SimilarImage { path, score: f32 }`, `DedupCluster { paths: Vec<String> }`, `TagHit { path, score: f32 }`,
`CoveredCount { folders: u64, images: u64, pending: bool }`, `Tag { label, score: f32 }`, and the extended
`MediaIndexVolumeState { …, qualifying_count: Option<u64> }`. Live-apply for the threshold + exclude settings needs a
`settings-applier.ts` entry (the one FE handoff the backend can't do).

### Frontend surface (the settings slider, progress, and find-similar)

The user-facing surface lives in the Svelte frontend, not here; this section is the map so the two stay in sync.

- **The importance slider** — `src/lib/settings/sections/MediaIndexImportanceSlider.svelte`, rendered in the "Image
  search" card in `ImageSearchSection.svelte` when `mediaIndex.enabled` is on. It exposes five NAMED BUCKETS
  ("Only my most-used folders" → "Everywhere, even folders I rarely open") over the typed threshold; each bucket maps to
  a fixed threshold stop `[0.8, 0.6, 0.4, 0.2, 0.0]` (left → right, restrictive → broad). Dragging RIGHT indexes MORE (a
  LOWER threshold). The **default is the rightmost bucket, threshold `0.0`** — deliberately equal to the backend
  `DEFAULT_IMPORTANCE_THRESHOLD`, so the UI and an unpersisted (sparse) store agree without eagerly writing a default,
  and it's non-regressive (junk is floored out at any level regardless). The persisted value is the raw threshold;
  the slider maps it to the nearest bucket on load.
- **Persist + live-apply** follows the `mediaIndex.enabled` precedent, NOT the per-item delta path: the slider calls
  `setSetting('mediaIndex.importanceThreshold', threshold)` and the `settings-applier.ts` passthrough pushes it to
  `media_index_set_importance_threshold`. (Threshold is a scalar, so it fits the applier's key→value table — unlike the
  network/exclude delta setters, which co-locate persist+IPC in a prefs helper.)
- **Live honest preview** — the slider debounces `media_index_covered_count(threshold, enabledVolumeIds)` over the
  enabled volumes (local `root` + opted-in SMB; the backend drops non-opted-in SMB / MTP), rendering "Indexes about N
  images across M folders" with thousands separators + ICU plurals. `pending` ⇒ a "still scanning" caveat. A drag also
  shows the incremental delta vs the last settled level ("Adds about 12,000 images"), which folds into the baseline once
  the value settles (~900 ms). No ETA on the slider: the enriched-rate isn't exposed and a fixed per-image cost would be
  dishonest across HEIC/RAW/network, so counts stand alone.
- **Honest per-volume progress** reads `qualifying_count` from `media_index_volume_state`: the local disk line lives in
  the slider component, the network lines in `MediaIndexNetworkVolumes.svelte`, both showing "N of M images indexed" (or
  "Counting images…" while `qualifying_count` is `null`).
- **Find similar** — `ImageSearchResults.svelte` grows a per-tile "Find similar images" action that re-queries the grid
  via `media_index_find_similar` from that tile's STORED (index-relative) path (NOT the resolved OS path — the command
  keys on the stored path), showing a "Similar to <name>" header with a back button. A new query exits similar mode.
- **Tags need no separate UI**: tag labels fold into `media_ocr` (`source='tag'`), so the existing OCR keyword search
  already matches tag words and shows them in the snippet. `OcrHit` carries no `source`, so the grid can't label a hit
  as "matched a tag" without a backend field — deferred, not needed now.

## CLIP semantic search (M3)

Natural-language text→image search ("beach sunset" → the photo). CLIP maps images and text into ONE shared 512-d vector
space, so a typed query is encoded to a vector and cosine-matched against stored image embeddings. Everything lives under
`clip/`; the enrichment + query plumbing rides the existing subsystem unchanged.

### The model (evidence-anchored)

- **OpenAI CLIP ViT-B/32**, HF `openai/clip-vit-base-patch32`, **MIT-licensed** weights (a commercial product can ship
  them; Apple's MobileCLIP is research-only and can't — `docs/notes/clip-coreml-rust-spike.md`). Embedding dim 512, image
  224×224, text context 77.
- **Two Core ML `.mlpackage` towers** pinned in `clip/install.rs` (`CLIP_TOWERS`): the **image tower is 8-bit k-means
  palettized** (M5b, 2026-07-23), the **text tower stays fp**. Image ~83 MB (cosine min 0.9988 / mean 0.9995 vs torch
  fp32 over a 50-image fixture), text ~184 MB (cosine 1.0000), combined ~267 MB (down from ~392 MB non-palettized). The
  text tower stays fp because its 8-bit Core ML inference is all-NaN; 6-bit on the image tower falls below the 0.99 gate
  (min 0.957), so 8-bit is the floor. Per-variant numbers: the plan's M5b status (`docs/specs/resource-use-plan.md`).
- **Conversion is an out-of-tree dev script** (`apps/desktop/scripts/convert-clip-model/`), NEVER run by CI/pnpm: a
  throwaway `uv` venv (Python 3.11–3.12; coremltools/torch have no cp314 wheels), pinned `requirements.txt`. It bakes
  CLIP's per-channel `(x-mean)/std` normalization INTO the image model and prints each zip's SHA-256 + size + the
  David-upload handoff. `reference-tokenization.json` + `reference-vectors.json` are checked in (they back the Rust tests).
- **Both towers feed an `MLMultiArray`** (the exact path the spike proved), NOT a Core ML `ImageType` — a deliberate
  deviation that drops the CVPixelBuffer/MLImageConstraint FFI surface. So the image tower takes a float `[1,3,224,224]`
  CHW `[0,1]` tensor; the Rust side resizes + center-crops the decoded CGImage to 224 and divides by 255
  (`clip_pixels_from_cgimage` in `backend/vision`), and the model bakes the normalization.
- **Models are hosted on the public Hugging Face repo `veszelovszki/cmdr-clip-vit-b32-coreml`** (uploads only with
  David's explicit approval; `hf` CLI + `secret HF_TOKEN`): the pinned `url` in `install.rs` must serve the exact pinned
  bytes, else the checksum-verified download fails and the feature stays honestly gated off. The `resolve` URLs redirect
  to a CDN (reqwest follows) and support Range resume (verified 2026-07-16).

### Two vector spaces, two-part staleness

CLIP's space is DIFFERENT from the Vision feature print, so its embeddings live in a SEPARATE `media_clip_embedding`
table (schema v3 added it + the `media_status.clip_stamp` column). NEVER cosine-compare across the two — mixing them
would silently rank across incompatible spaces.

Staleness is two-part (`store::needs_enrichment` for Vision by `engine_version`; `store::needs_clip` for CLIP by
`clip_stamp`), decoupled on purpose (plan M3 Q5): installing/upgrading the CLIP model re-embeds CLIP for every image
WITHOUT re-running OCR/tags for everyone, and a Vision engine bump re-runs OCR/tags WITHOUT re-embedding CLIP.
`clip_stamp` is `clip;model={id};os={major.minor.patch}` (the OS component re-embeds after an upgrade, which recompiles
`.mlmodelc` and can drift ANE output); `None` when no model is installed ⇒ CLIP is never attempted.

### One decode, two writer paths

The enrich core (`enrich_and_gc_scoped`, and the network core) computes `want_vision || want_clip` per image and calls
`backend.analyze_media(input, want_vision, want_clip)` — ONE decode runs the requested side(s). The macOS backend decodes
via ImageIO, runs the Vision requests when `want_vision`, and (when `want_clip`) resizes/center-crops the same decode to
224 and hands the pixel buffer to the CLIP worker thread. Persistence is two INDEPENDENT writer messages (`apply_media_upsert`):
`upsert` writes the Vision row (identity + `engine_version` + OCR/tags/feature-print); `upsert_clip` stamps `clip_stamp`
+ replaces `media_clip_embedding`, touching NO Vision column. A CLIP encode that can't run yet (model still loading) yields
`clip: None`, so the pass leaves `clip_stamp` unstamped and retries next pass — it never fails the whole analysis on a
transient CLIP miss. The `clip_stamp` reaches the passes via `EnrichGates.clip_stamp` / `NetworkEnrichCtx.clip_stamp`,
read once per pass from `clip::current_stamp(data_dir)`. The whole-store `enrich_and_gc` wrapper is Vision-only (CLIP-agnostic,
`clip_stamp: None`) and test-only now; production reaches the scoped core directly with the installed stamp.

### The semantic-search on/off gate + delete-model

Semantic search is a real user toggle (`gate::semantic_search_enabled`, an atomic ON by default, seeded from the FE-owned
`mediaIndex.semanticSearch.enabled` at startup and live-applied by `media_index_set_semantic_search_enabled` in
`commands/policy.rs`). Downloading the model is no longer the de-facto opt-in — the toggle is.

**One atomic, both sides.** The gate is enforced at exactly two seams so read and write can't disagree:

- **Read:** `media_index_search_semantic` short-circuits to `[]` when the atomic is off (beside the master-toggle and
  empty-query checks).
- **CLIP write:** `clip::current_stamp` returns `None` when the atomic is off. Because `needs_clip(_, None)` is always
  false, EVERY pass type (full `run_pass_blocking`, network `run_network_pass_blocking`, live tick) computes `want_clip =
  false` and embeds no CLIP — without touching the per-pass `want_clip` line. This is the SINGLE CLIP-write seam; ❌ don't
  re-gate `want_clip` per pass. Turning off mid-pass stops new CLIP work at the next image (the pass reads the stamp once
  per pass, so an in-flight pass finishes its current image's already-decided CLIP work; a `should_stop`-style per-image
  re-read isn't needed because turning off is non-destructive).

Enabling the toggle while a model is installed makes every image CLIP-stale again, so the command kicks the ready passes
(guarded on `is_installed`, so with no model it's a no-op — nothing to embed). Disabling kicks nothing and deletes
nothing: existing embeddings stay searchable until re-enabled or the model is deleted. Turning off ≠ erase.

**Delete model (`media_index_delete_clip_model`).** The explicit reclaim: `MediaScheduler::delete_clip_model` removes the
shared on-disk `clip-model` dir (both towers), then, for EVERY volume with a `media-{id}.db` (mounted or not —
`media_volume_ids` reads the data dir, so an unmounted NAS's embeddings are reclaimed too), prunes its
`media_clip_embedding` rows via the writer's `prune_all_clip`, `VACUUM`s, and drops the resident CLIP vector cache.
`prune_all_clip` deletes every embedding AND resets every `media_status.clip_stamp` to `''` in one transaction —
resetting the stamp is what makes a later re-download re-embed (the row goes CLIP-stale again against the reinstalled
stamp). Vision data (status/OCR/tags/feature print) is untouched, and CLIP embeddings aren't part of the `accounted`
aggregate (that counts `media_status` rows), so no aggregate delta. After the delete, `media_index_clip_model_status`
reads `installed: false`, so the UI returns to the download affordance. Pinned by
`writer::tests::prune_all_clip_drops_embeddings_resets_stamps_and_keeps_vision` and
`enrich_tests::delete_clip_model_removes_the_model_and_every_volumes_embeddings`.

### The Core ML towers + worker thread (`clip/macos.rs`)

Mirrors the Vision backend's threading discipline: `MLModel` is `!Send` and a synchronous ANE predict is an XPC round-trip
that can overrun a small stack, so ONE dedicated 8 MB-stack `clip-worker` thread owns both loaded towers and SERIALIZES
every predict (Apple's pooled-inference recommendation). `encode_text` (query-time) and `encode_image` (from the Vision
worker) both send a job to it and block for the reply, so no `!Send` object crosses a boundary — only the input ids /
pixel `Vec` in and the embedding `Vec<f32>` out. `.mlpackage` is compiled to `.mlmodelc` on-device at first load
(`compileModelAtURL:error:`) and the compiled bundle is cached beside the model so later launches skip the 1–2 s compile;
after a verified compile the `.mlpackage` source is reclaimed (§ Model install — the M5a package reclaim). Every `unsafe`
block carries a per-site `// SAFETY:` (the objc2-core-ml `MLMultiArray` fills/reads via `dataPointer`, the
`MLDictionaryFeatureProvider` build, the CoreGraphics `CGBitmapContextCreate` render). The tokenizer (`clip/tokenizer.rs`,
`instant-clip-tokenizer`) produces the fixed `[1,77]` int32 sequence (`[BOS] content [EOS]`, EOS-padded), pinned bit-exact
to the HuggingFace reference.

### Model install (`clip/install.rs`, plan Decision 9)

New code reusing only `ai::download::download_file` (the resumable HTTP GET). Distinct from the GGUF two-flag gate: Core ML
models are `.mlpackage` DIRECTORY bundles (zipped), so this adds a zip extractor (with a zip-slip guard) and — unlike
`ai/`'s size-only check — a **SHA-256 verify BEFORE unpacking**. A truncated/tampered download never reaches the extractor,
so a half-model can never load and mis-embed (data safety, `verify_checksum` red→green tests). `installed_stamp` builds
the `clip_stamp`.

**The M5a package reclaim (plan M5a).** The model dir was 1.1 GB because it kept BOTH the ~550 MB combined downloaded
`.mlpackage` sources and the compiled `.mlmodelc`. Now, on the `clip-worker` thread's first load, once both towers load
AND a zero-input encode is sane (512-d, all-finite — `verify_sane`, guarding against a NaN-emitting model), each
`.mlpackage` source is deleted (`reclaim_source_package`), keeping only the compiled model (~350 MB dir, faster first
load). **Tradeoff:** the `.mlmodelc` is OS-version-specific, so an OS upgrade can invalidate it, and with the source gone
we can't recompile locally. `load_tower` handles this: it prefers the cached `.mlmodelc`; if it won't load it drops the
stale compiled and, if a `.mlpackage` is still present, recompiles from it; if NEITHER a loadable compiled model nor a
source remains, it returns `NotAvailable` having deleted the stale compiled — so `is_installed` (now **`.mlpackage` OR
`.mlmodelc` present per tower**) flips to `false` and the standard `media_index_download_clip_model` flow refetches the
pinned zip (same sha contract). A rare ~200 MB re-download vs ~550 MB saved on every launch, and never a crash or a
silently-dead feature. The filesystem decisions (`is_installed`, `reclaim_source_package`, `drop_compiled`) are unit
tested; the FFI compile/load around them isn't (needs Core ML + the real model).

### The query path

`media_index_search_semantic(volume_id, query, limit)` (IPC, registered in both `ipc.rs` + `ipc_collectors.rs`) runs OFF
the IPC thread (`spawn_blocking`): tokenize + warm-text-tower encode (`clip::encode_text_query`, which hops to the CLIP
worker) → `MediaIndex::search_semantic(query_vec, limit)` brute-force top-k over a SECOND resident CLIP cache
(`vector::cache::get_or_load_clip`, keyed `(db_path, EmbeddingTable::Clip)`, invalidated per completed pass, dropped by the
memory watchdog with the feature-print cache). The read API takes the already-encoded query VECTOR, so it's a pure vector
query testable with deterministic vectors; the command owns the encode. `[]` (never an error) when indexing is off, no
model is installed, or the volume has no CLIP embeddings — so the UI voices coverage. Answers offline from `media.db`.

**Latency:** the text tower is kept warm (a cold Core ML load is 1–2 s; a warm encode ~2 ms — spike numbers); the vector
top-k is brute force below ~50k stored vectors and the per-volume ANN index at or above it (§ ANN vector search — the
engine decision went to `usearch` by a measured spike; `sqlite-vec` was disqualified as not actually ANN).

### Frontend

- **`search_semantic` is the PRIMARY text→image signal** in the Search dialog's image grid (`ImageSearchResults.svelte`):
  each keystroke runs semantic + OCR in parallel, semantic hits lead (snippet-less tiles with a "matched description"
  reason via `search.imageResults.matchedDescription`), then OCR keyword hits not already shown (dedup by path). With no
  model, semantic returns `[]` and the grid degrades to OCR-only. The three test mocks
  (`ImageSearchResults.{gating,a11y}.test.ts`, `SearchDialog.svelte.test.ts`) stub `mediaIndexSearchSemantic → []`.
- **Settings download** (`MediaIndexClipModel.svelte` in the Image search card): self-gates on Apple Silicon
  (`is_local_ai_supported`), shows install state + a "Download model (~X MB)" button (honest size from
  `media_index_clip_model_status`), and downloads/installs via `media_index_download_clip_model`, which kicks a pass so
  already-enriched images gain CLIP embeddings (like a threshold decrease). "Coming soon" until the artifact is published.

## ANN vector search (`ann/`, plan M6)

CLIP text→image search over a per-volume `usearch` HNSW index, so semantic search stays low-ms as a corpus scales past
what the exact resident-f16 scan can serve. Engine chosen by a measured spike
(`docs/notes/ann-vector-search-spike-2026-07-24.md`): at 200k vectors usearch answers in 0.30 ms p50 at 0.994 recall@10
from an mmap-backed view, where `sqlite-vec` 0.1.9 turned out to be an exact linear scan (141 ms p50, not ANN at all).
Files: `media-{id}.clip.usearch` beside the media DB, plus a JSON sidecar (`….usearch.meta`: format version, model id,
dims, rows, SHA-256 of the index file) and a transient dirty marker (`….usearch.dirty`). The module is
dimension-generic: `AnnSpace` names the space (table + file suffix + model identity), so the 768-d Vision feature print
(similar-images/dedup) adopts ANN later by adding a variant — deliberately NOT wired now.

**Decision: the writer thread owns incremental index mutations (single-writer discipline).** The `MediaWriter` loop
buffers an `AnnOp` (upsert/remove, keyed by the `media_file` id as `u64`) beside every CLIP write/delete it commits, and
lands the batch via `flush_ann_index()` at exactly the seams that invalidate the resident vector cache (local/network
pass completion, live tick, reclaim prune, retro-delete), plus an in-writer auto-flush at 8,192 pending ops. usearch has
no in-place file mutation, so a flush loads the index to the heap, applies the ops in order (an upsert removes the key
first, so re-embeds overwrite), and saves temp+rename — a live mmap view keeps the old inode, and a crash never leaves a
torn file. The one writer-external mutator is the background rebuild, and its install serializes with flushes on a
per-file mutex (`ann::file_lock`); ops buffered while a rebuild snapshot was building re-apply idempotently on top of
it. Accepted cost: a flush's load+save is linear in index size (~235 MB of I/O and transient heap at 200k), paid once
per pass seam, not per image.

**Decision: crash detection is a dirty marker, not a row-count compare.** The writer creates the marker BEFORE the first
buffered op's DB write commits and the flush removes it after a successful save, so a session that dies with unflushed
ops leaves it behind and the next writer spawn wipes the index (the next query rebuilds from the DB, the truth). A
count-compare at query time would misread normal mid-pass write lag as corruption and rebuild-storm during enrichment.
Until the wipe, a lagging index only under-returns (missing newest vectors); it can never return wrong paths, because
hits resolve ids through the DB (below).

**Decision: verify a SHA-256 of the index file before EVERY load/view.** usearch trusts the bytes it maps: viewing a
garbage file SIGSEGVs (observed in tests), so corruption cannot be caught at open time by the engine itself. The sidecar
carries the checksum from the last save; a mismatch fails closed (`AnnError::Corrupt` → exact-scan fallback + background
rebuild). Cost: one streamed hash per open/flush (~0.1 s per GB), paid at pass seams, not per query.

**Decision: brute force below `ANN_MIN_VECTORS` = 50,000 vectors.** Below it the exact scan is ≤ ~19 ms from a ≤ ~50 MB
resident cache (74 ms/205 MB measured at 200k, linear) — exact, with no index file to build, maintain, or store. At/above
it, latency and RAM grow linearly while HNSW stays sub-ms over an evictable mmap. No index file is ever created below
the threshold (a flush with no index drops its ops); crossing it makes the first query kick the rebuild.

**Decision: over-fetch 4× + exact re-rank, so ORDERING stays exact-quality.** The ANN route fetches `k × 4` candidates,
reads each candidate's stored f16 vector and CURRENT path from the DB, re-scores with the same `cosine_f16` the exact
scan uses, and returns the top `k`. HNSW recall dips as corpora grow (0.895–0.982 at 1M depending on `expansion_search`
— spike); re-ranking the over-fetched set exactly restores exact ordering for the k callers see, and the DB join both
follows renames (keys are stable `media_file` ids — a rename needs NO index touch, pinned by
`a_rename_touches_neither_the_index_nor_the_dirty_marker_and_hits_follow`) and silently drops ghost keys (a key whose
row is gone yields nothing). Measured on the real corpora below: the re-rank's read-and-rescore adds well under a
millisecond at k = 10.

**Rebuild** (`ann/rebuild.rs`): background thread, single-flight per index file, streams `(file_id, f16)` rows from the
DB (no whole-corpus `Vec`), single-threaded on purpose (spike: 71 s per 200k; usearch `add` is thread-safe, so a
parallel build is a future lever), polls the memory watchdog's cancel every 1,024 adds (deliberately NOT
`gate::should_stop`: queries can't kick a rebuild while the master toggle is off, and the watchdog cancel is the
"release resources now" signal that matters). Triggered by the query-side route whenever the index is missing, corrupt,
or sidecar-incompatible; search answers exactly via the fallback until it lands. `expansion_search` scales stepwise with
corpus size (`expansion_search_for`: 128 ≤ 300k, 256 ≤ 700k, 512 beyond — the spike's recall-vs-ef curve).

**Versioning + lifecycle:** the sidecar pins `ANN_FORMAT_VERSION` and the space's model id (`CLIP_MODEL_ID`, no OS
component — OS-drift re-embeds flow through writer upserts row by row), so a model bump or index-format change reads as
`MetaIncompatible` and rebuilds. The index is versioned independently of `SCHEMA_VERSION`, and the disposable-cache
paths all take it along: `MediaStore::delete_and_recreate` (schema wipe), `PruneAllClip` (delete model), `PurgeVolume`,
and the crashed-session wipe. `ann::cache` holds the query-side route + warm view per volume, invalidated through
`vector::cache::invalidate` — the ONE choke point for "this volume's derived query caches changed" — and dropped by the
memory-watchdog stop hook with the resident caches.

**Measured on real embeddings (M6 verification, 2026-07-24, M3 Max):** see the harness
(`ann/tests.rs::real_corpus_recall_and_latency`, run against copies of the real `media.db`s) — recall@10 and
before/after latency numbers are recorded in the plan's M6 status (`docs/specs/resource-use-plan.md`).

## What's left for later

- **Per-folder COUNTS now exist** (`coverage.rs`'s incremental `accounted` aggregate + subtree rollups): the honest
  `eligible` / `accounted` per folder feed `media_index_file_status` / `media_index_folder_coverage`, which the file- and
  folder-icon overlays consume (`file-explorer/selection/DETAILS.md` § Image-index overlay) and the drive dot rolls up per
  volume. Accepted staleness caveat: a `done` row whose file changed since indexing still counts as `accounted` until
  re-enriched, so a folder/drive can briefly read complete while a changed file awaits re-work. Excluding stale rows would
  need a per-row `(mtime, size)` compare against the live index; out of scope.
- **CLIP model size:** ~267 MB combined — the image tower is 8-bit palettized (M5b, 2026-07-23; cosine 0.9995, ~83 MB),
  the text tower stays fp (~184 MB; its 8-bit inference NaNs). Down from ~392 MB non-palettized. Numbers: the plan's M5b
  status (`docs/specs/resource-use-plan.md`) + `clip/install.rs`.
- **Later:** faces (detect/embed/cluster/name), the durable identity store, and LLM captions.

## Testing

Most tests are FFI-free and registry-free. Pure: the predicate (`predicate.rs`), the staleness key (`store/tests.rs`),
`gc_targets`, `build_ocr_match_query`, the coalescer (`scheduler/coalescing_tests.rs`), the command's limit clamp
(`commands/tests.rs`). Over the fake backend + a synthetic index: the walk, the enrich pass, deletion-driven GC, the
throttle/cancel decision, the edge-triggered `Completed` consumption (`scheduler/enrich_tests.rs`), and the OCR search +
offline-after-unmount round-trip (`read/tests.rs`), plus the FTS5 availability smoke. **macOS-gated real FFI**
(`backend/vision/tests.rs`, the module is macOS-only so it can't run off-macOS): real Vision OCR reads the known words
off the committed fixture, and hostile inputs (non-image, empty, missing) each return a typed `VisionError` with no
panic. The async wire-up (`ready_volumes_with_kind` sweep → `wire_volume` → `run_pass_blocking`) is covered indirectly by
the reactive pieces (bus-edge consumption + coalescer + the enrich core); a full end-to-end async test needs the
process-global index registry and is deferred to the E2E slice.

**Tags + similarity tests** (all real red→green on the pure/risky bits): cosine + `top_k` ranking + source exclusion + dedup grouping
(`vector/tests.rs`, pure); tags/embedding round-trip + tag-score filtering + the embedding codec + the
clear-on-re-enrichment invariant (`store/tests.rs`); `prioritized` ordering + the scheduler DEFERS a below-threshold
folder + ENRICHES an overridden one, both keeping deferred rows for GC (`scheduler/enrich_tests.rs`); the covered-count
arithmetic over a synthetic counts+scores map (`coverage.rs`); the fake backend's deterministic tags/feature-prints
(`backend/fake.rs`). **macOS-gated real FFI** (`backend/vision/tests.rs`): `analyze` returns real OCR + well-formed tags
+ a stable-length feature print off the fixture, and a real feature print's self-cosine is ~1.0.

**Privacy retro-delete tests (all real red→green — deletion is data-safety-critical):** the writer prune primitives
(`writer.rs`) — `prune_under_folder` deletes rows at or under a folder across ALL four tables and only those,
trailing-slash-safe (`/Photos2` survives pruning `/Photos`); `prune_paths` deletes only the explicit set; prune + VACUUM
round-trips. The live privacy veto (`scheduler/enrich_tests.rs` + `network/tests.rs`): exclusion beats an override-covered
image, and an exclusion landing mid-`analyze` (a stateful veto flipping false → true across its two calls) persists NO
row — both the local and network cores. The OS-folder → index-prefix mapping is the inverse of `os_join`
(`network/fetch.rs`). The scheduler retro-delete (`scheduler/kick_tests.rs`) prunes a local folder and skips a volume the
folder isn't under, and maps a network folder into the volume's index space.
