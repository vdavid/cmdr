# Media index subsystem — details

Image-ML enrichment: makes a volume's images searchable by their content. Full design and milestone plan:
[`docs/specs/media-ml-index-plan.md`](../../../../../docs/specs/media-ml-index-plan.md). This doc covers what M1 shipped,
the port-from-`importance/` rationale, the GC safety argument, and the schema.

M1 (this slice) ships the plumbing + OCR-text search with **no model download and no vector math**: a per-volume
disposable `media.db`, a lifecycle-bus-driven scheduler, an OCR pipeline behind a fakeable `VisionBackend` seam,
deletion-driven GC, and the `MediaIndex` read API. It's **local-only** (SMB/MTP is M1.5) and **off by default**.

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
runs the conservative network pass (§ "Network-volume enrichment (M1.5)"); MTP is NEVER background-swept. Both local and
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

## Network-volume enrichment (M1.5)

Making an opted-in NAS's images searchable by content is the headline use case (`/Volumes/naspi` over SMB). This is the
ONE part of the plan with no `importance/` sibling to copy: `importance` follows a hard rule ("never a filesystem syscall
against an SMB/MTP mount"), but media enrichment MUST read image bytes off the wire. Everything lives under `network/`;
the scheduler routes SMB volumes to it (§ "The lifecycle bus"). Scoped to OCR (it inherits M1's Vision backend — no new
models).

### The byte-fetch decision (`network/fetch.rs`) — why the OS mount, not a direct smb2 client

**Decision: read image bytes via the OS mount path (`/Volumes/<share>/…`) with plain `std::fs`, bounded by a timeout —
the SAME transport the file viewer already uses for SMB image preview** (`file_viewer/media_protocol.rs` reads bytes with
`std::fs::File::open` on the mount path + its own `spawn_blocking` + timeout). We do NOT stand up a parallel direct-`smb2`
client (`Volume::open_read_stream`). Why:

- The viewer's OS-mount read is the ONE existing byte-read path for images over SMB; reusing it keeps a single transport
  and matches what the M1 local pass already does (`std::fs::read`).
- The direct-`smb2` `open_read_stream` is the chunked large-transfer/copy path; an OCR fetch wants the whole (bounded)
  compressed file, which a single `std::fs::read` gives simply.
- The whole use case is an OS-mounted NAS. The mount root comes from `VolumeManager::get(volume_id).root()` — the same
  source `indexing::routing::index_read_path` uses for its read-side mount strip.

**Path mapping.** An SMB index's `ROOT_ID` is the mount root, so `walk_image_entries` reconstructs MOUNT-RELATIVE paths
(`/DCIM/x.jpg`). `os_join(mount_root, rel)` prepends the mount root to reach the real file (`/Volumes/naspi/DCIM/x.jpg`);
for the `root`/local volume the mount root is `/`, so the path passes through unchanged. The stored `media.db` row keeps
the index-relative identity (matching the index + GC set); the M1.5b UI reconstructs the display/open path via the mount
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
slider is M2, so in M1.5 the production importance oracle yields `None` (defer) and **the override is the load-bearing
input**: only override-covered volumes/folders enrich. The gate seam keeps the M2 importance path drop-in.

**Storage: a settings-seeded global, not a fourth per-volume store.** The opt-in and overrides are user config (a handful
of volumes/folders), not per-image data, so they ride the sparse settings store (`mediaIndex.networkVolumes`,
`mediaIndex.alwaysIndexVolumes`, `mediaIndex.alwaysIndexFolders` — FE-owned) rather than a new SQLite DB with its own
writer thread (the standing-cost note already flags per-volume thread growth). The scheduler runs off the IPC thread and
consults `network::config` (a process-global `RwLock`) each pass, seeded from `load_settings` at startup and live-applied
through the `media_index_set_*` commands. Folder overrides store absolute OS-mount paths; `path_is_within` is a
trailing-slash-safe prefix so `/Photos2` isn't "within" `/Photos`.

*Non-load-bearing candidate (NOT built):* a photo-density importance input (a folder that's mostly images is likely an
archive regardless of visit count) could feed the M2 importance oracle. Deliberately deferred; the manual override is the
M1.5 mechanism.

### Resumability across unmount + the disconnect data-safety lines

A mid-pass unmount is not a crash and not a bad file. On `FetchError::Disconnected` the pass returns
`NetworkPassOutcome::Paused { reason: Disconnected }`: it flushes every completed row (they survive), writes NO `Failed`
row for the in-flight image, and does NOT GC. The scheduler marks the volume paused (`network::config::mark_paused`),
which the coverage signal surfaces; resume happens via the registration bus on remount (the next completed scan re-runs
the pass, skipping already-Done rows). This is distinct from the M1 `Failed` state, which is reserved for a genuinely bad
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

### Backend commands + typed state for the M1.5b UI

The backend provides three setters + the extended state:

- `media_index_set_network_volume_enabled(volume_id, enabled)` — the per-volume SMB opt-in (live-applied; enabling kicks a
  pass).
- `media_index_set_always_index_volume(volume_id, always)` / `media_index_set_always_index_folder(folder, always)` — the
  overrides (live-applied; the folder setter does NOT kick a pass, next scan picks it up).
- `media_index_volume_state` extended with `network_opt_in`, `always_indexed`, `paused` (the "paused, resumes on
  reconnect" honesty).

**The FE surface (shipped, M1.5b).** The opt-in + volume override live in Settings > Behavior > File system watching >
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
("turn on indexing for this drive" when not opted in, "disconnected, showing what's indexed" when paused). NOTE: the
whole-drive Search dialog currently targets only `ROOT_VOLUME_ID` (a local-index surface), so those network states are
reachable only once a caller points `ImageSearchResults` at a network volume — the component + path reconstruction are
ready; wiring the dialog to search a chosen network volume is a separate search-feature change (deferred, see the M1.5b
report).

**Per-folder override — FE trigger deferred.** The `media_index_set_always_index_folder` command +
`mediaIndex.alwaysIndexFolders` setting are ready, but no FE control sets them yet: the natural trigger is a folder
right-click action, and the file context menu is a NATIVE (Rust) menu (`show_file_context_menu`), so adding the item +
its menu-event handler is a small backend follow-up rather than an FE change.

## Schema (`store/`)

`SCHEMA_VERSION` is a disposable-cache version: a mismatch delete-and-recreates `media-{volume_id}.db`. It's now `2`
(M2 added the tag + embedding tables and the FTS `source` column). Objects:

- `media_status` — `WITHOUT ROWID`, `path TEXT PRIMARY KEY COLLATE platform_case`; `mtime`, `size` (the `(path, mtime,
  size)` staleness key); `media_kind` + `state` (typed TEXT tokens, `sqlite3`-inspectable, parsed back to typed enums —
  `no-string-matching`); `engine_version` (the combined **analyze provenance stamp** — § M2 — so an OS upgrade to the OCR
  engine, tag taxonomy, or feature-print model re-runs analysis even on an unchanged file — data-COVERAGE, not
  data-safety, since the derived data is disposable).
- `media_ocr` — a **standalone** FTS5 table (`path UNINDEXED, source UNINDEXED, text`). Not external-content:
  `agent/store`'s `messages_fts` points at an integer `messages.id`, but `media_status` is path-keyed and `WITHOUT
  ROWID`, so there's no integer rowid to hang external content off. Standalone keeps enrichment and GC a simple `WHERE
  path = ?` delete with no trigger machinery to desync. It holds up to two rows per path: the OCR text (`source='ocr'`)
  and the space-joined tag labels (`source='tag'`), so a keyword search matches **tags alongside OCR** (M2). Created via
  `CREATE VIRTUAL TABLE … USING fts5`, which doubles as the FTS5 availability guard (a `bundled` build without FTS5 fails
  there — Decision 2's build-flag worry is closed, `agent/store` proves it).
- `media_tags` — `(path COLLATE platform_case, label, score)` with an index on `path` and on `label`: the STRUCTURED
  tags for tag-score filtering (`images_with_tag(label, min_score)`), distinct from the folded FTS keyword index above.
- `media_embedding` — `WITHOUT ROWID`, `(path PRIMARY KEY COLLATE platform_case, dims, vector BLOB)`: the image
  feature-print embedding as a little-endian `f32` BLOB (`encode_embedding`/`decode_embedding`; `dims` = element count).
  The vector store's load source (§ M2).
- `meta` — `schema_version` only.

The `needs_enrichment` staleness predicate is `(path, mtime, size)` + the analyze stamp: stale when there's no row, or
when `(mtime, size)` changed, or when the stamp changed. State is deliberately excluded from the key so a failed file
isn't re-hammered every completed scan; a real file change re-tries it. A successful `upsert` writes `media_status` +
the OCR/tag FTS rows + `media_tags` + `media_embedding` in ONE transaction (clearing each prior row first, so a
re-enrichment leaves nothing stale); a failure clears them all and records only the `Failed` status.

## The image-qualification predicate (`predicate.rs`)

Pure over a directory's file names (sibling-aware): images enrich (JPEG/PNG/HEIC/…); videos skip (out of scope in M1,
also a Live Photo's motion `.mov`); an image with a same-stem `.mov` is tagged `LivePhotoStill`; `.aae` edit sidecars
skip; a RAW beside a same-stem JPEG defers to the JPEG (cheaper decode), a lone RAW enriches. Classification is typed
(`Qualification`/`MediaKind`/`SkipReason`), never a substring branch. The scheduler groups the index walk by parent
directory and runs `qualify_dir` per group.

## The `VisionBackend` seam (`backend/`)

The inference boundary the scheduler, store, and GC sit behind, so all of that is testable with no GPU/ANE/FFI. The
trait is `VisionBackend`: `ocr` (OCR only, for the focused OCR tests), `analyze` (the enrichment entry point — OCR +
tags + feature print from ONE decode, M2), and the provenance stamps `engine_version` / `taxonomy_version` /
`analysis_stamp` (§ M2). Two impls:

- `fake::FakeVisionBackend` — deterministic, zero-FFI (scripted/derived OCR text, tags, and a stem-derived unit
  embedding). Every test injects it via `MediaScheduler::new`; it's also the production fallback off-macOS.
- `vision::VisionOcrBackend` (macOS only) — the real OCR + classify + feature print. `scheduler::start` selects it on
  macOS.

CLIP embeddings and faces become sibling methods on this trait as M3+ land, each returning its own typed result, each
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
current `VNRecognizeTextRequest` revision (read off a fresh instance). The M2 `analyze` path additionally computes
`taxonomy_version` (the `VNClassifyImageRequest` revision) and folds all three revisions into `analysis_stamp` (§ M2).

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
offline/purged `media.db` returns an empty list, never an error. Because the read API reads `media.db` directly, the
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

## The frontend surface (M1)

Three IPC doors feed the UI; all live in `commands.rs` and are registered in BOTH `ipc.rs`
and `ipc_collectors.rs` (regen the typed bindings with `pnpm bindings:regen`).

- **`media_index_search_ocr`** — the OCR search (above). Consumed by the Search dialog's
  "Text in images" grid (`src/lib/search/ImageSearchResults.svelte`), which QueryDialog
  renders via its `resultsExtra` snippet slot (Search-only; Selection passes none). The
  grid reuses the SAME live query text as the filename results.
- **`media_index_volume_state`** → `MediaIndexVolumeState { enabled, indexing,
  enriched_count }` — the honest per-volume coverage signal (plan M1 § Coverage honesty +
  per-volume state). `indexing` is a cheap in-memory snapshot off the scheduler's
  `PassCoordinator::is_running` (`MediaScheduler::is_enriching`); `enriched_count` is a
  `COUNT(*)` over `media_status` read off the IPC thread. Deliberately NOT a progress
  percentage or ETA — those are M2. It lets the UI tell apart four states rather than ever
  showing a confident-looking empty result that's really "not indexed yet": off (hint to
  enable), still indexing ("results may be incomplete"), enriched-but-no-match (a genuine
  miss), and not-indexed-yet. It's polled per search (no event subscription yet; a
  subscription is a reasonable M2 upgrade).
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
  that's the accepted M1 cost of reusing the preview path rather than producing a
  downscaled thumbnail — a real thumbnail cache would be a media_index-produced file
  Decision 5 defers.

The "why matched" snippet (`[`/`]`-wrapped matched terms) is parsed to structured
segments by the pure `src/lib/search/ocr-snippet.ts` and rendered with `<mark>`, NEVER via
`{@html}` — a document whose OCR text contains markup can't inject anything.

The master toggle `mediaIndex.enabled` renders in Settings > Behavior > File system
watching (a dedicated "Image search" card, `FileSystemWatchingSection.svelte`), off by
default. It live-applies through `settings-applier.ts` → `setImageIndexEnabled` (no
restart), the standard backend-affecting-setting pattern.

## M2 — Tags, image-similarity embeddings, vector search, importance-prioritization

M2 adds Vision tags + image feature-print embeddings, a brute-force vector store, real importance-prioritized
scheduling, the settings-slider covered-count preview, the per-folder photo-search exclude, and an honest per-volume
progress denominator. Still zero model download (Vision-only); local + opted-in SMB both get tags + embeddings.

### `analyze`: one decode, three outputs (`backend/vision.rs`)

The enrichment path calls `VisionBackend::analyze`, not `ocr`. The real backend decodes the thumbnail ONCE
(`decode_thumbnail`, the shared M1 downscale) and performs THREE Vision requests on a single `VNImageRequestHandler`:
`VNRecognizeTextRequest` (OCR), `VNClassifyImageRequest` (scene/object tags), and `VNGenerateImageFeaturePrintRequest`
(the image↔image feature print). Reusing one decode + one handler is the Decision-5 "decode once" applied across all
three — decoding the original three times would dominate cost.

- **Tags** (`read_tags`): the top `MAX_TAGS` (12) classifications above `MIN_TAG_SCORE` (0.1), highest confidence first
  (Vision returns them sorted, so the read breaks at the floor). The taxonomy is FIXED by the OS — **1,303 identifiers on
  macOS 26.5.1** (verified 2026-07-13 via `VNClassifyImageRequest::supportedIdentifiersAndReturnError().len()`). A
  taxonomy change on an OS upgrade re-tags via the provenance stamp below.
- **Feature print** (`read_feature_print`): the first `VNFeaturePrintObservation`'s raw bytes decoded per `elementType`
  (`Float` → `f32`, `Double` → `f64`→`f32`), length-checked against `elementCount` (a mismatch drops it rather than
  storing garbage). Vision's feature print is image↔image only (no text encoder — that's M3's CLIP).
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
(`MediaScheduler::folder_scores` → `above_threshold(threshold)`), the SAME signal the M2 slider sets. The scheduler:

- **orders** the walk by folder importance descending (`enrich::prioritized`), so high-importance folders enrich first;
- **filters** via a `should_enrich(path)` closure: an EXCLUDED folder never enriches (hard privacy veto, checked first);
  otherwise enrich when an "always index" override covers it OR its folder importance meets the threshold. A deferred
  image stays in the GC `current` set, so a below-threshold folder's rows are never wiped — only vanished files are GC'd.
- **`folder_scores` returns `Option`** — `None` when importance NEVER scored the volume (fresh, offline, importance
  disabled; detected via `recompute_generation() == 0`). This `None` is load-bearing and differs by path: a LOCAL volume
  falls back to "enrich everything" (cheap local reads; the next pass after importance scores applies the threshold), a
  NETWORK volume falls back to "override only" (conservative — never drag a whole NAS off importance-absence). Floored
  junk (`node_modules`, caches, hidden/system) has no importance row at all, so it's excluded at any threshold.

Threshold lives in `gate` as an `f64`-bits atomic (`set_importance_threshold` / `importance_threshold`, clamped
`0.0..=1.0`), seeded from `mediaIndex.importanceThreshold` and live-applied by `media_index_set_importance_threshold`.
Default `0.0` (`DEFAULT_IMPORTANCE_THRESHOLD`): enrich every scored folder (non-regressive vs M1's "enrich all real
folders"), the slider raises it to defer low-importance folders. Importance keys on the INDEX identity, so the network
gate strips the mount root off the OS path before the lookup.

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

### Per-folder photo-search exclude (privacy complement)

`network::config` gained `excluded_folders` (seeded from `mediaIndex.excludedFolders`, live-applied by
`media_index_set_excluded_folder`): an image at or under an excluded folder never enriches, a HARD veto that beats any
"always index" override — the privacy complement to the opt-in (protect a high-importance `~/Documents/IDs` the
threshold alone can't). It stops FUTURE enrichment; existing rows for a now-excluded folder stay until the next GC/rescan
(purging them on exclude is a possible follow-up).

### M2 read API + commands

`MediaIndex` gained `find_similar(source_path, k)` (source embedding → `top_k` over the resident cache, source
excluded), `dedup_clusters(threshold)`, and `images_with_tag(label, min_score)` (structured tag-score filter). New async
commands (all `spawn_blocking`, offline-capable, registered in BOTH `ipc.rs` + `ipc_collectors.rs`; regen bindings):
`media_index_find_similar`, `media_index_dedup_clusters`, `media_index_search_tag`, `media_index_covered_count`,
`media_index_set_importance_threshold`, `media_index_set_excluded_folder`. **Shapes for the M2 frontend agent:**
`SimilarImage { path, score: f32 }`, `DedupCluster { paths: Vec<String> }`, `TagHit { path, score: f32 }`,
`CoveredCount { folders: u64, images: u64, pending: bool }`, `Tag { label, score: f32 }`, and the extended
`MediaIndexVolumeState { …, qualifying_count: Option<u64> }`. Live-apply for the threshold + exclude settings needs a
`settings-applier.ts` entry (the one FE handoff the backend can't do).

## What's left for later

- **M2 frontend (next agent):** the importance-threshold slider + live covered-count preview, the find-similar UI, the
  progress/ETA surface, the per-folder exclude UI, and tag-search surfacing. The backend commands + shapes above are
  ready; live-apply of the threshold + exclude settings needs a `settings-applier.ts` entry.
- **M3+:** CLIP text→image semantic search, the model-install path, faces (detect/embed/cluster/name), the durable
  identity store, and LLM captions.

## Testing

Most M1 tests are FFI-free and registry-free. Pure: the predicate (`predicate.rs`), the staleness key (`store/tests.rs`),
`gc_targets`, `build_ocr_match_query`, the coalescer (`scheduler/coalescing_tests.rs`), the command's limit clamp
(`commands/tests.rs`). Over the fake backend + a synthetic index: the walk, the enrich pass, deletion-driven GC, the
throttle/cancel decision, the edge-triggered `Completed` consumption (`scheduler/enrich_tests.rs`), and the OCR search +
offline-after-unmount round-trip (`read/tests.rs`), plus the FTS5 availability smoke. **macOS-gated real FFI**
(`backend/vision/tests.rs`, the module is macOS-only so it can't run off-macOS): real Vision OCR reads the known words
off the committed fixture, and hostile inputs (non-image, empty, missing) each return a typed `VisionError` with no
panic. The async wire-up (`ready_volumes_with_kind` sweep → `wire_volume` → `run_pass_blocking`) is covered indirectly by
the reactive pieces (bus-edge consumption + coalescer + the enrich core); a full end-to-end async test needs the
process-global index registry and is deferred to the E2E slice.

**M2 tests** (all real red→green on the pure/risky bits): cosine + `top_k` ranking + source exclusion + dedup grouping
(`vector/tests.rs`, pure); tags/embedding round-trip + tag-score filtering + the embedding codec + the
clear-on-re-enrichment invariant (`store/tests.rs`); `prioritized` ordering + the scheduler DEFERS a below-threshold
folder + ENRICHES an overridden one, both keeping deferred rows for GC (`scheduler/enrich_tests.rs`); the covered-count
arithmetic over a synthetic counts+scores map (`coverage.rs`); the fake backend's deterministic tags/feature-prints
(`backend/fake.rs`). **macOS-gated real FFI** (`backend/vision/tests.rs`): `analyze` returns real OCR + well-formed tags
+ a stable-length feature print off the fixture, and a real feature print's self-cosine is ~1.0.
