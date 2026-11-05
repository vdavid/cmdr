# Multi-Rename as a crate (`cmdr-multi-rename`), with a browser host

Agent-facing plan. Goal (David's decision): move the Multi-Rename Tool's engine out of
`apps/desktop/src-tauri/src/multi_rename/` into a new tauri-free crate, so the browser demo (`crates/cmdr-listing-wasm`)
runs the real sheet: masks, counters, search & replace, case steps, presets, preview, row reordering, and apply through
the real write engine with undo. In the demo, media fields (`cmdr-media-meta`) and Results (⌥⏎) don't work, and fail
through typed paths the sheet already words. Desktop behavior stays byte-identical.

Models to copy: `cmdr-viewer` (a value built over a host, a wasm half; `crates/cmdr-viewer/DETAILS.md` § "The browser
build"), `crates/cmdr-listing-wasm/src/viewer.rs` and `search.rs` (the demo's command families), `crates/cmdr-archive`
(the crate template). Process: `docs/guides/multi-agent-refactors.md` § The loop.

## Loud rules

- ❗ **Desktop byte-identical.** Same command names, args, answers, errors, events, files on disk, and `bindings.ts`
  with a ZERO diff (`desktop-bindings-fresh`). specta copies doc comments into `bindings.ts`, so ❌ don't reword a doc
  comment on a type that crosses IPC while moving it.
- ❗ **No `tauri`, no app paths, no ambient runtime in the crate.** Clock through `cmdr_fs::rt`, background work on the
  `rt::Runtime` the host hands in, never `tokio::task::spawn_blocking` / `Handle::current` / `std::time::Instant::now`.
  A `clippy.toml` copied from `crates/cmdr-viewer/clippy.toml` enforces it.
- ❗ **Persistence is the host's.** The crate holds the types and the pure list ops; the desktop keeps the files
  (`RecentsFile`, `PresetStore`, `state_recovery`), the demo keeps lists in memory.
- ❗ **No TS logic in the demo.** Every multi-rename command is answered by the engine (`DemoListing.handles`); ❌ no
  `answers.ts` entry, ❌ no TS-faked success. Demo gaps answer with typed errors the sheet already words.
- ❗ **One volume, one `Listings`, one write engine** in the demo: the multi-rename host takes `Demo`'s, never its own.
- ❌ No milestone tags (`M2`, "Phase 3") in code, comments, test names, `reason` strings, or docs. Only this file keeps
  them.
- Each milestone asks before reporting: is it solid AND elegant AND documented?

## What exists today (fresh read, 2026-11-05)

Module: `mod.rs`, `error.rs`, `mask.rs` + `mask/media_token.rs`, `transform.rs`, `transliterate.rs`, `plan.rs`,
`session.rs`, `names_file.rs`, `run.rs`, `media.rs`, `history.rs`, `presets.rs`, `preset_store.rs`, three
`*_token_vectors.json`, and 12 test files. IPC: `commands/multi_rename.rs` (24 commands, registered in
`ipc.rs:220–243`), the event `MultiRenameMediaProgress` (`ipc.rs:1362`). Other consumers: the MCP tool
`tools/actions/multi_rename.rs` (+ `_test.rs`), startup loads in `lib.rs:603–605`, `security_log/types.rs`'s `StateFile`
variants.

App reach, per file (everything else is std, `cmdr-fs`, `cmdr-listing`, `cmdr-media-meta`, `cmdr-operations`,
`cmdr-operation-log`, `cmdr-recents`, `chrono`, `regex`, `unicode-normalization`, `serde`, `specta`, `uuid`,
`futures-util`, `log`):

- Pure today: `mask.rs`, `mask/media_token.rs`, `transform.rs`, `transliterate.rs`, `plan.rs`, `error.rs`.
- `session.rs`: `crate::listing_host::listings()` (open: `selection_names`, `location`, `entries`; `compute`:
  `entries`), `crate::ignore_poison`, process statics `SESSIONS` / `NEXT_SESSION`, `std::time::Instant::now` (4 sites,
  panics in the browser), `MAX_SESSIONS = if cfg!(test) { 1024 } else { 4 }`.
- `media.rs`: `tauri_specta::Event` derive on `MultiRenameMediaProgress`; `Loader::for_volume` and `volume_reader`
  (volume manager, `io_budget_for_volume`, `VolumeByteSource`, `tokio::runtime::Handle::current`); `SessionMedia::run`
  (`tokio::task::JoinSet::spawn_blocking`, `Handle` stored in `Inner.loader`, `Instant::now`); `crate::pluralize` (one
  log line); `crate::ignore_poison`. The `Loader { reader, emit, parallel, volume_gone }` seam already exists (tests use
  `start_media_with`).
- `run.rs`: `portal_serves`, `path_routes_over_its_parent`, `get_volume_manager().resolve(...)` (read-only routes),
  `io_budget_for_volume`, `crate::operations_host::operations()` + `refuse_not_installed`, the `volume_id == "root"`
  branch (`SourceFingerprint::capture_local`, `std::fs`), `blocking_typed_result_with_timeout` (calls
  `tokio::task::spawn_blocking`: panics in the browser), `timeout_detached_within` (uses `rt::spawn_ambient`: fine).
- `names_file.rs`: pure std fs; `path_for` reads `crate::config::app_cache_dir()`.
- `history.rs`, `presets.rs`: `crate::recents::{RecentEntry, RecentsFile}` (the `RecentEntry` impls name `FILENAME` /
  `STATE_FILE`), statics `FIELD_HISTORY`, `LAST_SPEC`; `uuid::Uuid::new_v4` (fine in wasm: `cmdr-fs` turns on uuid's
  `js` feature there).
- `preset_store.rs`: all desktop (file, `tauri::AppHandle`, `state_recovery`, `PRESETS` static).
- `commands/multi_rename.rs`: the IPC deadlines (`blocking_typed_result_with_timeout`, `timeout_detached_typed`,
  `BlockingBudget` `PREVIEWS`), the emitter from `AppHandle`, `TauriEventSink`, the history add after apply.
- `tools/actions/multi_rename.rs` (the MCP tool) reaches deep into `media.rs`: it builds `Loader { reader, ..loader }`
  itself (`gated` wraps the reader with the sensitive-content gate), calls `Loader::for_volume` with a no-op emit,
  `SessionMedia::seed` / `adopt_where` / `read_all` / `progress` / `lookup` / `cancel`, matches `MediaRead`, reads
  `FileJob.path`, and calls `session::media_over`, `plan::{Compiled, preview_names, NameEdits, NoMedia}`. Its `read_all`
  runs on the ambient runtime (`JoinSet::spawn_blocking`), and its no-op emit means it sends no event.

Verified while planning:

- `chrono::Local` works in the browser: chrono's default `wasmbind` feature reads the offset from JS `Date`
  (`chrono-0.4.45/src/offset/local/mod.rs`, read 2026-11-05), and `chrono` is already in the wasm graph via
  `cmdr-operations`.
- `cmdr_fs::rt::task::JoinSet` in the browser has `spawn_on` and `join_next_with_id` but no `spawn_blocking*` and no
  `join_next` (which `SessionMedia::run` calls); natively it IS tokio's. `rt::Runtime::spawn_blocking` exists on both
  halves (`crates/cmdr-fs/src/rt/wasm.rs`).
- The log calls in `media.rs` / `names_file.rs` carry explicit `target:` strings (`multi_rename::media`, …), so
  `RUST_LOG` recipes keep working after the move, and `logging/provenance.rs` counts any `cmdr_*` crate as ours. Keep
  every moved log call on an explicit target.
- `cmdr_fs::deadline::blocking_*` and `BlockingBudget::run` call `tokio::task::spawn_blocking` directly: never callable
  from the browser.
- `cmdr_operations::Operations::start_renames` already gates read-only routes and the home folder, and picks the local
  disk by `host().volumes().local_disk(..)`; `DemoOperations::engine()` is `cfg(test)` only today.
- The demo answers no multi-rename command: `open_multi_rename` rejects, so the sheet never opens
  (`apps/desktop/src/lib/demo/DETAILS.md` § Writes, "Not answered yet").
- The frontend's `counter-token.test.ts`, `range-token.test.ts`, and `media-token.test.ts` import the vector JSONs by
  relative path into `src-tauri/src/multi_rename/`.

## Seam design

**Recommendation: a value plus a host trait, the `cmdr-viewer` shape.** The crate exports `MultiRename`, which owns the
sessions (today's `SESSIONS` / `NEXT_SESSION` become fields), built with
`MultiRename::builder(listings: Listings, runtime: cmdr_fs::rt::Runtime, host: Arc<dyn MultiRenameHost>)` plus
`.max_sessions(n)`. Today's `session::*` free functions become its methods (`open`, `preview`, `page`, `move_row`,
`range_samples`, `media_progress`, `media_overview`, `field_samples`, `prioritize_media`, `media_over`, `close`,
`spec_of`, `write_names`, `read_names`, `clear_names`, `apply`).

The host trait carries only what differs between hosts (shape to adapt as the code demands; keep it small):

- `media_loader(&self, volume_id: &str, emit: ProgressEmit) -> Loader`: the desktop's `Loader::for_volume` body (volume
  reader, `volume_gone`, `MAX_PARALLEL_READS`); the demo's reader answers `Err(ReadError::NotSupported)` for every file,
  which `SessionMedia::store` already turns into `Unreadable` without counting a failure (only `TimedOut` / `Io` count
  toward `give_up`), so every media file settles at once and the progress event ends `done`.
- **`emit` stays a `Loader` field, built by the caller** (the IPC command from its `AppHandle`, the demo from its sink),
  ❌ not a host-trait method: the MCP tool's `read_all` passes a no-op emit today and must keep sending NO event (a
  host-level emit would push `multi-rename-media-progress` for its `mcp-multi-rename` pseudo-session). The crate's
  `ProgressEmit` takes a crate payload, `MediaProgressEvent { session_id, read, total, done }` (serde camelCase, no
  specta); the desktop's `MultiRenameMediaProgress` (struct and `Event` derive stay desktop, same name and doc comments,
  so `bindings.ts` doesn't move) is built `From` it. A test pins that the two serialize identically, since the demo
  sends the crate's payload as the event body.
