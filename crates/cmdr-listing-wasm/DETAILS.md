# cmdr-listing-wasm details

The browser build of the listing engine, for the website's `/demo` (#302). Must-knows: `CLAUDE.md`. The engine's side of
the seam it needs: `crates/cmdr-listing/DETAILS.md` § "The browser build"; `cmdr-fs`'s gates:
`crates/cmdr-fs/DETAILS.md` § "The browser build".

## Decision: `wasm32-unknown-unknown` + `wasm-bindgen`, on the page's event loop

**Decision:** the target is `wasm32-unknown-unknown` with `wasm-bindgen`. The engines' executor and clock go through one
seam, `cmdr_fs::rt`: natively it re-exports `tokio::runtime::Handle`, `tokio::time`, and `std::time::Instant` (so
desktop code is the same code), and in the browser it spawns with `wasm_bindgen_futures::spawn_local`, runs blocking
work inline, reads `performance.now()` (`web-time`), and sleeps on `setTimeout`.

**Why:** what the engine needs from its host is "run this future", "run this closure", "what time is it", and "wake me
in 50 ms". A browser answers all four natively, and none of them needs a thread. Options considered (verified on rustc
1.99.1, tokio 1.x, 2026-11-04):

- **`wasm32-unknown-unknown`, tokio current-thread runtime pumped from JS.** Rejected: tokio's time driver calls
  `std::time::Instant::now`, which panics there, so every `tokio::time::sleep` would trap; and a runtime only runs while
  someone `block_on`s it, which in a page means a pump loop guessing when the queue is empty.
- **`wasm32-wasip1` plus a browser WASI shim.** Rejected: `std`'s clock works there, but nothing else gets easier. Still
  no threads (`spawn_blocking` still panics), tokio's timer would park the page's only thread, `rusqlite`'s bundled C
  needs a WASI C toolchain, and the JS boundary would be a hand-written pointer ABI instead of `wasm-bindgen`.
- **`wasm32-unknown-unknown` with atomics and `wasm-bindgen-rayon`-style threads.** Rejected: needs nightly
  `-Z build-std`, cross-origin isolation headers on getcmdr.com, and a worker pool, all for a demo whose reads are
  in-memory and instant.

The chosen seam costs ten call sites in the engine (types renamed, bodies untouched) and adds no crate to the native
graph: `wasm-bindgen`, `wasm-bindgen-futures`, `js-sys`, and `web-time` were already in `Cargo.lock` (other crates'
wasm-only deps), and every new edge is under `cfg(target_family = "wasm")`.

**`Listings::flush_pending_diffs` wasn't the answer** for the diff flush: it's a test hook that bypasses the 50 ms
coalescing, so a demo using it would behave unlike the app. The real `setTimeout` flush keeps the behavior.

## The JS API

`wasm-bindgen --target web` output, one ES module. All JSON crosses as JS values (`JSON.stringify` in, `JSON.parse`
out), so answers are exactly what Tauri's `invoke` resolves with.

- `default(init?)`: the glue's initializer. In a page `await init()` fetches `cmdr_listing_wasm_bg.wasm` next to the
  glue; Node passes `{ module_or_path: bytes }`.
- `new DemoListing(tree, onEvent, contents?)`: `tree` is
  `{ volumeId, volumeName?, home?, freeBytes?, root, entries: [{ path, kind: "file" | "dir" | "symlink", size?, modifiedMs?, createdMs?, symlinkTarget? }] }`,
  every parent folder listed. Throws on a bad tree (a relative path, a duplicate, a missing parent). `contents` is
  `{ [path]: Uint8Array }`: those files' real bytes (and sizes), crossing as bytes, never JSON numbers; every other file
  reads bytes generated from its path. `onEvent(name: string, payload: unknown, target?: string)` hears every event by
  its Tauri name: `listing-opening`, `listing-progress`, `listing-read-complete`, `listing-complete`, `listing-error`,
  `listing-cancelled`, `listing-stalled`, `directory-diff`, `listing-respelled`, `directory-deleted`, and the viewer's
  `viewer-media-load`, the one with a `target` (the window label the app's `emit_to` names). Two demo-only events:
  `demo://media-token-dropped` `{ token }` (revoke that token's `blob:` URL) and `demo://open-url` `{ url }` (a followed
  web or mail link).
- `demo.mediaBody(token, windowLabel)`: a viewer media token's `{ mime, bytes: Uint8Array }`, or `undefined` (the app's
  404). That window hears `viewer-media-load` `Done`. `demo.closeViewerWindow(windowLabel)`: frees the window's sessions
  and tokens, the app's `Destroyed` path.
