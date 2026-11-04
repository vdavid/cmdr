# cmdr-listing-wasm

The real listing engine (`cmdr-listing` over `cmdr-fs`'s `InMemoryVolume`) in a browser, for the `/demo` page (#302).
The engine's second host, beside `apps/desktop/src-tauri/src/listing_host/`. Nothing in the desktop app depends on it.

## Module map

- `tree.rs`: the page's JSON tree into an `InMemoryVolume`. `events.rs`: engine facts into Tauri-named events.
  `commands.rs`: `Demo`, the listing commands by their Tauri names, plus Brief widths and font metrics
  (`cmdr-brief-columns`, memory only). `menu.rs`: `DemoMenu`, the menu-state pushes stored and the macOS bar drawn from
  `cmdr-menu`; `context_menu.rs`: its right-click popups, the `show_*` commands answered with rows. `folder_sizes.rs`:
  the drive index's stand-in, a fold over the volume behind both engines' index seams (DETAILS § Folder sizes).
  `viewer.rs`: the file viewer's commands (`cmdr-viewer`, same volume). `search.rs`: ⌘F (`cmdr-search` over the same
  volume, sized by the fold; DETAILS § Search). `invoke.rs`: the args/answer JSON helpers every command family shares, a
  leaf so no family imports `commands.rs`. `bindings.rs` (wasm only): the `wasm-bindgen` API.
- `operations_host/`: the write engine's host (seams, sinks, `wire.rs` event names, `commands/` by domain), on the same
  volume and `Listings` the listing engine uses. DETAILS § Writes.
- `smoke.mjs` (+ `smoke-writes.mjs`, `smoke-viewer.mjs`): loads the built module in Node and drives a listing, every
  write, every viewer command, and a search. Build: `pnpm build:listing-wasm`.

## Must-knows

- **Same wire as the desktop, or the frontend breaks silently.** Command names, camelCase args, answers, rejections, and
  event payloads all mirror `commands/file_system/listing.rs`, `commands/font_metrics.rs`, `commands/menu_state.rs`,
  `commands/menu.rs`, `commands/file_viewer.rs`, `commands/search.rs`, and `listing_host/events.rs` (writes: the
  desktop's `operations_host/` and its write commands; search events: `search/live/events.rs`). A change there needs one
  here; `events_test.rs`, `menu_test.rs`, `context_menu_test.rs`, and `search_test.rs` pin event keys and names against
  `bindings.ts`.
- **One volume, one `Listings`, shared by every engine** (listing, writes, viewer), or a copy lands unseen by the panes.
- **Everything but `bindings.rs` builds natively**, so the tests run with `cargo nextest` like any crate. Keep JS types
  out of `commands.rs`, `events.rs`, and `tree.rs`.
- **The runtime traps are browser-only**: `std::time::Instant::now` and `SystemTime::now` panic, `Path::is_absolute` is
  always `false`, and nothing can spawn a thread. Native tests can't see them; `desktop-rust-listing-wasm` runs
  `smoke.mjs` against the real `.wasm` so they fail a check.
- **`JsSink` is `unsafe impl Send + Sync`**, sound only while the build has no `atomics` target feature (one thread).
  The `cfg` makes a threaded build fail to compile; ❌ never drop it.
- **The `wasm-bindgen` CLI must equal the locked crate exactly.** The build script reads `Cargo.lock` and installs the
  match; ❌ never pin the CLI anywhere else.
- No `tauri` in the tree, enforced by `index-crate-isolation`: the demo reaches the app's logic only through the crates.

The decision (target, runtime), the JS API, the build, and where the demo differs from the app: `DETAILS.md`. Read it
before any non-trivial work here: editing, planning, reorganizing, or advising.
