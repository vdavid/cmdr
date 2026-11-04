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
  page measures its own fonts with the Canvas API and stores them, as the desktop's frontend does.
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
  build is about to link, `browserBound` in `main.go`, so a host-only dep can't creep into them first), then
  `wasm-bindgen --target web`.
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
  it) too: 9,746,088 bytes raw, 3,538,186 gzipped (2026-11-04; rayon's own share unmeasured, the build being stripped).
  `wasm-opt` isn't run (no binaryen in the toolchain).
- **Every crate the module links is a demo image input**: `scripts/website-demo-key.sh`, `websiteDemoInputs` in
  `scripts/check/checks/inputs.go`, and ci.yml's `website` filter each list them (the first two are kept aligned by
  `TestWebsiteDemoInputsMatchTheImageKey`, the filter by hand). A new path dep goes in all three, or an engine-only
  change keeps the old image key and getcmdr.com serves a stale engine. List them with
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
  that fails past a 50 ms hold. Any unhandled rejection fails the run.
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
`destination_exists`, `destination_root_echo`, and `stat_paths_kinds`, answered from the live volume). The volume is
named after the tree's `volumeName` (`Macintosh HD`), and its `home` makes it stand in for the computer's own disk
(`Volume::stands_in_for_own_disk`), so a transfer's summary reads `~/Documents` as the desktop's does. `Demo` routes a
command to it when `DemoOperations::handles` claims it.

- **One volume, one `Listings`**: `Demo::new` builds the `InMemoryVolume` once, hands it to both engines, and gives it
  pane patches through `backend_listings.rs`'s late-bound `BackendListings` (the volume needs its listing host before
  the engine that lists it exists). The write engine's `VolumeHost` carries the same host, and its `NameOrder` is the
  listing engine's collator.
- **The device**: `with_bandwidth(40 MB/s)` (its burst window keeps a page's late timers from eating the rate, and a
  chunk behind the timeline takes no timer: `crates/cmdr-fs/DETAILS.md` § Bandwidth), so the 1,200 photos (3.59 GB,
  eight streams at once) copy at the full rate, about 90 s, long enough to watch and pause in the queue. One stream
  stays below it: a 64 KiB chunk per page task caps `Rain sounds.flac` (120 MB) at about 7 MB/s, 17 s end to end
  (verified in a `pnpm build:demo` under headless Chromium on a loaded Mac: 39 MB/s on the photo copy, 7.1 MB/s on the
  one file, 2026-11-04), `with_yielding(8 ms)` (the page never waits long on engine work), and `with_capacity` at the
  disk size the volume list shows, so free space is live. `Pacing` stays the engine's default `None`.
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
  memo), then to the panes, then `FolderSizes::publish_moved` tells every open listing AT or above the change what
  moved, the app's index-size worker's way: the one folder row on the way down written into the cache by
  `update_index_sizes` (the pane AT the change has none: it heard the change itself), then `listing-index-sizes-changed`
  with that row and the listing's own folder (`currentDir`, the `..` row). That event is the only path the frontend has
  for the `..` row and the status bar to move without a re-read. No cooldown and no hourglass: the app's worker batches
  a live index, and a fold is never behind.
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
- **Name rules** (`DEMO_NAME_RULES`): a Mac's case rule and APFS fold (it presents as one), patterns left as typed (the
  tree's names are NFC, so an NFD pattern would miss every accented one). Home is the tree's `home`, so rows read `~/…`.
- **A live run covers everything at once**: `search_files_streaming` announces, hands the covered half to
  `ResultStream`, and finishes with `CoverageFacts::new(NothingToWalk, Covered, ..)`, all before the command answers.
  The page's sink defers every event a task (`listing.ts`), so the dialog has registered the run first. Payloads mirror
  the app's `search/live/events.rs`; `search_test.rs` pins their keys against `bindings.ts`.
- **Results order and recent searches**: `sort_search_results` is `cmdr_listing::sort_search_rows` on the panes'
  collator; the recent list is `cmdr-recents`' rules over `cmdr_search::history`'s key, in memory.

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
- **Search** (`search.rs`): recent searches last the session; no `realpath`, so a scope naming a symlinked folder is the
  link itself and finds nothing inside; no AI (`getAiStatus` answers unavailable, so the dialog's AI mode stays off as
  on a Mac with AI off); no importance weights, so ties rank by name and id alone.
- **The viewer** (`viewer.rs`): every call runs inline, with none of the app's 2 s timeouts or stall watches (an
  in-memory read can't hang). No LineIndex upgrade, so a big file keeps its estimated row count; a search scans in 8 ms
  slices on the page's event loop instead of a thread; no watcher, so tail mode never sees a change.
  `viewer_write_range_to_file` refuses with `destinationIsReadOnly`; web pictures in a Markdown document never load (the
  page reaches no network). Why each: `crates/cmdr-viewer/DETAILS.md` § "The browser build".