- `demo.invoke(command: string, args: object): Promise<unknown>`: a listing command by its Tauri (snake_case) name with
  its camelCase args. Resolves with the command's answer, rejects with its serialized error
  (`{ type: "gone", listingId }`), or a string for args that don't parse or a command it doesn't answer.
- `DemoListing.commands(): string[]` and `DemoListing.handles(command): boolean`: which commands `invoke` answers
  (`commands.rs`'s `COMMANDS`), so a mock routes those here and fakes the rest. Beside the listing commands, that's
  Brief mode's widths (`get_brief_column_text_widths`) and the font metrics behind them (`store_font_metrics`,
  `extend_font_metrics`, `has_font_metrics`), answered by `cmdr-brief-columns`, the code the desktop's commands run. The
  page measures its own fonts with the Canvas API and stores them, as the desktop's frontend does. Beside those, the
  viewer's, search's, writes', and Multi-Rename's commands (§ Multi-Rename).
- `demo.importPlan(items): Promise<ImportPlan>`, `demo.importFile(batch, index, bytes: Uint8Array): Promise<null>`,
  `demo.importHandOff(batch, offered)`, and `demo.importReleaseOffered()`: files dropped in from the computer (§ Files
  dropped in from the computer). Both reject with an `ImportError`
  (`{ kind: "badPath" | "unknownItem" | "sizeChanged" | "volume", ... }`).
