# Media index subsystem — details

Image-ML enrichment: makes a volume's images searchable by their content. Full design and milestone plan:
[`docs/specs/media-ml-index-plan.md`](../../../../../docs/specs/media-ml-index-plan.md). This doc covers what the OCR slice shipped,
the port-from-`importance/` rationale, the GC safety argument, and the schema.

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

## The lifecycle bus

`media_index`'s scheduler subscribes to `indexing/lifecycle_bus.rs` exactly as `importance`'s does — its OWN `start()`
mirrors the ordering (subscribe to registrations → sweep `ready_volumes_with_kind()` → wire per-volume subscriptions).
It can't piggyback `importance`'s subscription; because `app.manage` is keyed by type, an `Arc<MediaScheduler>` coexists
fine alongside `importance`'s scheduler. The bus mechanism (watch vs broadcast, late-subscriber replay, the registration
bus, why the sender outlives the registry) is documented once in [`indexing/DETAILS.md`](../indexing/DETAILS.md) — not
re-documented here (single-source).

`wire_volume` routes by typed kind: LOCAL enriches by default (when the master toggle is on); an opted-in SMB volume
runs the conservative network pass (§ "Network-volume enrichment"); MTP is NEVER background-swept. Both local and
SMB subscribe to the SAME bus the same way; only which pass method runs differs. The opt-in is checked INSIDE the network
pass, so flipping it on takes effect on the next scan completion (and the opt-in command kicks an immediate pass).

## The GC safety argument (data-safety)

GC deletes a stored `media.db` row when its source path no longer appears as a qualifying image in the CURRENT index
walk. The safety comes entirely from **when** a pass (and thus its GC) runs, not from generation arithmetic:

- A pass runs ONLY on a `Completed` bus edge or the Fresh registry sweep. The `Completed` signal fires AFTER the index
  writer flushes the truncate + repopulate (`indexing/scan_completion.rs`), so a triggered sweep always observes a
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

## Network-volume enrichment