- `results_path(&self, session_id: &str) -> Option<PathBuf>`: desktop is today's `names_file::path_for`; the demo
  answers `None`, so `write_names` refuses with `CouldntWriteNames` exactly as a desktop with no cache dir does. The
  sheet words it (`multiRename.error.couldntWriteNames`) and never asks for an editor.
- `rename_place(&self, volume_id: String, dir: PathBuf, ready: &[PreviewRow]) -> BoxFuture<RenamePlace>`, with
  `RenamePlace { ReadOnly(ReadOnlyPlace), LocalDisk, Volume(Arc<dyn Volume>), NotConnected }`: ONE host call that is
  today's block from `run.rs` in today's order: one `get_volume_manager().resolve(..)`, its `routed`, else the per-row
  `row_route` (`portal_serves`, `path_routes_over_its_parent`) over old and new names, then the `"root"` rule, then
  `resolved.volume` or `NotConnected`. ❌ Don't split it into a read-only check plus a target lookup: that resolves
  twice and the second lookup by volume id alone could disagree with the first by `(volume_id, dir)`. The demo always
  answers `Volume(page volume)`, so fingerprints go through `capture_remote` (the demo's write engine has no local disk
  either).
- `io_budget(&self, volume_id: &str, base: Duration) -> Duration`: desktop forwards `io_budget_for_volume`; demo returns
  `base`.