- `DemoListing.setLogLevel(level)`: the engine's `log` lines on the console, `"warn"` by default.
- `new DemoMenuBar()`: the menu bar (`menu.rs`). `DemoMenuBar.handles(command)` and `menu.invoke(command, args)` take
  the desktop's menu-state pushes by their Tauri names (`activate_window_menu`, `set_commands_refused_over_dialog`,
  `update_menu_accelerator`, `update_sort_menu`, `update_view_mode_menu`, and the rest of `MENU_COMMANDS`) and store
  them, as `commands/menu_state.rs` stores them in `MenuState`. `menu.bar()` answers the macOS bar as it stands:
  `[{ id, title, rows }]`, each row `item` / `check` / `submenu` / `system` / `separator` with its label (a catalog key
  plus tokens, or the app menu's literal `Cmdr`), its shortcut in a Mac's glyphs, and its enabled and checked state, all
  from `cmdr-menu`. `menu.click(id)` answers what the click sends: `{ kind: "emit", event, payload, to }`
  (`execute-command`, `settings-changed`, `view-mode-changed`, `menu-sort`, the payloads `handle_menu_event` emits, `to`
  the target window's label), `nativeEdit`, `closeFocusedWindow`, or `nothing` for a greyed or OS row.
- The right-click menus are the same object (`context_menu.rs`). `DemoMenuBar.opensPopup(command)` claims the
  `show_*_context_menu` commands (`POPUP_COMMANDS`), and `menu.popup(command, args, windowLabel)` stashes what the menu
  went up over (the row's path, the eject target, the host row, the window), as `commands/menu.rs` stashes it in
  `MenuState`, and answers `{ rows, anchor }`: `cmdr-menu`'s rows for those args, plus a `header` row kind, minus every
  OS-filled `Slot`. `update_menu_context` is one more `invoke` push. `menu.contextClick(id)` answers a pick through
  `cmdr_menu::context::click::context_click`: an `emit` to `main`, `queue`, or the viewer the menu went up over, an
  `invoke` (`add_favorite`, as `handle_menu_event` calls it directly), or `nothing` for a row the last popup didn't
  offer live. What the demo has no OS for: no slot, no image-search group, a breadcrumb volume never busy and always
  "Eject", a servers hub host with nothing mounted.

The volume is built `with_held_folders_only()`, so a folder the tree doesn't hold ends in the typed `notFound`
`listing-error` a disk's missing folder does (and a file, `NotADirectory`). The tree: dates are JS milliseconds and
become `FileEntry`'s seconds; files get mode `0644`, folders `0755`; a symlink whose `symlinkTarget` is a folder in the
tree lists as a folder (as on disk).

## The build

- `pnpm build:listing-wasm` (`scripts/build-listing-wasm.sh [-out DIR] [-smoke] [-golden]`, a Go program):
  `cargo build -p cmdr-listing-wasm --target wasm32-unknown-unknown --profile wasm-release` (plus the crates the browser
  build is about to link, `browserBound` in `main.go`, so a host-only dep can't creep into them first; empty while every
  such crate is linked), then `wasm-bindgen --target web`, then `wasm-opt` (below).
- **The C toolchain**: `cmdr-fs` links SQLite (`rusqlite`, which in the browser swaps in `sqlite-wasm-rs` and compiles
  `sqlite3.c` with `cc`). Apple's clang can't target wasm32, and `sqlite-wasm-rs` 0.5.5 has no precompiled option, so
  the script installs conda-forge's LLVM clang and llvm-ar (pinned in `scripts/build-listing-wasm/toolchain.go`,
  Renovate-tracked) with mise into a PRIVATE home, `~/.cache/cmdr/wasm-toolchain/` (`CMDR_WASM_TOOLCHAIN_DIR`
  overrides), and hands cargo only `CC_wasm32_unknown_unknown` / `AR_wasm32_unknown_unknown` as absolute paths. ❗ Never
  on PATH and never in the default mise home: the conda env ships its own `cc`, `cpp`, `ar`, and `ld`, and their mise
  shims would sit ahead of `/usr/bin`, one `mise use` away from switching every native C build (verified on macOS 27 and
  Linux aarch64, mise 2026.10.25, 2026-11-04). First build downloads about 770 MB (macOS) or 1.5 GB (Linux); CI restores
  it from its own cache (`.github/actions/wasm-toolchain`, saved from `main` only).
- Output: `crates/cmdr-listing-wasm/pkg/` (gitignored): `cmdr_listing_wasm.js`, `cmdr_listing_wasm_bg.wasm`, their
  `.d.ts`, and a `package.json` saying `"type": "module"`.
- `wasm-release` (root `Cargo.toml`): `opt-level = "s"`, fat LTO, one codegen unit, `panic = "abort"`, stripped. With
  the write engine, SQLite, and the operation log linked: 6.5 MB raw, 2.5 MB gzipped (`gzip -9`, 2026-11-04; the listing
  engine alone was about 2.5 MB raw, ICU collation data most of it). With the viewer and the search core (rayon among
  it) too: 9,836,769 bytes raw, 3,565,021 gzipped. Rayon (with `crossbeam`) is about 27 KB of the 7.5 MB of code, about
  0.4%, and the search core about 120 KB (code naming each, after fat LTO; verified 2026-11-04 with an unstripped build,
  `CARGO_PROFILE_WASM_RELEASE_STRIP=false pnpm build:listing-wasm`, then the toolchain's own
  `llvm-nm -S -C --defined-only` over `target/wasm32-unknown-unknown/wasm-release/cmdr_listing_wasm.wasm`).
- **Multi-Rename** (`cmdr-multi-rename`, with `cmdr-media-meta` beside it) adds 335,351 bytes raw, 111,589 gzipped, and
  76,705 Brotli'd (shipped build after `wasm-opt`, raw / `gzip -9` / `brotli -q 11`: 9,447,722 / 3,568,545 / 2,589,429
  without it, 9,783,073 / 3,680,134 / 2,666,134 with it; rustc 1.99.1, binaryen 133, 2026-11-05). `regex` and
  `unicode-normalization` were already linked (search, viewer), so that's the engine itself.
  - **Decision: no `read` feature in `cmdr-media-meta`.** The media readers (`lofty`, `kamadak-exif`, the byte source's
    `tokio/rt` bridge) cost the browser build nothing worth a feature: built with them gated off (a trial `read`
    feature, `default-features = false` on both edges), the module was 9,782,207 / 3,680,195 / 2,656,733, so 866 bytes
    raw, nothing gzipped (61 bytes the other way), and 9 KB Brotli'd. Why: the demo's reader never calls
    `read_media_from`, so fat LTO drops the parsers. The bar was 50 KB gzipped; a feature would be a second build shape
    of the crate to keep compiling for that.
- **`wasm-opt -O1 --duplicate-function-elimination`** runs on the bindgen output in place (binaryen pinned in
  `.mise.toml`; `wasmOptArgs` in `main.go`, with rustc's default wasm32 feature set so it validates what rustc emits).
  ❌ Don't switch it to `-Oz` or `-Os`: they shrink the raw file most but compress worse, and the page downloads the
  compressed bytes. Measured on the same engine build (binaryen 133, rustc 1.99.1, `gzip -9`, `brotli -q 11`,
  2026-11-04), raw / gzip / brotli in bytes:
  - No `wasm-opt`: 9,840,785 / 3,565,560 / 2,581,959.
  - `-Oz` (8 s): 8,913,278 / 3,622,720 / 2,657,221, so 75 KB more over the wire. `-Oz --converge` (17 s): 8,907,860 /
    3,608,262 / 2,648,169.
  - `-Os` (7 s): 8,970,378 / 3,618,853 / 2,660,197. `-O2`: 9,079,077 / 3,635,131 / 2,667,136. `-O3`: 9,047,078 /
    3,623,800 / 2,651,647.
  - `-O1`: 9,381,874 / 3,542,793 / 2,577,048.
  - **`-O1 --duplicate-function-elimination` (1 s, shipped): 9,358,817 / 3,540,069 / 2,571,369**, smaller on all three
    (raw −4.9%, brotli −10 KB).
  - Node's `WebAssembly.compile` took 7–9 ms for every variant (lazy tiers), so the raw size buys no startup time
    either. What the page actually receives is the precompressed Brotli file (`apps/website/DETAILS.md` § Browser demo).
    The smoke tests run on the optimized module; the golden replay runs on the dev-profile test build, which `wasm-opt`
    never touches.
- **The engine ships at release cadence.** No land builds it for wasm: the release pipeline's `demo` job builds and
  smoke-runs it before it publishes the demo image, and the nightly CI run builds it too (`rust-lane.yml`'s `wasm` job),
  so a break shows the next morning. An engine change reaches getcmdr.com with the next release
  (`apps/website/DETAILS.md` § Browser demo). `websiteDemoInputs` in `scripts/check/checks/inputs.go` lists the crates
  the module links, for the local website Docker build's fingerprint; a new path dep goes there too. List them with
  `cargo tree -p cmdr-listing-wasm --target wasm32-unknown-unknown -e normal --prefix none | rg '\(/'`.
- The `wasm-bindgen` CLI has to be the exact version of the `wasm-bindgen` crate in `Cargo.lock`, or it refuses the
  module; the script reads the lockfile and `cargo install`s that version into `~/.cargo/bin` when it's missing.
- The check: `desktop-rust-listing-wasm` runs the script with `-smoke -golden` into a temp dir, in CI's "Desktop (Rust)"
  job. The smoke test (`smoke.mjs`) loads the module in Node and drives a listing, a fuzzy jump, a refresh, Brief column
  widths from stored font metrics, and a typed rejection, which is what catches the browser-only traps. It also drives
  ⌘F (a glob, a scope, a folder-size filter, and a streaming run to its `search-complete`) with `debug` logging on: the
  proof that the scan's rayon fallback runs inline and its timing log reads the page's clock. Its write half
  (`smoke-writes.mjs`) runs a copy and its undo, a conflict answered Skip, a cancel, a new folder, a rename, a move, a
  delete, and the journal read back, each with its `directory-diff`; then a 1,200-file delete under a `setTimeout` probe
  that fails past a 50 ms hold. The probe runs past the settle until the engine has sent nothing for 200 ms, so the
  folder-size news that trails the delete is inside it. Its Multi-Rename half (`smoke-multi-rename.mjs`) opens a sheet,
  previews a counter mask, moves a row, renames and rolls back, then reads 1,200 photos' media fields to `done` under
  the same probe. Any unhandled rejection fails the run.
  - **The hold is the main thread's CPU time** between two ticks (`process.threadCpuUsage`), with wall time only
    printed. Decision/Why: wall time failed under machine load (87 ms at a load average of 74–130 from parallel agents'
    checks, 13–19 ms alone), since a descheduled process ages the wall clock but not its thread's CPU time. Measured
    together, 2026-11-05 on Node 26.12 at load 85–92: CPU 16–20 ms across four runs while wall swung 16–104 ms. The
    checker's contention verdict (`contention-verdict.go`) would only have rerun a red; this can't go red from load.
  - **The runtime self-test's yield-cost probe** (`rt_self_test.rs`) can't use CPU time: the timer clamp it guards
    against is idle time. So it measures yields against a yardstick in the same run: 10 rounds of 10 yields interleaved
    with 10 rounds of 10 `setTimeout(0)` sleeps, the fastest round of each, and the yields must win by 4x. Decision/Why:
    the old fixed budget (100 yields under 50 ms) went red at load 80–92 (57–167 ms); interleaving shares the load, and
    the fastest round dodges a descheduling burst. Verified 2026-11-05 on Node 26.12 at load 62–78: the real yield won
    by 22–55x over six runs, and the old timer-based yield fed in its place came in at about 1x (13.9 against 13.3 ms)
    and failed.
- **The golden replay** (`-golden`, also in the check): `src/operations_host/golden_test.rs` runs every volume-engine
  scenario in `cmdr_operations::golden` on a real `Demo`'s host (its `DemoVolumes` with the fixtures' `src` and `dst`
  registered beside the page volume, its engine events, name order, and operation log) and compares the trace, as text,
  with the fixture the native engine blesses (`crates/cmdr-operations/tests/golden/`, embedded via `VOLUME_GOLDENS`). It
  runs natively under `cargo nextest`, and the script builds the crate's wasm TEST target (dev profile) and runs it
  under Node with `wasm-bindgen-test-runner` (from the same lock-matched `wasm-bindgen-cli`), so the browser runtime's
  `JoinSet` ordering, inline op-log writer, and inline `spawn_blocking` must produce the native traces (plan invariant
  13). Why a test build: the shipped `.wasm` carries no scenarios (`golden` is a dev-dependency feature). The other test
  modules are `not(target_family = "wasm")`, with their dev-deps under a native-only target table: threaded tokio,
  `tempfile`, and `tokio/net` don't build for wasm32. Cost: about 33 s for the first wasm test build, about 1 s warm
  (the whole check ran 4 s warm, 2026-11-04). Shown red by perturbing one fixture's size (2026-11-04).

## Writes

`operations_host/` is the write engine's host, mirroring the desktop's `operations_host/`: `volumes.rs`
(`OperationVolumes` over the page's volumes by id, one in the page, more in the golden replay; `local_disk_id` is
`None`, so the demo's `root` takes the volume engine for every op), `events.rs` (`EngineEvents`), `sinks.rs` (the per-op
and per-preview sinks; a failure is retained for the queue before its `write-error`, as the desktop's terminal effects
order it), `wire.rs` (every event name, pinned against `bindings.ts` by `events_test.rs`), and `commands/` by domain
(`transfers.rs`, `instant.rs`, `queue.rs`, `history.rs`, and `paths.rs`: the dialogs' `path_exists`,
`destination_exists`, `destination_root_echo`, and `stat_paths_kinds`, answered from the live volume). `history.rs` also
drives an operation log tree backfill on the page's event loop, one chunk per turn, since the page has no thread to
block (every demo op is journaled with its summary built, so only a test's doctored journal needs one).
`DemoOperations::new` also emits `operation-log-tree-indexes-settled` as the app's host does; the inline writer builds a
fresh journal's indexes inside `open`, so it fires at once. The volume is named after the tree's `volumeName`
(`Macintosh HD`), and its `home` makes it stand in for the computer's own disk (`Volume::stands_in_for_own_disk`), so a
transfer's summary reads `~/Documents` as the desktop's does. `Demo` routes a command to it when
`DemoOperations::handles` claims it.

- **One volume, one `Listings`**: `Demo::new` builds the `InMemoryVolume` once, hands it to both engines, and gives it
  pane patches through `backend_listings.rs`'s late-bound `BackendListings` (the volume needs its listing host before
  the engine that lists it exists). The write engine's `VolumeHost` carries the same host, and its `NameOrder` is the
  listing engine's collator.
- **The device**: `with_bandwidth(40 MB/s)` (its burst window keeps a page's late timers from eating the rate, and a
  chunk behind the timeline takes no timer: `crates/cmdr-fs/DETAILS.md` § Bandwidth), so the 1,200 photos (3.59 GB,
  eight streams at once) copy at the full rate, about 90 s, long enough to watch and pause in the queue. One stream
  reaches it too: `Rain sounds.flac` (120 MB) copies in about 3 s at 39.8 MB/s, because a yield is a `MessageChannel`
  task, not a clamped timer (it was 7.2 MB/s, 16.7 s, on `setTimeout(0)` yields; why and how measured:
  `crates/cmdr-fs/DETAILS.md` § "The browser build"), `with_yielding(8 ms)` (the page never waits long on engine work),
  and `with_capacity` at the disk size the volume list shows, so free space is live. `Pacing` stays the engine's default
  `None`.
- **The home folder stays**: the tree's `home` is also the engine's `home_folder`, so deleting `~`, `/Users`, or `/`
  refuses exactly as on the desktop (`crates/cmdr-operations/src/delete/DETAILS.md` § "The home folder stays"), with no
  demo branch. Pinned by `commands_test.rs`'s home-folder cell.
- **A full disk, on purpose**: the tree's `freeBytes` (the page's `?free=<bytes>`, `listing.ts`'s `freeBytesFromSearch`)
  sizes the disk as what the files hold plus that much room. A copy that doesn't fit then fails the way a real disk does
  (`StorageFull` → `destination_full`), and a backgrounded one stays in the queue as "Couldn't finish", whose Retry
  lands it once a delete has made room. Pinned by `commands_test.rs`'s full-disk cell and the website demo spec.
- **The journal** is the real `cmdr-operation-log` on `/demo/operation-log-<n>.db`, numbered per engine so the golden
  replay's engines in one module never share one (SQLite's in-memory VFS, gone on reload like the volume; the writer
  applies inline, `cmdr-operation-log`'s browser build). The ledger is claimed unpersisted; its `LedgerUnpersisted`
  report stays quiet. Natively (tests) the log is a scratch file in the temp dir.

## Folder sizes

`folder_sizes.rs`'s `FolderSizes` stands in for the drive index, with no `cmdr-index` in the build: a fold over the
store (`InMemoryVolume::subtree_totals`, counting the way `cmdr-index`'s aggregator does), complete and never stale.

- **Decision: the existing seams, no new one.** The app's index already reaches both engines through one trait each:
  `cmdr_listing::host::ListingIndex` (`enrich` fills pane rows on every read, insert, and `refresh_index_sizes`) and
  `cmdr_operations::host::OperationIndex` (dialog sizes, progress totals). `FolderSizes` implements both, and the
  `get_dir_stats` / `get_dir_stats_batch` commands (the status bar, selection info, the `..` row) read it too. A third
  "folder size source" trait would duplicate `ListingIndex::enrich`. `search_leaves` stays downgraded (`IndexAbsent`):
  the fold counts, it keeps no leaf list.
- **Changes compose ahead of the panes**, as the app's composer feeds its index first: `BackendListings` hands every
  `directory_changed` to `FolderSizes::changed` (which forgets the changed entry's subtree and its ancestors from the
  memo), then to the panes, then `FolderSizes::note_moved` gathers its parent. 50 ms after the first note
  (`SIZES_GATHER` in `backend_listings.rs`, the write engine's progress cadence), `FolderSizes::publish_moved` tells
  every open listing AT or above a gathered change what moved, the app's index-size worker's way: each folder row on the
  way down written into the cache by `update_index_sizes` (the pane AT a change has none: it heard the change itself),
  then ONE `listing-index-sizes-changed` with those rows and the listing's own folder (`currentDir`, the `..` row). That
  event is the only path the frontend has for the `..` row and the status bar to move without a re-read. No hourglass,
  and a short gather in place of the app's 2 s cooldown, since a fold is never behind.
  - **Decision/Why: gathered, not one event per change.** A 1,200-file delete reported 1,200 removals, each its own
    event per pane; the page took them through the frontend's handler back to back, a ~45 ms hold at the delete's end
    (#956). Gathered, the smoke's long delete sends one. The app's worker doesn't have this burst: it already merges
    index batches per listing behind its cooldown (`apps/desktop/src-tauri/src/listing_index_sizes/schedule.rs`), so the
    fix is the demo's alone. A test waiting on a pane's row after a write waits for the event, not the settle
    (`folder_sizes_test.rs`'s `heard_size`).
- **Physical size** is each file's `physical_size`, or its logical size where the store models none.

## Search

`search.rs`'s `DemoSearch` answers the search commands by their Tauri names (`SEARCH_COMMANDS`): the drive search's core
over the page's one volume, with no index to load.

- **The volume is the index**: a `cmdr_search::MemorySource` built from `InMemoryVolume::held_entries`, rebuilt when
  `FolderSizes::changes` moved (every write the panes hear moves it first). A few thousand rows rebuild in well under a
  millisecond, cheaper than any staleness rule. `prepare_search_index` answers `ready: true` on the first ask, and
  `release_search_index` only stops dialog runs: the arena is tiny, so there's no idle drop.
- **Folder sizes are the fold's** (`FoldSizes`, a `DirSizeSource` over `FolderSizes::held_at`), read per query: a size
  filter and a results row read the numbers the panes and `get_dir_stats` show. ❌ Never a second rollup here.
- **Name rules** (`DEMO_NAME_RULES`): a Mac's (`NameRules::MACOS`), since it presents as one; a pattern meets a name in
  either normal form, so the tree's NFC names answer whichever form is typed. Home is the tree's `home`, so rows read
  `~/…`.
- **A live run covers everything at once**: `search_files_streaming` announces, hands the covered half to
  `ResultStream`, and finishes with `CoverageFacts::new(NothingToWalk, Covered, ..)`, all before the command answers.
  The page's sink defers every event a task (`listing.ts`), so the dialog has registered the run first. Payloads mirror
  the app's `search/live/events.rs`; `search_test.rs` pins their keys against `bindings.ts`.
- **Results order and recent searches**: `sort_search_results` is `cmdr_listing::sort_search_rows` on the panes'
  collator; the recent list is `cmdr-recents`' rules over `cmdr_search::history`'s key, in memory.

## Multi-Rename

`multi_rename.rs`'s `DemoMultiRename` answers the sheet's 24 commands by their Tauri names (`MULTI_RENAME_COMMANDS`,
pinned against `bindings.ts` by `multi_rename_test.rs`): `cmdr-multi-rename`'s `MultiRename` engine, built in
`Demo::new` over the panes' `Listings`, the page runtime, and `DemoHost`.

- **One volume, one `Listings`, one write engine**: the host's `rename_place` answers the page volume (or `NotConnected`
  for another id), so fingerprints go through `capture_remote`, and `operations()` is `DemoOperations`' engine. Apply's
  events go through `DemoOperations::op_sink`, the sink every write uses, so a rename passes `ImportWatch` and the queue
  like a copy.
- **Media fields**: the host's loader answers `ReadError::NotSupported` for every file, which the engine stores as
  `Unreadable` without counting toward giving up. The whole read runs in ONE task (an inline `spawn_blocking_on` leaves
  `join_next` always ready, and the browser has no coop budget), so the throttle sends the first and the final `done`
  event. Measured by the smoke: 1,200 photos in two events and a 2 ms hold of the main thread's CPU (2026-11-05). A
  sheet that subscribes late asks `get_multi_rename_media_progress`, which answers `done` from the cache. The progress
  payload is the crate's `MediaProgressEvent`, sent as is under `multi-rename-media-progress`.
- **Results**: `results_supported` is `false`, so `open_multi_rename` answers `resultsAvailable: false` and the sheet
  disables Results with a tooltip saying it needs the desktop app. `results_path` is `None`, so a
  `write_multi_rename_names` anyway refuses `couldntWriteNames`.
- **Persistence in memory**: presets (`PresetList`, seeded once on the first `get_multi_rename_presets`, as
  `PresetStore` does), the last settings, and the field history (started from `history_seed()`, as a new install's,
  added to after each apply with `cmdr_recents::add_to`). Gone on reload, like the volume.
- Tests: `multi_rename_test.rs` (open, a counter preview, a moved row retiring the preview, apply and its
  `directory-diff`, roll back, history, a media mask ending `unreadable` with one final `done` event, Results' refusal,
  presets, last settings); `smoke-multi-rename.mjs` runs the same on the real `.wasm`, plus the 1,200-photo probe.

## Files dropped in from the computer

`imports.rs`'s `DemoImports` lands a visitor's dropped files on the page's volume through `InMemoryVolume`'s import door
(`import_folder`, `import_file`: real bytes beside the synthetic files, no clobber, `StorageFull` past the disk's room;
`crates/cmdr-fs/DETAILS.md` § Module map). The page side is `apps/desktop/src/lib/demo/DETAILS.md` § Files dropped in
from the computer. Nothing leaves the page.

- **Two steps, so the page reads only bytes that will land.** `plan(items)` takes the drop's shape (relative
  `/`-separated paths, folders before their contents, kinds, sizes, `modifiedMs`; no bytes), accepts the longest prefix
  the budget covers, creates its folders, and reserves its files' bytes. `file(batch, index, bytes)` lands one accepted
  file; bytes of another size (`SizeChanged`) or a volume refusal give the reservation back. A path with an empty, `.`,
  or `..` name, or a leading `/`, is `BadPath`, checked by hand: `Path::components` drops a `.`, and `Path::is_absolute`
  is always `false` in the browser.
- **Staging**: each drop gets a fresh `/.drops/<n>/` (hidden like a dotfile), so a drop never collides with itself or an
  earlier one; the page hands the staged top-level paths to the app as an OS drop, and the transfer dialog settles
  collisions with the destination.
- **A staging folder lives only while something can read it**, and its sweep gives back the room of what didn't land. A
  batch is `Landing` (plan to `hand_off`), then `Offered` (the page sent it as a drop), then `Claimed` by the transfer
  whose sources lie in it (`Demo::invoke` claims on a transfer command's answer). The claiming transfer's
  `write-settled` (done, cancelled, or failed) sweeps it: `ImportWatch`, the write engine's sink, hears the event and
  runs the sweep on the page runtime; a settle that beats its command's answer is kept and finished at the claim. An
  `Offered` batch nobody claimed (its dialog dismissed, or the drop landed nowhere) is swept when the app pushes
  `set_file_operations_blocked { blocked: false }` (no dialog holds file operations, so no transfer can still take it;
  `backend.ts` passes the fact on), and at the next `plan`. A hand-off with nothing offered sweeps at once.
  - What didn't land: a move's is whatever is still staged; a copy leaves the originals, so its landed bytes are its
    `write-complete` `bytesProcessed` (else its last `write-progress` `bytesDone`), and the rest goes back. A copy
    rolled back after reporting progress keeps those bytes counted; the budget errs toward the cap.
  - **Decision/Why: the dialog's unblock, not its close.** On confirm, `notify_dialog_closed { transfer-confirmation }`
    comes BEFORE the transfer command (verified in Chromium against `pnpm build:demo`, 2026-11-05, an IPC log of drop →
    Escape and drop → Enter), so sweeping on it would delete the files the transfer is about to read. The unblock comes
    after the progress dialog closes, when the batch is already claimed.
- **Decision: 10 MB per visit (`IMPORT_BUDGET_BYTES`), counting every byte imported**, David's call (#957). Reserved at
  plan time, so two drops in flight can't both spend the same room. The first item that doesn't fit stops the drop
  there: it and everything after stay out (`OverBudget { budgetBytes, leftOut }`), even a smaller one that would fit, so
  what came in is always a prefix of the drop. A reload resets it, with the volume.
- Tests: `imports_test.rs` (plan, structure, the budget across drops, a changed size, bad paths, a staged file copying
  and counting in folder sizes like any other, and the sweep: a dismissed drop, an empty hand-off, the next drop, a
  move, a move that skipped its clash, a copy).

## Where the demo differs from the app

- **No `listing-progress`**: `InMemoryVolume` ignores its progress callback; an in-memory read is instant anyway.
- **No `~` expansion** on `list_directory_start_streaming` (the desktop expands it for `root`); a browser has no home.
- **`refresh_listing` awaits the re-read in place**: the desktop's two-second cap guards hung mounts, which an in-memory
  read can't be.
- **Font metrics live in memory only**: the desktop persists each size to `font-metrics/{font_id}.bin`, so a reload of
  the page measures again. No `timeout` kind either: the desktop's two-second cap on `get_brief_column_text_widths`
  guards a write-locked cache, which a page's one thread can't be waiting on.
- **Owners show as empty, permissions are fixed**, and nothing reads tags, packages, or git status. Folder sizes come
  from a fold, never an index (§ Folder sizes).
- **The menu bar is the macOS one** whatever the visitor's OS, and its OS rows (Services, Hide, Quit, Undo, Minimize)
  are drawn greyed: a page has no OS to hand them to.
- **No local disk**: `move_files` (the desktop's same-`root` move) runs as a volume move on `root`, the dialog's
  `previewId` carried in its config; `trash_files` and `move_to_trash` refuse typed (`trash_not_supported`), as a volume
  without a Trash does, and F8 takes the app's permanent-delete confirm. `trash_routing_for_paths` answers the plain
  trash route (nothing online-only here).
- **No host work around writes**: no `~` expansion, no S3 cost pricing (`check_rename_validity` prices nothing), no
  analytics, no indexing priority, and `get_volume_space` answers the one volume's live space for every path.
- **Search** (`search.rs`): recent searches last the session; a scope resolves its symlinks through
  `InMemoryVolume::canonicalize` (the tree's `symlinkTarget`s, stored by `tree.rs`) where the desktop calls `realpath`,
  with exact-case names, so a scope spelled in another case through a link stays the literal (still matched
  case-insensitively, but as the link, a leaf); no AI (`getAiStatus` answers unavailable, so the dialog's AI mode stays
  off as on a Mac with AI off); no importance weights, so ties rank by name and id alone.
- **Multi-Rename** (`multi_rename.rs`, § Multi-Rename): no media reads (every media field reads as couldn't-read), no
  Results, presets and history in memory, and every call runs inline with none of the desktop's IPC deadlines or preview
  budget.
- **The viewer** (`viewer.rs`): every call runs inline, with none of the app's 2 s timeouts or stall watches (an
  in-memory read can't hang). No LineIndex upgrade, so a big file keeps its estimated row count; a search scans in 8 ms
  slices on the page's event loop instead of a thread; no watcher, so tail mode never sees a change.
  `viewer_write_range_to_file` refuses with `destinationIsReadOnly`; web pictures in a Markdown document never load (the
  page reaches no network). Why each: `crates/cmdr-viewer/DETAILS.md` § "The browser build".