Making an opted-in NAS's images searchable by content is the headline use case (`/Volumes/naspi` over SMB). This is the
ONE part of the plan with no `importance/` sibling to copy: `importance` follows a hard rule ("never a filesystem syscall
against an SMB/MTP mount"), but media enrichment MUST read image bytes off the wire. Everything lives under `network/`;
the scheduler routes SMB volumes to it (§ "The lifecycle bus"). Scoped to OCR (it inherits the OCR slice's Vision backend — no new
models).

### The byte-fetch decision (`network/fetch.rs`) — why the OS mount, not a direct smb2 client

**Decision: read image bytes via the OS mount path (`/Volumes/<share>/…`) with plain `std::fs`, bounded by a timeout —
the SAME transport the file viewer already uses for SMB image preview** (`file_viewer/media_protocol.rs` reads bytes with
`std::fs::File::open` on the mount path + its own `spawn_blocking` + timeout). We do NOT stand up a parallel direct-`smb2`
client (`Volume::open_read_stream`). Why:

- The viewer's OS-mount read is the ONE existing byte-read path for images over SMB; reusing it keeps a single transport
  and matches what the local pass already does (`std::fs::read`).
- The direct-`smb2` `open_read_stream` is the chunked large-transfer/copy path; an OCR fetch wants the whole (bounded)
  compressed file, which a single `std::fs::read` gives simply.
- The whole use case is an OS-mounted NAS. The mount root comes from `VolumeManager::get(volume_id).root()` — the same
  source `indexing::routing::index_read_path` uses for its read-side mount strip.

**Path mapping.** An SMB index's `ROOT_ID` is the mount root, so `walk_image_entries` reconstructs MOUNT-RELATIVE paths
(`/DCIM/x.jpg`). `os_join(mount_root, rel)` prepends the mount root to reach the real file (`/Volumes/naspi/DCIM/x.jpg`);
for the `root`/local volume the mount root is `/`, so the path passes through unchanged. The stored `media.db` row keeps
the index-relative identity (matching the index + GC set); the network-enrichment UI reconstructs the display/open path via the mount
root.

**Non-blocking discipline (the crux).** A network `std::fs::read` can block indefinitely on a hung mount. So `FsByteFetcher`
runs the read on a throwaway thread and waits with `recv_timeout`; a timeout returns `FetchError::Disconnected` (pause),
never a wedge. Critically, the fetch happens in the ENRICH layer, not on the serialized Vision OCR worker thread — the
backend receives the already-fetched bytes via `ImageInput.bytes` (`Some` = network, `None` = local read-it-yourself), so a
hung mount can never stall OCR of other (local) volumes. Failures classify by I/O error KIND (a typed errno, not a message
match): `NotFound` ⇒ skip (a vanished source; GC collects it), else ⇒ `Disconnected` (pause). A `MAX_FETCH_BYTES` cap skips
a pathological file rather than OOMing.

### The conservative-fetch policy with teeth (`network/policy.rs`)

Typed knobs (`ConservativeFetchPolicy`), each a real gate, not a comment:

- **Idle-gated.** The pass proceeds only while the app has been idle for `idle_threshold` (default 5 s). The idle signal
  is NEW work — there is no foreground/idle signal in `indexing/` (its only `Idle` is an indexing work-state). `foreground.rs`
  is a single process-global "last foreground activity" timestamp; the hot foreground filesystem IPC (directory listing =
  every navigation) calls `note_foreground_activity`, and the pure `is_idle(now, last, threshold)` is unit-tested over a
  fake clock. A non-idle app pauses the pass (`PauseReason::NotIdle`) so a NAS is never dragged over the wire while the
  user browses.
- **Bandwidth-bounded.** After each image, `throttle_delay(bytes, max_bytes_per_sec)` sleeps so the sustained fetch rate
  stays under the cap (default 8 MB/s). Pure and tested; it deliberately over-throttles slightly (ignores OCR time) — the
  conservative direction.
- **Bounded concurrency.** `max_concurrency` (default 1); the pass fetches serially today, so it's honestly 1, with the
  field bounding any future parallel fetch.
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

The backend provides three setters + the extended state:

- `media_index_set_network_volume_enabled(volume_id, enabled)` — the per-volume SMB opt-in (live-applied; enabling kicks a
  pass).
- `media_index_set_always_index_volume(volume_id, always)` / `media_index_set_always_index_folder(folder, always)` — the
  overrides (live-applied; the folder setter does NOT kick a pass, next scan picks it up).
- `media_index_volume_state` extended with `network_opt_in`, `always_indexed`, `paused` (the "paused, resumes on
  reconnect" honesty).

**The FE surface (shipped).** The opt-in + volume override live in Settings > Behavior > File system watching >
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

**Per-folder override — FE trigger deferred.** The `media_index_set_always_index_folder` command +
`mediaIndex.alwaysIndexFolders` setting are ready, but no FE control sets them yet: the natural trigger is a folder
right-click action, and the file context menu is a NATIVE (Rust) menu (`show_file_context_menu`), so adding the item +
its menu-event handler is a small backend follow-up rather than an FE change.

## Schema (`store/`)

`SCHEMA_VERSION` is a disposable-cache version: a mismatch delete-and-recreates `media-{volume_id}.db`. It's now `2`
(the tag + embedding tables and the FTS `source` column arrived with tags and embeddings). Objects:

- `media_status` — `WITHOUT ROWID`, `path TEXT PRIMARY KEY COLLATE platform_case`; `mtime`, `size` (the `(path, mtime,
  size)` staleness key); `media_kind` + `state` (typed TEXT tokens, `sqlite3`-inspectable, parsed back to typed enums —
  `no-string-matching`); `engine_version` (the combined **analyze provenance stamp** (§ The analyze provenance stamp) so an OS upgrade to the OCR
  engine, tag taxonomy, or feature-print model re-runs analysis even on an unchanged file — data-COVERAGE, not
  data-safety, since the derived data is disposable).
- `media_ocr` — a **standalone** FTS5 table (`path UNINDEXED, source UNINDEXED, text`). Not external-content:
  `agent/store`'s `messages_fts` points at an integer `messages.id`, but `media_status` is path-keyed and `WITHOUT
  ROWID`, so there's no integer rowid to hang external content off. Standalone keeps enrichment and GC a simple `WHERE
  path = ?` delete with no trigger machinery to desync. It holds up to two rows per path: the OCR text (`source='ocr'`)
  and the space-joined tag labels (`source='tag'`), so a keyword search matches **tags alongside OCR**. Created via
  `CREATE VIRTUAL TABLE … USING fts5`, which doubles as the FTS5 availability guard (a `bundled` build without FTS5 fails
  there — Decision 2's build-flag worry is closed, `agent/store` proves it).
- `media_tags` — `(path COLLATE platform_case, label, score)` with an index on `path` and on `label`: the STRUCTURED
  tags for tag-score filtering (`images_with_tag(label, min_score)`), distinct from the folded FTS keyword index above.
- `media_embedding` — `WITHOUT ROWID`, `(path PRIMARY KEY COLLATE platform_case, dims, vector BLOB)`: the image
  feature-print embedding as a little-endian `f32` BLOB (`encode_embedding`/`decode_embedding`; `dims` = element count).
  The vector store's load source (§ The vector store + resident cache).
- `meta` — `schema_version` only.

The `needs_enrichment` staleness predicate is `(path, mtime, size)` + the analyze stamp: stale when there's no row, or
when `(mtime, size)` changed, or when the stamp changed. State is deliberately excluded from the key so a failed file
isn't re-hammered every completed scan; a real file change re-tries it. A successful `upsert` writes `media_status` +
the OCR/tag FTS rows + `media_tags` + `media_embedding` in ONE transaction (clearing each prior row first, so a
re-enrichment leaves nothing stale); a failure clears them all and records only the `Failed` status.

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
a small worker stack — the same hazard as calling AppKit off rayon (`src-tauri/CLAUDE.md`). So the backend owns ONE
dedicated OS thread with an 8 MB stack; `ocr` dispatches each image to it over a channel and blocks for the reply. One
thread also SERIALIZES Vision calls (Apple's recommendation for pooled inference) and confines every `Retained`/
`CFRetained` object to that thread (nothing `!Send` crosses a boundary — only the path `String` in and `OcrResult`
out). Each job runs inside `objc2::rc::autoreleasepool`, so framework temporaries free per image, not per pass.

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

The master toggle `mediaIndex.enabled` renders in Settings > Behavior > File system
watching (a dedicated "Image search" card, `FileSystemWatchingSection.svelte`), off by
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
project adopted only if a library outgrows brute force, behind this same `VectorStore` trait). `cosine` guards
degenerate inputs (zero magnitude / length mismatch → `0.0`, never `NaN`). `BruteForceVectorStore::top_k` linearly
ranks by cosine (source excluded, ties by path); `dedup_clusters` groups near-duplicates by single-linkage union-find
over pairs at/above a cosine threshold (default 0.9), returning clusters of two or more.

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

### Defer-until-scored (M1)

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

The residual risk is made VISIBLE, never silent: M2 guarantees the recompute *trigger*, not its *success* (a read-pool
or write error leaves generation 0 with no notify). Under defer-until-scored that would mean image indexing silently
never starts. So `media_index_volume_state` exposes `waiting_for_importance` (enabled + index ready + not scored), and the
settings slider voices it ("Working out which folders matter…") REPLACING the generic covered-count spinner — one honest
line for one wait, never two spinners. There is deliberately NO silent fallback to enrich-all on timeout: a persistently
failing recompute is an importance bug to surface, not to paper over.

### The importance "has scored" detection (M2)

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
folder is an O(entries) index walk, so it's cached per volume (`coverage::get_or_build`, a `folder → count` map,
invalidated on each pass) and the threshold is applied cheaply by intersecting with `above_threshold` — a debounced drag
only re-runs the cheap importance read + `covered_for_volume` (pure, unit-tested). `pending` is `true` when any enabled
requested volume isn't ready (still scanning / not yet scored), so the UI voices "naspi still scanning" rather than a
confident wrong number. `media_index_volume_state` gained `qualifying_count: Option<u64>` (the honest denominator for
"12,000 of 38,900 images", `None` when offline/scanning); ETA math lives UI-side off `(enriched_count, qualifying_count)`.

### Per-folder photo-search exclude + the privacy retro-delete (M3)

`network::config` gained `excluded_folders` (seeded from `mediaIndex.excludedFolders`, live-applied by
`media_index_set_excluded_folder`): an image at or under an excluded folder never enriches, a HARD veto that beats any
"always index" override — the privacy complement to the opt-in (protect a high-importance `~/Documents/IDs` the
threshold alone can't).

Excluding a folder does more than veto the future: it **retro-deletes** the folder's already-indexed rows so extracted
OCR text stops being searchable at once (privacy is a hard requirement, not "eventually on the next GC"). The pieces:

- **Why it's a new deletion path (vs the GC-safety doctrine).** GC's safety comes from *when* it runs (only a
  `Completed` edge, tree whole — § The GC safety argument). The retro-delete is USER-EXPLICIT and derives ONLY from
  settings state (the exclusion the user just set), never scan/bus/gate state, so it can't wipe live coverage by
  mistiming — it needs no edge. This is the same doctrine the reclaim prune (M4) rides. The slider stays forward-only;
  the ONLY row deletions are (a) vanished files via GC, (b) the reclaim prune, (c) this privacy retro-delete.
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

### Reclaim space (M4)

Lowering the importance slider is forward-only: it never deletes rows, so a drive indexed at a broad setting keeps that
coverage after the user narrows the setting (the GC `current` set stays the full walked image set — § The GC safety
argument). The reclaim UI surfaces that leftover coverage and offers to delete it. Like the privacy retro-delete, the
prune is USER-EXPLICIT and derives ONLY from settings state, so it needs no `Completed` edge — it's deletion path (b) of
the three the slider's forward-only contract allows.

- **One arithmetic source, or the numbers don't add up.** `MediaScheduler::stored_coverage(volume_id, mount_root,
  threshold)` computes THREE quantities from ONE pass so the reclaim preview, the prune, and M5's `keptCount` can never
  disagree: `surviving_stored` (stored rows inside coverage), `doomed_stored` (outside it — M4's "delete N" AND M5's
  `keptCount`, the SAME set), and `covered_qualifying` (drive-index qualifying images in covered folders — the slider
  preview's number, a DIFFERENT thing: it counts what WOULD be indexed, not what IS). It guarantees `total_stored =
  surviving_stored + doomed_stored`, and reuses the `coverage.rs` cache path for `covered_qualifying` (never a second
  derivation). It returns `None` when importance hasn't scored the volume (M2 makes that transient) — the partition
  can't be computed safely, so the command reports `pending` and the UI hides the reclaim line rather than proposing a
  destructive count off a lower bound.
- **The partition rule** (`coverage::partition_stored`, pure) reuses the SAME precedence enrichment does: a stored row
  survives when it's NOT under an excluded folder AND (covered by an "always index" override OR its parent folder scores
  at or above the threshold). Crucially it keys on score-MAP MEMBERSHIP, not a `>= 0.0` on a defaulted score: a folder
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
  the space-vs-reindex tradeoff — one narrative, composing with M5's kept-rows line, never two sentences in tension. A
  confirm dialog (recoverable, but re-reading costs time) precedes the prune; an honest toast reports the freed space.

### Progress events + vanished-file skip (M5)

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
core already handles a vanished source via `FetchError::NotFound` (same quiet skip). The too-small-image skip (M3) is a
sibling quiet case: it writes an empty `Done` row instead. Pinned by
`enrich_tests::a_vanished_image_still_completes_the_pass_at_done_equals_total` and
`enrichable_totals_excludes_deferred_and_excluded_images`.

### Threshold-aware volume state (M5)

`media_index_volume_state` gained `covered_qualifying_count` + `kept_count`, from `MediaScheduler::stored_coverage_counts`
— a counts-only sibling of the M4 `stored_coverage` that does NOT allocate the doomed-path `Vec` (the settings poll runs
it every few seconds). Both share the ONE canonical survival rule (`coverage::stored_row_survives`) and the `coverage`
cache, so they can never disagree with the reclaim preview. `covered_qualifying_count` drives the settings progress line
"N of M in your covered folders" (N = `enriched_count − kept_count`, capped); `kept_count` (= the M4 doomed count) drives
the quiet "K more indexed from broader settings, still searchable" line, gated by the SAME `shouldOfferReclaim` floor so
it never duplicates the reclaim offer. Both `None` when importance hasn't scored the volume.

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
  search" card in `FileSystemWatchingSection.svelte` when `mediaIndex.enabled` is on. It exposes five NAMED BUCKETS
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

## What's left for later

- **Per-folder "always index" UI (FE trigger):** the backend setter (`media_index_set_always_index_folder`) + the
  `mediaIndex.alwaysIndexFolders` setting are ready, but no FE control sets them yet. The natural trigger is a folder
  right-click action, the same native-menu shape the exclude trigger now uses (§ Per-folder photo-search exclude), so
  it's a small backend/menu follow-up.
- **Later:** CLIP text→image semantic search, the model-install path, faces (detect/embed/cluster/name), the durable
  identity store, and LLM captions.

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

**Privacy retro-delete tests (M3, all real red→green — deletion is data-safety-critical):** the writer prune primitives
(`writer.rs`) — `prune_under_folder` deletes rows at or under a folder across ALL four tables and only those,
trailing-slash-safe (`/Photos2` survives pruning `/Photos`); `prune_paths` deletes only the explicit set; prune + VACUUM
round-trips. The live privacy veto (`scheduler/enrich_tests.rs` + `network/tests.rs`): exclusion beats an override-covered
image, and an exclusion landing mid-`analyze` (a stateful veto flipping false → true across its two calls) persists NO
row — both the local and network cores. The OS-folder → index-prefix mapping is the inverse of `os_join`
(`network/fetch.rs`). The scheduler retro-delete (`scheduler/kick_tests.rs`) prunes a local folder and skips a volume the
folder isn't under, and maps a network folder into the volume's index space.
