# The viewer crate and the demo's viewer window

Issue [#302](https://github.com/vdavid/cmdr/issues/302), round two, steps 5 and 6. Pull the file viewer's core out of
`apps/desktop/src-tauri/src/file_viewer/` (about 23,000 lines with tests) into a Tauri-free crate, then build that crate
into the demo's WASM engine and open the real viewer window in the browser demo: text, Markdown, hex/binary, images, and
PDF, the same kinds the app covers (`ViewerContentKind` is `Text | Image | Pdf | Markdown`; the app has no video or
audio view, so neither does the demo).

Survey taken 2026-11-04 on `main` at `49aad7f793`. Re-grep before each milestone: the code moves under you.

## Loud rules

1. **Desktop stays byte-identical.** `bindings.ts` regenerates with zero diff, every event keeps its name and payload,
   every command keeps its answers, timeouts, and threads. Existing tests MOVE, they don't get rewritten; a test that
   has to change its assertion is a behavior change and needs the lead's OK.
2. **Characterize first.** The wire-shape and open-routing pins (milestone 1) land before any code moves, and stay green
   unchanged through every later commit.
3. **The crate never names Tauri or the app.** No `tauri`, `tauri_specta`, `crate::` reach into `cmdr_lib`, no volume
   manager, no `AppHandle`. App concerns arrive through injected host seams (`host/`, one trait per concern).
   `index-crate-isolation` enforces it.
4. **Threads and clocks go through seams.** Every background job goes through the crate's one worker module (natively
   the same `std::thread::spawn` it is today); every clock read through `cmdr_fs::rt`. The crate's `clippy.toml` bans
   the direct calls, like `crates/cmdr-listing/clippy.toml`.
5. **No viewer logic in TS, no `if (demo)` in app code** (`cmdr/demo-isolation`). The demo answers the viewer's commands
   from the real crate inside `cmdr-listing-wasm`; TS only routes, and answers window and menu plumbing.
6. **The security model survives both hosts.** ammonia's allowlist is untouched; a media URL is still minted only from
   an unguessable, session-scoped token; a closed window frees its session AND its token (and, in the demo, its `blob:`
   URL).
7. **The app keeps** the Tauri commands (`commands/file_viewer.rs`), the `cmdr-media://` scheme handler, "Open with"
   (`open_with_extract.rs`, `open_with_copies.rs`), analytics (`analytics.rs`), and the pull-to-temp materializer
   (`materialize.rs`), each implementing a host seam where the crate needs it.
8. **No milestone tags in code, comments, `reason` strings, test names, or docs**
   (`docs/guides/multi-agent-refactors.md`).
9. **Foreground commands only** for every executing agent.

## Survey: what couples the viewer to its host

Counts are non-test files. The headline: the core (rows, encodings, the three backends, search, Markdown render, content
kinds) is already pure; the coupling is concentrated in seven files, plus seven thread spawns, a handful of `Instant`s,
and three `cfg(unix)`-only std calls that break a wasm32 compile.

- **Tauri** (4 files):
  - `session.rs`: a process-wide `AppHandle` (`init_app_handle`) only to `emit("viewer:file-changed:<sid>")` from the
    watcher. Becomes a host event call.
  - `media_protocol.rs`: the URI scheme (`tauri::http`, `UriSchemeResponder`, `async_runtime::spawn_blocking`,
    `tokio::time::timeout`, `ViewerMediaLoad::emit_to`). Stays in the app; its pure half (token parse, `resolve_range`,
    `build_response`, the volume read loop) moves so the demo serves bytes through the same code.
  - `materialize.rs`, `markdown_images.rs`, `volume_source.rs`: `tauri::async_runtime::block_on` (and one `spawn` in
    `volume_source.rs`) bridging sync viewer calls to async volumes.
  - `tauri_specta::Event` derives on `ViewerMediaLoad` (`media_volume.rs`), `ViewerPullProgress` (`pending_open.rs`),
    and `OpenWithCopyLaunched` / `OpenWithCopyRefused` (`open_with_extract.rs`, which stays).
- **The app** (`crate::`):
  - `file_system::volume::manager::{get_volume_manager, path_routes_over_its_parent}`: `origin.rs`, `materialize.rs`,
    `markdown_session.rs` (`volume_root`), `open_with_extract.rs`. The types (`Volume`, `VolumeError`,
    `VolumeRandomReader`, `RoutedKind`) are already `cmdr_fs::volume`; only the registry and the routing check are
    app-side.
  - `commands::file_system::expand_tilde`: a re-export of `cmdr_fs::tilde::expand_tilde`; use that directly.
  - `redact::display_line` (`row_text.rs`, `log_view.rs`): the debug log's marker-free rows. Host seam.
  - `analytics::events::capture` (`analytics.rs`, called from `session::open_session_inner`). Host seam.
  - `ignore_poison`: `cmdr_fs::ignore_poison` already exists.
  - `test_support` in tests: `cmdr_fs::testing` already has `TestDir`, `wait_until`, `wait_until_async`.