- `operations(&self) -> Result<Operations, MultiRenameError>` (an owned clone of the handle) and the events sink is
  passed per `apply` call, as today.

Blocking work inside the crate (`apply`'s `prepare` and local fingerprints) runs on the held `rt::Runtime`: add a
runtime-taking twin of `blocking_typed_result_with_timeout` to `cmdr_fs::deadline` (natively `runtime.spawn_blocking` =
the same tokio pool the ambient call used, since the desktop hands in the Tauri runtime's handle; in the browser it runs
inline). The media loader spawns through a new `rt::task::JoinSet::spawn_blocking_on(f, &runtime)`: natively it IS
tokio's `JoinSet::spawn_blocking_on`, so the desktop loop doesn't change; in the browser it runs `f` inline and queues
its output as a finished task (counted in `len()`, removed on join, so the loop's `reads.len() < parallel` gate holds).
The wasm `JoinSet` also needs `join_next` (only `join_next_with_id` exists there). Both go through
`crates/cmdr-fs/DETAILS.md` § "The browser build" and its rt tests.

Where the runtime comes from: `SessionMedia` stops storing `Handle::current()`; `start` takes the `MultiRename`'s
runtime, and `read_all(loader, runtime)` takes one too. The MCP tool (desktop code, ambient calls allowed) passes
`Handle::current()`, exactly the pool it reads on today. The desktop host builds `volume_reader`'s byte-source handle
from the same held runtime, ❌ not `Handle::current()` at loader build time.

**IPC-level deadlines stay in the hosts.** `commands/multi_rename.rs` keeps every `blocking_*`,
`timeout_detached_typed`, and `PREVIEWS` budget around the method calls, unchanged. The demo calls methods inline, as
`viewer.rs` does (an in-memory listing can't hang).

**Persistence split.** Crate: `MultiRenamePreset`, `PresetList`, `PresetSeed`, `PresetSaveRefusal`, `PresetMove`, the
`*_in` list ops, `MAX_PRESETS`, `LastSpec` (with its legacy `StoredLastSpec` read), `MultiRenameLastSettings`,
`FieldHistoryEntry`, `HistoryField`, `MAX_FIELD_HISTORY`, `history_seed`, `history_entries`, and the `RecentItem` impls.
Desktop: `impl RecentEntry for LastSpec / FieldHistoryEntry` (orphan rule allows it: the trait is the app's), the
`FIELD_HISTORY` / `LAST_SPEC` statics, `preset_store.rs` unchanged. Demo: `DemoMultiRename` holds `presets: Vec` +
`seeded: bool` (running `seed_in` once on the first `get_multi_rename_presets`, as `PresetStore::seed` does),
`last: Option<LastSpec>`, and `history: Vec<FieldHistoryEntry>` started from `history_seed()` (a fresh page is a new
install), added to with `cmdr_recents::add_to(.., MAX_FIELD_HISTORY, ..)`.

**The desktop host keeps the folder name `multi_rename/`** (like `file_viewer/` for the viewer): `host.rs` (the trait
impl and the app's `MultiRename` in a `LazyLock` over `crate::listing_host::listings()` and
`tauri::async_runtime::handle().inner().clone()`, the `file_viewer/app_host.rs` pattern: the desktop tests install
nothing, so a `OnceLock` set in `setup()` would leave them without an engine; and a per-test `Handle::current()` would
die with its test's runtime while the static outlives it), the `Loader::for_volume` / `volume_reader` body the MCP tool
also calls, `preset_store.rs`, the persistence statics, the event struct, and the desktop-only tests. Its `C+D.md`
shrink to the host's must-knows and point at the crate's.

**Media metadata in the browser build.** The crate needs only `cmdr-media-meta`'s vocabulary (`MediaFacts`,
`MediaField`, `MediaValue`, `FieldKind`, `ReadError`, `is_media_name`); the readers (`lofty`, `kamadak-exif`) are the
desktop host's. Recommendation: a default-on `read` feature in `cmdr-media-meta` gating `read.rs`, `source.rs`, the
parsers, and their deps; the crate depends on it with `default-features = false`, the desktop with defaults. Measure the
wasm size both ways first (§ M3); if the readers cost under ~50 KB gzipped, skip the feature and say so in the crate's
`DETAILS.md`.

Rejected: (a) free functions with injected closures and a process static (the demo's golden replay builds several
engines per module, a static would share sessions across them, and desktop tests would share a cap of 4); (b) keeping
`run.rs` in the desktop and having the demo reimplement apply (two copies of the proof `prepare` gives, the thing most
worth sharing); (c) a demo that seeds media files `Unreadable` at `open` with no loader (a second code path just for the
demo, and the progress event would never run there).

## Milestones

Per-milestone cadence (from `AGENTS.md` § Checker script): before each commit
`pnpm check --fast && pnpm check svelte-tests-related`, plus `pnpm check clippy` and
`cargo nextest run -p <touched crates>` / the touched desktop tests when Rust moved. Each milestone also names its own
gates below. Before landing: `pnpm check` in the background; this is a 3–4 agent effort, so no `--only-slow` unless the
lead decides otherwise. Push each green milestone with `pnpm land -no-wait`.

### M1: characterization pins

- **Scope**: desktop tests only, in `apps/desktop/src-tauri/src/multi_rename/`. No production code.
- **Intentions**: pin what the extraction could silently change and isn't pinned yet. Audit the 11 test files first and
  add only what's missing; likely gaps:
  - A loader whose reader answers `NotSupported` for every file: all media files end `Unreadable`, the failure count
    never trips `give_up`, the last event is `done: true`, and a preview using `[=exif.taken]` shows `Unreadable` rows
    while `[=exif.taken|modified]` falls back (the demo's exact path, run natively).
  - `write_names` with no Results path answers `CouldntWriteNames` and leaves the session usable.
  - The MCP tool's media read sends no `multi-rename-media-progress` event (its emit is a no-op today); pin it so moving
    `emit` can't start broadcasting the tool's pseudo-session.
  - Apply's refusal order: a routed (archive / portal) folder answers `ReadOnly` before `NotConnected`, and a ready row
    whose new name lands in a portal path answers `ReadOnly` (only if `run_test.rs` doesn't already pin both).
  - A JSON snapshot of each wire shape the demo must mirror: `MultiRenameOpened`, `MultiRenamePreview`, a `PreviewRow`
    per status, every `MultiRenameError` variant, `MultiRenameMediaProgress`, `MediaOverview`, `FieldSamples`,
    `MultiRenamePreset`, `MultiRenameLastSettings`, `FieldHistoryEntry` (serde, not specta), so M2 proves serde didn't
    move.
  - Session eviction at the real cap (4) through a constructor knob is M2's; here just note it.
- **Landmines**: pin reality, not the plan. Make each new pin fail once on purpose (red first) and note how.
- **Test plan**: the new tests themselves.
- **DONE**: pins green, red-first noted in the commit body, no production diff.

### M2: extract the crate, desktop adapts (no behavior change)

- **Scope**: new `crates/cmdr-multi-rename/` (Cargo.toml modeled on `cmdr-archive` / `cmdr-viewer`:
  `[lints] workspace = true`, `specta = "=2.0.0-rc.24"`, path deps only plus existing workspace versions; `clippy.toml`
  from `cmdr-viewer`'s), workspace `members`, the `cmdr_fs::rt` / `cmdr_fs::deadline` additions, the desktop host, the
  MCP tool (`tools/actions/multi_rename.rs` + `_test.rs`: imports, `Loader::for_volume` → the host's loader fn,
  `media_over` on the app's `MultiRename`, `read_all` with `Handle::current()`), `svelteInputs` and ci.yml's `svelte`
  filter (the vector JSONs), `ipc.rs`, `lib.rs` startup, the frontend test imports of the three vector JSONs (they move
  with `mask.rs`).
- **Intentions**:
  - Move pure modules verbatim (`git mv` where it keeps history readable). `pub(crate)` becomes `pub` only where a host
    or the MCP tool needs it; keep the surface small and measure it.
  - `session.rs` statics become `MultiRename` fields; `Instant` becomes `cmdr_fs::rt::Instant` / `rt::now()`.
  - `media.rs`: `tokio::runtime::Handle` becomes `rt::Runtime`; `JoinSet` becomes `rt::task::JoinSet` with
    `spawn_blocking_on`; `Instant` through `rt`; inline the `pluralize` log text; `Loader::for_volume` and
    `volume_reader` move to the desktop host.
  - `run.rs`: the place (`rename_place`, one call), budget, and engine through the host; `tokio::time::Duration` becomes
    `std::time::Duration`; the two `blocking_typed_result_with_timeout` calls take the held runtime.
  - Wire guard: `MAX_SESSIONS` stays 4 in production, and the desktop's TEST install sets a high cap through the builder
    (today `cfg!(test)` does it, which a crate's `cfg(test)` can't see from the app's tests).
  - Add the crate to `index-crate-isolation`'s guarded list and a ceilings entry measured at extraction, with a comment
    in the house style (see `cmdr-sort` / `cmdr-recents`); `inputs.go`'s package list; `cargo deny` / `machete` clean.
  - Tests: pure tests move with their modules. Tests needing a listing move too if `cmdr-listing`'s `testing` fixtures
    (`TestEngine`, `TestListing`) over `InMemoryVolume` serve them; tests needing the real disk, the volume manager, the
    real reader, `state_recovery`, or `operations_host` (`run_test.rs`, `media_real_test.rs`, `presets_test.rs`'s file
    cells, `names_file_test.rs`'s cache-dir cell) stay as desktop host tests.
- **Landmines**:
  - `bindings.ts` must not change by a byte: run `pnpm check desktop-bindings-fresh` and diff. Moving a type can reorder
    specta's output or change a doc comment.
  - The MCP tool shares `SessionMedia` and `media_over` with sessions (`tools/actions/DETAILS.md`); keep `adopt` and
    `read_all` working against the installed `MultiRename`.
  - `names_file.rs` uses `std::os::unix::fs::OpenOptionsExt` under `cfg(unix)`; make sure the non-unix half compiles for
    wasm32 (it's never reached there, since `results_path` is `None`).
  - `rt::Runtime` natively IS `tokio::runtime::Handle`: the desktop must hand in the Tauri async runtime's handle, the
    one the ambient calls used, or blocking work moves pools.
  - `media_test.rs` runs each test on its own `multi_thread` runtime; with the held Tauri runtime the loaders run on the
    process-wide one instead. Fine, but a test that leans on its own runtime's clock or shutdown needs a look.
  - Vector JSONs: the frontend vitest lane's inputs (`svelteInputs` in `scripts/check/checks/inputs.go`, mirroring
    ci.yml's `svelte` filter) never covered `src-tauri/src/multi_rename/*_token_vectors.json`, so a vector edit is a
    cache hit for the three frontend tests today. Add the new paths
    (`crates/cmdr-multi-rename/src/**/*_token_vectors.json`) to both while moving them.
  - Don't let the crate depend on `cmdr-listing-wasm` or anything app-side; `index-crate-isolation` catches `tauri` but
    not a wrong-direction workspace dep.
  - `file-length` / `jscpd` allowlists don't name these paths today (checked); a moved file over the limit is a new
    entry, your call per `AGENTS.md` § Hard rules.
- **Test plan**: `cargo nextest run -p cmdr-multi-rename`, the desktop's `multi_rename` and
  `tools::actions::multi_rename` tests, M1's pins unchanged,
  `pnpm check desktop-bindings-fresh index-crate-isolation clippy`, the three frontend vector tests, and
  `cargo build -p cmdr-multi-rename --target wasm32-unknown-unknown` (add it to `browserBound` in
  `scripts/build-listing-wasm/main.go` until M3 links it).
- **DONE**: desktop green with zero `bindings.ts` diff, the crate compiles for wasm32, no `tauri` in its tree, docs for
  the crate exist (`CLAUDE.md` + `DETAILS.md`, can be thin, M4 finishes them).

### M3: the browser host, demo wiring, smoke, and the demo spec

- **Scope**: `crates/cmdr-listing-wasm` (`src/multi_rename.rs` + test, `commands.rs` routing, `DemoOperations` accessor
  for the engine and an op sink, `smoke-multi-rename.mjs` imported by `smoke.mjs`), the image-key lists,
  `apps/desktop/src/lib/demo/engine-commands.test.ts`, `apps/website/e2e/demo.spec.ts`.
- **Intentions**:
  - `DemoMultiRename` with `MULTI_RENAME_COMMANDS` (all 24 Tauri names) and `handles`, built in `Demo::new` over
    `listings.clone()`, `page_runtime()`, and a host over the page volume, the write engine, and the sink. Routed after
    the viewer and search in `Demo::invoke`, and chained into `Demo::handles` and `Demo::commands()` (what `bindings.rs`
    exposes and `engine-commands.test.ts` reads). Apply's `OperationEventSink` is the one `DemoOperations::op_sink`
    builds (private today), so a rename's events pass `ImportWatch` like every write's.
  - Entry points need no change: `file.multiRename` is ⌃M, which no macOS browser takes and `browser-shortcuts.ts`
    leaves alone; the File menu (`cmdr-menu`'s `MULTI_RENAME_ID`) and the palette dispatch the same command. Before
    `open_multi_rename` the front end calls nothing multi-rename (`refuseMultiRename` is TS capability logic; the demo
    volume is writable), and the sheet then calls `get_multi_rename_last_settings` (`null` → defaults),
    `preview_multi_rename`, the media-progress listen + `get_multi_rename_media_progress`, and lazily presets, history,
    and `render_multi_rename_examples`. Playwright's synthetic keys can't prove browser chrome leaves ⌃M alone, so David
    checks it by hand in Safari, Chrome, and Firefox.
  - Wire mirrors `commands/multi_rename.rs`: camelCase args (`listingId`, `includeHidden`, `selectedIndices`,
    `expectedSequence`, `sessionId`, `previewId`, `offset`, `limit`, `filter`, `row`, `direction`, `token`, `rows`,
    `examples`, `spec`, `presetId`, `seeds`, `preset`, `id`, `name`), the `Result` commands reject with the serialized
    `MultiRenameError` / `PresetSaveRefusal`, the void ones (`prioritize_multi_rename_media`, `close_multi_rename`,
    `save_multi_rename_last_settings`) answer `null`.
  - `apply_multi_rename` adds `history_entries` after a start, as the desktop command does.
  - Measure the `.wasm` size (raw, gzip -9) before and after, with and without the `cmdr-media-meta` `read` feature, and
    record it in `crates/cmdr-listing-wasm/DETAILS.md` § The build.
- **Landmines**:
  - Every crate the module links is an image input: add `crates/cmdr-multi-rename` and `crates/cmdr-media-meta` (and
    anything else new in
    `cargo tree -p cmdr-listing-wasm --target wasm32-unknown-unknown -e normal --prefix none | rg '\(/'`) to
    `scripts/website-demo-key.sh`, `websiteDemoInputs` in `scripts/check/checks/inputs.go`, and ci.yml's `website`
    filter. Drop the crate from `browserBound` once linked.
  - Browser traps compile and then panic: `Instant::now`, `spawn_blocking`, threads. Only the Node smoke run catches
    them; it must exercise open, a media read to `done`, preview, apply, and undo.
  - The demo has 1,200 photo names, so a session over them queues 1,200 instant `NotSupported` reads inline; check the
    page stays responsive (the smoke's `setTimeout` probe pattern from `smoke-writes.mjs`) and that the progress event
    is throttled, not 1,200 events. Expect the whole read in ONE task: an inline `spawn_blocking_on` leaves `join_next`
    always ready, so the loop never yields (no tokio coop budget in the browser), and the throttle sends just the first
    and the final `done` event. Cheap per file, so fine; if the probe says otherwise, `rt::task::yield_now` every N
    files in the loop is the fix (it also runs on the desktop, so measure its cost there first).
  - `catch_unwind` does nothing under `panic = "abort"`; harmless while the demo reader can't panic, but note it in the
    crate's browser-build section.
  - Events reach the sheet through the page sink's deferral; the sheet may subscribe late and asks
    `get_multi_rename_media_progress`, so that must answer correctly after `done`.
- **Test plan**:
  - Native `cmdr-listing-wasm` cells: command names and the event name pinned against `bindings.ts` (as `search_test.rs`
    does), open over a listing, preview with `[N]_[C:2]`, `move_multi_rename_row` then `previewOutOfDate` until the next
    preview, apply then `directory-diff` and the names on the volume, roll back, presets
    save/rename/move/delete/`listFull`, seeds once, last settings round trip, history after apply, a media mask ending
    `unreadable` with a final `done` event, `write_multi_rename_names` refusing `couldntWriteNames`.
  - `smoke-multi-rename.mjs` under Node on the optimized module: open, a counter mask preview, a row move, apply and its
    `write-settled`, the journal's undo, the media read to `done`, with no unhandled rejection.
  - `engine-commands.test.ts`: the multi-rename commands are engine-only (no TS answer).
  - `apps/website/e2e/demo.spec.ts`: ⌃M on a selection, type a mask, see the preview, Rename, the pane shows the new
    names, Roll back restores them; a `[=exif.taken]` mask shows the couldn't-read status; `__cmdrDemo.unanswered` has
    no multi-rename entry.
  - Gates: `pnpm check desktop-rust-listing-wasm` (smoke + golden), `website-e2e`, plus the cadence.
- **DONE**: in `pnpm dev:demo`, the sheet opens, previews, reorders, renames, and undoes; media and Results fail typed
  and worded; smoke and the demo spec green. Hand David a stable state to look at (the sheet open on a selection) rather
  than judging timing-sensitive UI from frames.

### M4: docs and close-out

- **Scope**: docs only, plus the found bug below.
- **Intentions**:
  - `crates/cmdr-multi-rename/CLAUDE.md` (300–400 words: module map, the must-knows moved from today's
    `multi_rename/CLAUDE.md`) and `DETAILS.md` (today's DETAILS moved, plus § Host seams and § The browser build).
  - The desktop host's `multi_rename/CLAUDE.md` + `DETAILS.md`: host-only must-knows, a pointer to the crate.
  - `crates/cmdr-listing-wasm/CLAUDE.md` (module map, the "same wire" mirror list) and `DETAILS.md` (§ Multi-Rename, §
    Where the demo differs: no media reads, no Results, presets and history in memory, no IPC deadlines).
  - `apps/desktop/src/lib/demo/DETAILS.md`: drop multi-rename from "Not answered yet".
  - Root `AGENTS.md` crate list, `docs/architecture.md`, `crates/cmdr-media-meta/CLAUDE.md` + `DETAILS.md` (who reads
    it, and the `read` feature if added), `apps/desktop/src-tauri/src/recents/CLAUDE.md`, `tools/actions/DETAILS.md`,
    `file_system/git/DETAILS.md`, `src/lib/multi-rename/CLAUDE.md` + `DETAILS.md` (backend paths, vector JSON paths),
    `src/lib/file-explorer/pane/DETAILS.md`. Leave `docs/notes/` history as is unless a link breaks.
  - Run the milestone-tag `rg` from `docs/guides/multi-agent-refactors.md` over every touched path.
- **Test plan**:
  `pnpm check docs-reachable docs-dead-links docs-link-text claude-md-length claude-md-details-sibling resident-doc-budget`.
- **DONE**: every touched `C+D.md` describes the current shape; conformance review against the invariants below.

## Found bugs

- **`apps/desktop/src/lib/demo/DETAILS.md` repeats itself**: lines 378–574 are a second copy of § Right-click menus
  through § Drag and drop (lines 184–377), and the copy is stale: its § Drag and drop says files dragged in from the OS
  "do nothing ... Decision: no import", which § Files dropped in from the computer contradicts. Fix in M4: delete the
  second copy, keep the first (it carries the current text). Clear win.
- Not a bug today, a trap the extraction creates: `MAX_SESSIONS`'s `cfg!(test)` raise stops reaching the desktop's tests
  once the code lives in a crate, so parallel desktop tests would evict each other's sessions at a cap of 4. M2 replaces
  it with a builder knob.
- **The frontend vector tests can go green from cache on a vector edit**: `svelteInputs`
  (`scripts/check/checks/inputs.go`) and ci.yml's `svelte` filter cover `apps/desktop/src/**` but not
  `src-tauri/src/multi_rename/*_token_vectors.json`, which `counter-token.test.ts`, `range-token.test.ts`, and
  `media-token.test.ts` import. Fixed in M2 alongside the move (the new crate path goes into both). Clear win.

## Decisions (lead, 2026-11-05)

1. **`index-crate-isolation` ceiling**: a fresh entry measured at extraction, as `cmdr-sort` got. A new crate's first
   number is not a loosening.
2. **`cmdr-media-meta` `read` feature**: decide by the M3 measurement, as the seam section says.
3. **Real media reads in the demo**: out of scope, no issue.
4. **Results in the demo: a capability flag.** "Zero `bindings.ts` diff" holds for the extraction (M2). M3 adds ONE
   deliberate, additive wire change: `MultiRenameOpened` gains `resultsAvailable: bool` (the host's `results_path`
   answers `Some`; the desktop is always `true`). The sheet disables its Results control (⌥⏎ and its button) when it's
   `false`, with a tooltip saying Results needs the desktop app (a new English string with a good `@key` description; ❌
   never translate it). Why: "Check that your startup disk has free space" must never reach a demo visitor.
   `write_names` still refuses `CouldntWriteNames` if called anyway. Regenerate with `pnpm bindings:regen`, name the
   field in the commit body, and add a vitest cell for the disabled state.

## Invariants (conformance footer)

1. Every desktop multi-rename command answers the same JSON for the same inputs; `bindings.ts` has a zero diff through
   the extraction, and afterwards differs only by `MultiRenameOpened.resultsAvailable`.
2. Desktop files on disk (`multi-rename-presets.json`, `-last.json`, `-history.json`, the Results file) keep their
   names, shapes, and recovery paths.
3. `cmdr-multi-rename` has no `tauri`, no app crate, no ambient runtime call, and no `std` clock read; its clippy bans
   hold.
4. The selection becomes names once, at open; apply proves its ready rows equal the preview's (`prepare`) on both hosts,
   through the same code.
5. Apply runs through `Operations::start_renames` on both hosts, journaled, so the operation log's roll back undoes it.
6. The demo's media and Results gaps are typed answers the sheet already words (`Unreadable` rows, Results disabled by
   `resultsAvailable: false`), never a TS answer, a fake success, a hang, or a panic.
7. The demo shares one volume, one `Listings`, and one write engine across every engine.
8. Every crate the wasm module links is in the three image-key lists.
9. No multi-rename command has a TS answer in `lib/demo/`.
10. No milestone tags outside this file.
