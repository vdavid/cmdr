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

M1 wires LOCAL volumes only. `wire_volume` skips SMB/MTP with a log; their byte-fetch enrichment is M1.5 (the one part
with no `importance/` sibling to copy — `importance` never reads bytes off the wire).

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

## Schema (`store/`)

`SCHEMA_VERSION` is a disposable-cache version: a mismatch delete-and-recreates `media-{volume_id}.db`. Three objects:

- `media_status` — `WITHOUT ROWID`, `path TEXT PRIMARY KEY COLLATE platform_case`; `mtime`, `size` (the `(path, mtime,
  size)` staleness key); `media_kind` + `state` (typed TEXT tokens, `sqlite3`-inspectable, parsed back to typed enums —
  `no-string-matching`); `engine_version` (the OS/Vision engine stamp so an OS upgrade re-runs OCR even on an unchanged
  file — data-COVERAGE, not data-safety, since OCR text is disposable).
- `media_ocr` — a **standalone** FTS5 table (`path UNINDEXED, text`). Not external-content: `agent/store`'s
  `messages_fts` points at an integer `messages.id`, but `media_status` is path-keyed and `WITHOUT ROWID`, so there's no
  integer rowid to hang external content off. Standalone keeps enrichment and GC a simple `WHERE path = ?` delete with no
  trigger machinery to desync. Created via `CREATE VIRTUAL TABLE … USING fts5`, which doubles as the FTS5 availability
  guard (a `bundled` build without FTS5 fails there — Decision 2's build-flag worry is closed, `agent/store` proves it).
- `meta` — `schema_version` only.

The `needs_enrichment` staleness predicate is `(path, mtime, size)` + engine: stale when there's no row, or when
`(mtime, size)` changed, or when the engine stamp changed. State is deliberately excluded from the key so a failed file
isn't re-hammered every completed scan; a real file change re-tries it.

## The image-qualification predicate (`predicate.rs`)

Pure over a directory's file names (sibling-aware): images enrich (JPEG/PNG/HEIC/…); videos skip (out of scope in M1,
also a Live Photo's motion `.mov`); an image with a same-stem `.mov` is tagged `LivePhotoStill`; `.aae` edit sidecars
skip; a RAW beside a same-stem JPEG defers to the JPEG (cheaper decode), a lone RAW enriches. Classification is typed
(`Qualification`/`MediaKind`/`SkipReason`), never a substring branch. The scheduler groups the index walk by parent
directory and runs `qualify_dir` per group.

## The `VisionBackend` seam (`backend/`)

The inference boundary the scheduler, store, and GC sit behind, so all of that is testable with no GPU/ANE/FFI. The
trait is `VisionBackend` (`engine_version` + `ocr`). Two impls:

- `fake::FakeVisionBackend` — deterministic, zero-FFI (scripted/derived OCR text). Every test injects it via
  `MediaScheduler::new`; it's also the production fallback off-macOS.
- `vision::VisionOcrBackend` (macOS only) — the real OCR. `scheduler::start` selects it on macOS.

Tags, image feature prints, CLIP embeddings, and faces become sibling methods on this trait as M2+ land, each returning
its own typed result, each fakeable the same way.

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
current `VNRecognizeTextRequest` revision (read off a fresh instance). Both bump when the OS ships a new OCR engine, so a
stored row's stamp mismatches and re-runs — data-COVERAGE, cheap and stable within an OS version.

The fixture for the macOS-gated real-OCR test lives at `backend/test-fixtures/ocr-sample.png` (a tiny PNG rendering
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

## What M1 leaves out

The search/query-ui frontend + thumbnail grid + coverage-honesty line; the settings toggle UI + the M1 E2E; SMB/MTP
enrichment + the conservative byte-fetch policy (M1.5); tags, feature prints, CLIP, faces, the durable identity store,
and the importance-threshold slider (M2+).

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