- **`std::fs`** (crate-bound files): `source.rs` (`LocalFileSource`, `open_local`), `origin.rs` (`current_size`),
  `session.rs` (5: exists / is_dir checks, the save path via `cmdr_fs::staging`), `watcher.rs` (canonicalize, metadata),
  `markdown_images.rs` (local image entry), `media.rs`, `encoding.rs` (test-only). `std::os::unix::fs::FileExt`
  (`source.rs`) and `MetadataExt` (`watcher.rs`) don't compile for `wasm32-unknown-unknown`.
- **Threads** (all `session.rs`): the LineIndex upgrade and its timeout sleeper, the search worker and its watchdog
  (`thread::sleep` poll), the encoding rebuild, the tail catch-up, and the watcher manager (`recv_timeout` loop). Each
  panics in the browser. Plus `test_hooks.rs`'s `Condvar` gates (test-only).
- **Clocks**: `Instant` in `session.rs` (search watchdog), `pending_open.rs`, `media_volume.rs`; `SystemTime` only in
  the Open-with files (staying). `cmdr_fs::deadline` reads `tokio::time::Instant::now` directly, so `StallWatch` /
  `blocking_typed_result_until_stalled` panic in the browser too.
- **Statics**: `SESSIONS`, `WINDOW_TO_SESSION`, `MEDIA_TOKENS`, the pending-open map, the materialize and open-with
  dirs, the watcher registry. Fine in a single-threaded wasm (std `Mutex` works there); whether they stay statics is
  Decision 1.
- **Randomness**: session ids are `uuid` v4 (wasm-wired through `cmdr-fs`'s `js` feature); media tokens are `rand`
  0.10's OS RNG, whose `getrandom` needs its own wasm backend wiring.
- **Native-only deps**: `notify`, `notify-debouncer-full` (watcher), `cmdr_http` / reqwest (remote Markdown images),
  `cmdr_local::space` (materialize). wasm-fine but heavy: `regex`, `ammonia` (html5ever), `encoding_rs`, `image`
  (header-only dimension reads), `pulldown-cmark`, `arc-swap`, `memchr`.
- **Consumers outside `file_viewer/`**: `commands/file_viewer.rs` (every command), `tools/read/inspect/*` (headless
  backends, `Matcher`, `content_kind`, `encoding`, `materialize`, `read_image_dimensions`, many tests),
  `menu/open_with.rs` and `menu/context_menu_facts.rs` (open-with), `commands/media_index/thumbnail.rs` (tokens,
  `content_kind`), `ipc.rs` (event types), `tauri_builder.rs` (scheme), `app_lifecycle.rs` (`close_session_for_window`
  on `Destroyed`), `lib.rs` (three `init_*`), `adb/volume_wiring_test.rs`, `redact/error_producer_tests.rs`,
  `operations_host/backend_suites/viewer_media_integration_test.rs`.
- **Frontend**: `routes/viewer/media-view.ts` `mediaUrl(token)` hardcodes `cmdr-media://localhost/<token>`, the one
  media seam; `MediaPdfView.svelte` renders `<embed type="application/pdf">`, `MediaImageView.svelte` an `<img>`.

## Shape of the end state

- **`crates/cmdr-viewer`** (named after the UI): everything in `file_viewer/` except the app-kept files in rule 7, with
  `media_protocol.rs`'s pure half as a framework-free `media_serve` (a plain status / headers / body answer the app
  converts to `tauri::http::Response` and the demo turns into a `Blob`).
- **Host seams, one trait per concern**, in the crate's `host/` like `crates/cmdr-listing/src/host/`, each with a no-op
  default: `host/volumes.rs` (which volume serves an id, whether a path routes over its parent), `host/pulls.rs` (the
  materialize call and a Markdown image's `MediaSource`), `host/events.rs` (file changed, media load, pull progress,
  each carrying the window label where desktop uses `emit_to`), `host/remote.rs` (remote image download),
  `host/analytics.rs` (`viewer_opened` facts). The log display transform is pure (`redact/markers.rs` `display_line`),
  so it's a `fn(&str) -> Cow<str>` field on the `Viewer`, not a trait. The app implements them in `file_viewer/` beside
  the kept files, the WASM engine over its `InMemoryVolume`.
- **The app owns the wire types of its events** (the `cmdr-listing` pattern: the engine reports facts through
  `host/events.rs`, `listing_host/events.rs` turns them into `tauri_specta::Event` structs). `ViewerMediaLoad`,
  `ViewerPullProgress`, and the `viewer:file-changed` payload stay app structs with their derives, so `bindings.ts`
  can't grow a same-named crate type. The browser host mirrors the same JSON, pinned against `bindings.ts`.
- **One worker module** in the crate: `spawn(job) -> JoinHandle` and `after(delay, job)`. Natively exactly today's
  `thread::spawn` (+ `thread::sleep`), the handle a `std::thread::JoinHandle`. In the browser `spawn` runs the job
  INLINE and hands back a finished handle: a page has one thread, so a search started in `spawn_local` would block the
  page anyway, and inline means `search_start` returns with the search done and the first `search_poll` answers it.
  `after` uses `rt`'s `spawn_local` + `rt::time::sleep`. The search watchdog (a sleep loop that `join`s the worker) is
  native-only; in the browser a cancel can't interrupt a running search (demo files are small; document it in the
  crate's `DETAILS.md`).
- **Watcher** is a native-only module of the crate (`cfg(not(target_family = "wasm"))`, the `cmdr-fs` gating style); a
  wasm session is never watchable.
- **The demo** reads files through the real `PositionedRead::Held` path: `InMemoryVolume` gains `open_random_reader` (a
  test win too: `volume_source` tests run without SMB), and the viewer's sync-over-async bridge gets a browser answer
  for a future that's already ready.

## Milestones

### 1. Characterize the boundary

- **Scope**: pin what a move or a second host could silently change, against today's code, in the app crate:
  - Wire snapshots (serde JSON, ids and tokens masked) of every command answer type and error: `ViewerOpenResult` per
    kind (text FullLoad, text ByteSeek, Markdown, image, PDF, as-text override, log view), `LineChunk`, bytes,
    `SearchPollResult` (each status), `EncodingOptions`, `ViewerSessionStatus`, `MarkdownRender`, `MarkdownLink`,
    `SavedAs`, every `ViewerError` variant, and the `viewer:file-changed:<sid>` payloads (`grew`, `rotated`).
  - The open-routing table: local path, held-reader volume, ranged-media volume, routed path (materialize), a dead DFS
    link, a missing file, a folder. Per case: origin kind, content kind, watcher yes/no, temp yes/no, analytics facts.
  - `media_protocol` answers: status, headers, body for no range, a range, an open-ended range, an unsatisfiable range,
    an unknown token, a dropped token.
  - Events: `ViewerMediaLoad` in every phase (`Reading`, `Done`, `Failed`) and `ViewerPullProgress`, each with its
    `emit_to` target (the window label), through the pure emit closures `serve_volume` and `open_pulling` already take.
  - Search: match rows, `column` and `length` in UTF-16 units (a non-BMP line, a CRLF line, a regex match), and
    `SearchPollResult` across start, poll, cancel.
  - Markdown at session level: `viewer_render_markdown`'s HTML with image sources rewritten to tokens and links
    rewritten, through `open_session` on a fixture document (tokens masked).
  - Log view rows (`RowText::LogDisplay`): markers gone, values shown.
  - For each item, either name the existing test that already pins it (in the milestone's commit body) or add one.
- **Intentions**: these tests are the proof for rule 1. Pin reality, including anything that looks odd; flag oddities to
  the lead, don't fix them here. Reuse `row_characterization_test.rs`, `session_test.rs`, `media_protocol_test.rs`,
  `analytics_test.rs` where they already pin; add only the gaps.
- **Landmines**: snapshots that embed a temp path, a token, or a session id flake or leak; mask them. Timing-sensitive
  pins (search status mid-run) need a scripted backend, not sleeps (`src-tauri/CLAUDE.md` test rules).
- **Test plan**: `cargo nextest run` on the new tests; break one pinned field by hand once to see it bite.
- **DONE**: every shape above pinned, green, committed on its own.

### 2. Seams in place, still inside the app

- **Split into four commits, each a reviewable step that keeps milestone 1's pins green**: (a) the bridge, the worker
  module, and the clocks; (b) the host traits and the app's implementations, including the event wire types staying
  app-side; (c) the `media_serve` split; (d) `Viewer` and `MediaTokens` as values. The bullets below say what each
  covers.
- **Scope**: in `file_viewer/`, without moving a file:
  - The host traits (above) and the app's implementations; `init_app_handle` becomes the host install. `origin.rs`,
    `markdown_session.rs`, `markdown_images.rs`, `session.rs`, `row_text.rs`, `log_view.rs` call the host, never
    `crate::` app modules.
  - The worker module; every `thread::spawn` / `thread::sleep` in crate-bound files goes through it.
  - Clocks through `cmdr_fs::rt` (`media_volume.rs`, `pending_open.rs`, the search watchdog).
  - `expand_tilde` and `ignore_poison` from `cmdr_fs`; `map_volume_error` out of `materialize.rs` (it's used by
    `origin.rs`) into the crate-bound error module.
  - `media_protocol.rs` split: the pure `media_serve` beside the token map; the Tauri shell stays.
  - The sync-over-async bridge in `volume_source.rs` / `markdown_images.rs` behind one function (natively
    `tauri::async_runtime::block_on`'s exact behavior; see landmines). `HeldReader::drop`'s
    `tauri::async_runtime::spawn` (the reader's async close) goes through `cmdr_fs::rt` on the host's runtime
    (`spawn_local` in the browser).
  - A Markdown document's local image: `VolumeImage::fetch` (`markdown_images.rs`) pulls it to a temp through
    `materialize_markdown_image` and serves it as a path-based `MediaEntry`. That becomes a host call returning a
    `MediaSource`: the app keeps today's pull for every volume (moving SMB and the rest to positioned reads is a
    behavior change that needs David), and the browser host answers `MediaSource::Volume(VolumeMedia)` over the
    in-memory volume.
  - The viewer as a value (Decision 1, decided): a `Viewer` owns the session table, the window map, the pending opens,
    and the host. The media token table is its OWN shared value (`MediaTokens`), because thumbnails mint and drop tokens
    outside any viewer session (`commands/media_index/thumbnail.rs`) and the one scheme handler serves both. The
    `Viewer` holds an `Arc<MediaTokens>`; the app keeps one process-wide instance of each in managed state, reachable
    from every call site that reaches a static today: `commands/file_viewer.rs`, `app_lifecycle.rs`
    (`close_session_for_window` on `Destroyed`), the scheme closure in `tauri_builder.rs`, `thumbnail.rs`, `lib.rs`'s
    three `init_*`, `tools/read/inspect/*` (headless calls need no instance; check each), and the tests in
    `adb/volume_wiring_test.rs` and `commands/file_viewer_test.rs`. List them fresh by `codegraph_callers` before
    starting.
  - The event structs (`ViewerMediaLoad`, `ViewerPullProgress`) move to the app's host side with their
    `tauri_specta::Event` derives; the crate-bound code reports through `host/events.rs`.
  - `PreviewTemp`: the crate-bound session holds a handle type the crate defines; creating and deleting the temp stays
    with `materialize.rs`.
  - Logic in `commands/file_viewer.rs` that is more than a pass-through (window registration, `open_may_materialize`
    gating, `follow_markdown_link`'s external branch) moves under `file_viewer/` so both hosts share it.
- **Intentions**: make the move in milestone 3 a pure `git mv` plus import edits. A reviewer reads this milestone's diff
  as "same behavior, new seams". The value conversion is its own commit inside this milestone, after the seams, so its
  diff reads as call-site plumbing only.
- **Landmines**:
  - `tauri::async_runtime::block_on` is `Runtime::block_on` on Tauri's global runtime; a `Handle::block_on` replacement
    behaves the same on a multi-thread runtime from a non-async thread, and both panic inside an async context. Every
    caller today runs on a `spawn_blocking` or plain OS thread; keep it so, and say so in the bridge's doc comment.
  - ❗ Never resolve an ambient runtime in the bridge (`Handle::current()`, `try_current`, or an `rt` call that does): a
    `HeldReader` drops on a plain worker thread (a search, the tail catch-up) where there is none, and that panics. The
    `Viewer` stores the runtime its host hands over at creation (the app: Tauri's, as `listing_host/` does) and the
    bridge spawns and blocks on exactly that. The bridge's seam commit keeps `tauri::async_runtime` (the global, safe on
    any thread) until the `Viewer` holds the handle.
  - The event structs keep their names (the event name derives from them) and serde shapes, and no crate type may share
    a name with one (specta would emit both into `bindings.ts`); `bindings.ts` zero diff is the gate.
  - `close_session` is the one choke point that drops `SESSIONS`, the media token, and the temp; the host seams must not
    add a second path.
  - The FSEvents subscribe stays on the watcher manager thread, never inline in `open_session`.
- **Test plan**: milestone 1's pins unchanged; the full `file_viewer` and `tools::read::inspect` suites;
  `pnpm check clippy`; `pnpm check desktop-e2e-playwright` viewer specs (`viewer-media.spec.ts` included).
- **DONE**: the crate-bound files import nothing from `cmdr_lib` but `file_viewer` itself; zero `bindings.ts` diff.

### 3. Move into `crates/cmdr-viewer`

- **Scope**: `git mv` the crate-bound files and their tests; the app's `file_viewer/` keeps the host implementation and
  the rule-7 files; consumers import from `cmdr_viewer`. Test scaffolding the app's tests use (`ScriptedBackend`,
  `test_only_install_session`, `active_read_count`, `wait_for_watcher_subscribed`, the upgrade gate) goes behind
  `any(test, feature = "testing")`. Tests needing the app's volume manager, `volume_host`, git, or restic stay in the
  app as host integration tests over the crate's public API. Wiring: workspace member, `index-crate-isolation`'s crate
  list (and its ceilings entry), `scripts/check/checks/inputs.go` `Pkg`, `clippy.toml` with the `rt` bans plus a
  `std::thread::spawn` ban outside the worker module, `AGENTS.md` crate list, `docs/architecture.md`.
- **Intentions**: the crate's `CLAUDE.md` + `DETAILS.md` take the viewer's must-knows and the 100 KB `DETAILS.md`
  material about the core; the app's `file_viewer/` C+D.md keep only the host, the scheme, Open with, materialize, and
  analytics. Single-source: each claim moves, never copies.
- **Landmines**:
  - `cfg(test)` items in the crate are invisible to the app's tests; that's what the `testing` feature is for.
  - Implicit log targets change from `cmdr_lib::file_viewer::…` to `cmdr_viewer::…`; the 14 explicit
    `target: "cmdr_lib::file_viewer"` strings should follow, and `docs/tooling/logging.md`'s `RUST_LOG` recipe with
    them, or the recipe silently goes quiet.
  - `file-length` and `claude-md-length` may fire on the split docs; split or bump with a reason.
  - `jscpd` may see moved test fixtures as duplicates if a copy is left behind; leave none.
- **Test plan**: milestone 1's pins (now partly in the crate) unchanged; `cargo nextest run -p cmdr-viewer` and the
  app's viewer and inspect suites; `pnpm check --fast`, `clippy`, `svelte-tests-related`; the viewer E2E specs.
- **DONE**: `crates/cmdr-viewer` builds natively with no `tauri` in its graph; desktop behavior and bindings unchanged.

### 4. The crate builds for `wasm32-unknown-unknown`

- **Scope**: `cargo build -p cmdr-viewer --target wasm32-unknown-unknown`, with:
  - `LocalFileSource` / `open_local` and the watcher behind `cfg(not(target_family = "wasm"))`; a wasm session whose
    origin is local is a typed error, never a panic. Gate the local fallback in `open_session_core` BEFORE its
    `file_path.exists()` check: when `open_through_volume` answers `Ok(None)` (a volume open error falls back there,
    `origin.rs`), the browser would otherwise report every such file as `NotFound`, since `exists()` is always false
    there. The browser answers the volume's own typed error instead; pin it in the smoke.
  - The worker module's and the bridge's browser halves. The bridge: poll the future once and answer a typed error if
    it's pending (a page can't block). That holds only for an UNPACED read: the write effort's `InMemoryVolume` gains
    device pacing (`with_bandwidth` sleeps, `with_yielding` yields every 8 ms), so the viewer reads through its unpaced
    positioned reader and media path (the write effort owns that API, see "Parallel efforts"), and the difference goes
    under "Where the demo differs from the app".
  - Browser jobs run inline, so nothing may scan a whole big file inside one call: in the browser a session never
    upgrades ByteSeek to LineIndex (no `upgrading`, the estimate stays), and an encoding switch on a file past FullLoad
    takes the ByteSeek path. A search does scan to the end inline, with no cancel; it stays bounded by the demo files'
    sizes, which the smoke pins (a search on the largest demo file finishes under 200 ms). Chunking a search with
    `yield_now` would need the scan to be async end to end; not worth it for a demo.
  - The search watchdog is native-only (`cfg`), and `worker.rs` gets a test that `spawn` runs the job on another thread
    natively.
  - `rand`'s `getrandom` backend for the browser (or mint tokens through a source that already works there, keeping the
    32-lowercase-hex, 128-bit format).
  - Remote Markdown images: native-only download; in the browser, the host refuses (the demo's CSP allows no network).
  - `InMemoryVolume::open_random_reader` in `cmdr-fs`, with conformance tests.
  - A dedicated check, `desktop-rust-viewer-wasm`:
    `cargo build -p cmdr-viewer --target wasm32-unknown-unknown --profile wasm-release`, in CI's "Desktop (Rust)" job,
    registered in `workspace-member-coverage`. The listing-wasm check can't gate this milestone: it only pulls the
    viewer in once milestone 5 adds the dependency. Once it does, keep the dedicated check only if it still catches
    something the smoke doesn't, else fold it.
- **Intentions**: the browser traps are runtime-only (`crates/cmdr-listing-wasm/CLAUDE.md`), so a compile is not proof.
  Milestone 5's smoke run is.
- **Landmines**:
  - `std::thread::spawn` and `thread::sleep` compile and then panic; `Instant::now` / `SystemTime::now` panic;
    `cmdr_fs::deadline`'s `StallWatch` reads `tokio::time::Instant::now` directly, so the browser's media serve must not
    go through it.
  - `Path::is_absolute` is always `false` there, and `PathBuf::push` decides replace-vs-append on it: Markdown link and
    image resolution (`markdown_links.rs`, `markdown_images::resolve_image_src`) may resolve differently. Pin with a
    native test of the inputs and run the same cases in the smoke.
  - getrandom 0.3+ needs an explicit wasm backend (a feature, and on some versions a `--cfg`); verify on the locked
    version before choosing.
  - Measure the `.wasm` size before and after (about 2 MB today, 2026-11-04). `regex`, `ammonia` (html5ever),
    `encoding_rs`, and `image` all add weight; report the number, and trim features only if it's large.
- **Test plan**: `pnpm check desktop-rust-viewer-wasm`; native suites unchanged.
- **DONE**: `cmdr-viewer` compiles for wasm32 with no native-only dep in that graph, and desktop is untouched.

### 5. The WASM engine hosts the viewer

- **Scope**: in `crates/cmdr-listing-wasm`, a `viewer` module beside `commands.rs`: the viewer commands by their Tauri
  names (`viewer_open`, `viewer_open_as_text`, `viewer_get_lines`, `viewer_get_bytes`, `viewer_render_markdown`,
  `viewer_follow_markdown_link`, `viewer_get_status`, `viewer_search_start` / `_poll` / `_cancel`, `viewer_close`,
  `viewer_read_range`, `viewer_cancel_read`, `viewer_get_encoding_options`, `viewer_set_encoding`, `viewer_reload`),
  with typed refusals for `viewer_write_range_to_file`, `viewer_allow_remote_images`, and `viewer_set_tail_mode`
  (enabled); `close_session_for_window`; and a media body call (token in, MIME plus bytes out, from `media_serve`). The
  host serves the SAME `InMemoryVolume` the listing reads, so a file the write engine later changes is the file the
  viewer shows. The contents come from the write effort's content model (stored real bytes per entry, or synthetic bytes
  that are a pure function of seed and offset, so any positioned read works); this effort picks which demo files carry
  real bytes from repo assets, and touches `tree.rs` only after the write effort's content-model commit is on this
  branch (the lead merges it) or through the lead.
- **Media tokens report their drops**: `MediaTokens` tells `host/events.rs` when a token drops (a session close, a
  Markdown re-render, a link follow), so the browser host can revoke that token's `blob:` URL. The app's events host
  ignores it.
- **External links**: an opener seam (`host/opener.rs`); the app opens through `tauri_plugin_opener` as
  `commands/file_viewer.rs` does today, the browser host through a JS callback the page passes
  (`window.open(url, '_blank', 'noopener')`).
- **Intentions**: "same wire as the desktop": answers, rejections, and camelCase args mirror `commands/file_viewer.rs`,
  pinned against `bindings.ts` the way `events_test.rs` pins the listing's events. Keep the viewer's surface in its own
  files so the menu and write efforts don't conflict on `commands.rs`.
- **Landmines**:
  - The command layer's 2 s timeouts and stall watches are native; the browser host answers inline. List the differences
    in `cmdr-listing-wasm/DETAILS.md` § "Where the demo differs from the app".
  - Contents cross JS → wasm as bytes, not JSON numbers (a `Uint8Array` per file, or one buffer plus offsets); a JSON
    array of numbers would multiply the boot cost.
  - Window close must reach `close_session_for_window`, or sessions and tokens leak for the life of the page.
  - The demo's media is `mediaFromVolume: true` (the session reads through `SourceOrigin::Volume`), so the page shows
    the progress panel until `viewer-media-load` says `Done` (`routes/viewer/+page.svelte`). ❌ Don't fake `false`. The
    media body call runs the crate's volume serve loop without the `StallWatch` / `blocking_typed_result_until_stalled`
    wrapper and emits `ViewerMediaLoad` through the host event seam.
  - `EventSink::emit(event, payload)` (`events.rs`) has no target, but `ViewerMediaLoad` (and `ViewerPullProgress`) go
    `emit_to(window label)` on desktop. Give the sink an optional label, mapped to the page's `EventTarget`
    (`lib/demo/backend.ts`), so only the asking viewer panel hears it.
- **Test plan**: native tests of the module (`cargo nextest run -p cmdr-listing-wasm`); `smoke.mjs` opens a text file,
  walks rows, searches (start, poll to done), switches encoding, reads hex bytes, renders Markdown and follows a
  relative link, loads a Markdown document's local image through its token, opens an image and a PDF and fetches their
  bodies (hearing `viewer-media-load` `Done` for the right label), answers a volume read error with its typed error (not
  `NotFound`), mints two tokens that differ, hears one drop per minted token after closing by window label, follows an
  external link through the opener callback, and searches the largest demo file within its budget.
- **DONE**: the smoke drives every listed command against the real `.wasm` in Node; the wire tests pass.

### 6. The viewer window in the demo: text, Markdown, hex

- **Scope**: `lib/demo/windows.ts` opens `viewer-*` labels (the viewer's labels are `viewer-<timestamp>`, so the
  allowlist matches the prefix, and several can be open); the backend routes viewer commands to the engine; window and
  menu plumbing the viewer route calls (`viewer_setup_menu`, `viewer_set_word_wrap`, `viewer_set_search_input_focused`,
  `viewer_set_history_state`, title and size calls) get void answers in TS; `tauri://destroyed` for a `viewer-*` panel
  closes its sessions. The demo tree (`demo-tree.ts`) gains real contents for its text-like files (Decision 3).
- **Intentions**: the frontend route runs unmodified. Drive it with `window.__cmdrDemo.unanswered` until it's empty for
  a full session: open, scroll, search, encoding switch, word wrap, view-mode switches (text, hex, binary), copy, Esc.
- **Landmines**:
  - Browser-reserved keys inside the viewer frame (⌘F, ⌘G, ⌘W, F3 in some browsers): the main frame's remaps
    (`browser-shortcuts.ts`) must cover the viewer frame too, through the same layer, not a viewer-only copy.
  - The viewer's clipboard and save paths: copy goes through whatever clipboard answer the main window uses; "Save
    selection" refuses typed, never a silent success.
  - The viewer polls `viewer_get_status` for indexing; the browser never indexes, but a typed answer must still arrive.
  - **Demo events are delivered async** (`queueMicrotask` in `lib/demo/backend.ts` / `webview.ts`), as Tauri events
    always are. A synchronous delivery during an `invoke` lands a `viewer-media-load` `Done` inside the `$derived`
    `mediaSrc` (`viewer-media.svelte.ts`), and Svelte 5 throws `state_unsafe_mutation`. Pin it in Vitest, and tell the
    lead when it lands so the write effort knows.
- **Test plan**: Vitest for `windows.ts` (prefix allowlist, destroy closes sessions) and the routing; the demo build
  plus a scripted browser pass (agent-browser) with screenshots of each view in light and dark; `pnpm check --fast`,
  `svelte-tests-related`.
- **DONE**: a viewer panel opens from F3 on a text or Markdown file and every text-path feature works with no unanswered
  command.

### 7. Media in the demo: images and PDF

- **Scope**:
  - `mediaUrl(token)` builds its URL through `convertFileSrc(token, 'cmdr-media')` (`@tauri-apps/api/core`), which
    yields exactly `cmdr-media://localhost/<encodeURIComponent(token)>` on macOS (and the right
    `http://cmdr-media.localhost/…` on Windows). The demo's `webview.ts` implements `convertFileSrc` for that protocol
    as a `blob:` URL over the engine's media body, cached per token and revoked when the engine reports the token
    dropped (a Vitest pin: mints and revokes balance after a close).
  - A lint (or a check) refusing the `cmdr-media://` literal outside `media-view.ts`, so `mediaUrl` stays the only way.
  - The demo CSP (`apps/website/nginx-demo-security-headers.conf`) gains `blob:` in `object-src` for the PDF `<embed>`
    (it's `'none'` today; `img-src` already allows `blob:`).
  - Demo contents: images and a PDF (Decision 3).
- **Intentions**: one media path in the app, two hosts under it. The token is still the only way to name bytes.
- **Landmines**:
  - The desktop change must stay byte-identical: verify the URL string is unchanged on macOS, the CSP's `cmdr-media:`
    still covers it, and `viewer-media.spec.ts` stays green. `convertFileSrc` reads `__TAURI_INTERNALS__`, which the
    viewer's own unit tests may need mocked.
  - `blob:` URLs minted in the top frame are usable in a same-origin iframe; verify in Safari, Chrome, and Firefox,
    including each one's built-in PDF rendering inside an `<embed>` in an iframe.
  - The panel waits for `viewer-media-load` `Done` (milestone 5 emits it, `mediaFromVolume` stays true); the `blob:`
    must exist before the `<img>` / `<embed>` asks, so `convertFileSrc` fetches the body synchronously and the load
    event follows it.
- **Test plan**: Vitest for `mediaUrl` (string equality with the old form on macOS) and the demo's `convertFileSrc`; the
  desktop viewer media E2E; the demo build in all three browsers with an image, an SVG, and a PDF.
- **DONE**: images and PDFs render in the demo viewer, and the desktop media path is unchanged.

### 8. F3 and Enter open files in the demo

- **Scope**: F3 opens the viewer (falls out of milestone 6). Enter on a file: the demo's `openPath` answer (today a
  blanket `unsupported` refusal, which the app turns into "macOS couldn't find the file, or an app that opens it") opens
  the viewer instead, through the existing `open-file-viewer` event the main window already listens for, and resolves.
  That retires the misleading toast for every file the viewer can show (all of them: binary falls to hex). Update
  `lib/demo/DETAILS.md`, the website's demo notes, and `website-demo-key.sh` + `websiteDemoInputs` + ci.yml's `website`
  filter for every new input path (`crates/cmdr-viewer`, and any asset dir referenced in place).
- **Intentions**: the demo's "default app" is Cmdr's own viewer, which is honest in a browser with no other apps.
- **Landmines**: `openPath` is also reached for folders and packages in some flows; only files go to the viewer.
  `TestWebsiteDemoInputsMatchTheImageKey` fails if the key script and `inputs.go` disagree.
- **Test plan**: Vitest for the `openPath` answer; the demo pass: Enter and F3 on each kind; `pnpm check` (backgrounded)
  and `--only-slow` before the batch lands; website E2E against a local demo build.
- **DONE**: F3 and Enter open every demo file in the viewer, nothing in the demo says an app is missing, and the demo
  key covers every input.

## Parallel efforts: dependencies and collisions

- **`cmdr_fs::rt`** (write engine, #929 steps 2–3): this plan may add a browser answer for sync-over-async (a ready-only
  `block_on`) and possibly a timer helper. #929's thread and `block_on` audit may want the same thing. Whoever lands
  first defines it; the other adopts it. Talk before adding.
- **`cmdr-listing-wasm`**: the menu crate (step 2), the write engine (step 3), and this plan all add commands, deps,
  smoke steps, and `DETAILS.md` rows. Each effort keeps its surface in its own module, with one registration line in
  `commands.rs` / `bindings.rs`; conflicts get resolved once in the batch land.
- **`InMemoryVolume`** (`cmdr-fs`), decided by the lead: the write effort owns its content model and pacing. It offers
  stored real bytes per entry, synthetic content whose bytes are a pure function of (seed, offset), unpaced positioned
  reads (`open_random_reader`), and an unpaced path for the viewer's media serve. This effort consumes that API (and
  adds `open_random_reader` only if the write effort hasn't yet, in its shape), and owns which demo files get real
  stored bytes. The viewer reads through the same volume, so a demo write followed by F3 shows the new bytes: a test
  worth adding once both land.
- **`website-demo-key.sh`, `inputs.go` `websiteDemoInputs`, ci.yml `website` filter**: every effort appends crate paths
  to the same lists. Trivial conflicts; the test that ties them together catches a miss.
- **`lib/demo/windows.ts`**: step 4 adds `queue`, this plan `viewer-*`. **`lib/demo/answers.ts`**: this plan changes
  `openPath`; the write engine retires `writes.ts`.
- **Menu crate**: the viewer's native menu (`viewer_setup_menu` and friends) lives in `menu/`; if the menu crate moves
  the viewer's menu spec, the demo could later render it, but this plan answers those commands as no-ops.
- **Demo assets and the demo tree**: `demo-tree.ts` is PLACEHOLDER content David decides; any other agent editing it
  (assets, tree shape) must be coordinated before milestone 6.
- **Website CSP** (`nginx-demo-security-headers.conf`): coordinate with whoever owns the website demo work.

## Decisions for David

David took every recommendation below on 2026-11-04: the viewer as a value, `cmdr-viewer`, (a) for the contents, Enter
opens the viewer for every file, and the engine rename waits until all three efforts land.

1. **Session table**: (a) the viewer becomes a value (`Viewer`, owning its sessions, tokens, and host), like
   `cmdr-listing`'s "the host is a value, never a static" (recommended: the ideal end state, test-isolated, one engine
   per host; done in milestone 2 before the move, at the cost of touching every call site), or (b) keep today's
   process-wide statics with a host installed once (smaller diff, same behavior).
2. **Crate name**: `cmdr-viewer` (recommended, after the UI) or `cmdr-file-viewer` (after today's module).
3. **Demo contents**: (a) every demo file gets real contents and a true size, generated text for text-like files,
   in-repo art for images (`brand/screenshots/*.webp`, `brand/logos/cmdr.svg`), and one PDF we make (recommended; the
   1,200-photo folder shares one small image's bytes, so its sizes become small), or (b) only some files get contents
   and the rest open as zero bytes in hex. Any prose in the files (Markdown, notes, the PDF) is copy you review.
   `apps/desktop/test/e2e-shared/media-fixtures/sample.pdf` is a candidate if its origin is ours.
4. **Enter in the demo** opens the viewer for every file (recommended), or only for kinds it renders (text, Markdown,
   image, PDF) with binary files keeping a clearer refusal.
5. **Engine name**: `cmdr-listing-wasm` will host listing, menu, writes, and the viewer. Rename it (for example
   `cmdr-demo-engine`) once all three efforts land, as its own commit.

## Invariants register

- `bindings.ts` regenerates with zero diff after every milestone through 5; milestone 7's `mediaUrl` change is the only
  intended desktop source change outside Rust, and its output string is identical on macOS.
- The crate has no `tauri`, `tauri_specta`, or `cmdr_lib` in its dependency graph (`index-crate-isolation`).
- Every background job in the crate goes through the worker module; every clock read through `cmdr_fs::rt`; clippy bans
  the rest.
- Natively, the same OS threads run the same jobs with the same sleeps and timeouts as before (`worker.rs`'s thread-id
  test).
- The bridge never resolves an ambient runtime; it uses the one the `Viewer` was handed.
- `close_session` stays the one choke point freeing a session, its media tokens, and its temp; both close paths (IPC and
  window destroyed) reach it in both hosts, and every token drop reaches the events host, where the demo revokes its
  `blob:` URL (Vitest and smoke pins).
- A media URL is built only by `mediaUrl(token)` (a lint on the literal); a token is 128-bit and unguessable in both
  hosts (the smoke checks two differ). A viewer token lives as long as its session (a Markdown image's, until the next
  render); a thumbnail's lives until the main window drops it.
- Demo events reach listeners asynchronously, as Tauri's do.
- ammonia's allowlist, the cross-line regex rejection, the ISO-8859-1 table, rows-not-lines, and UTF-16 search columns
  are untouched (the crate's `CLAUDE.md` carries them).
- No `if (demo)` in app code, no viewer logic in TS, no demo code in a non-demo build.
- The demo never reports a write or a save it didn't make.
- No milestone tags outside this file.
