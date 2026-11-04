# cmdr-listing-wasm details

The browser build of the listing engine, for the website's `/demo` (#302). Must-knows: `CLAUDE.md`. The engine's side of
the seam it needs: `crates/cmdr-listing/DETAILS.md` § "The browser build"; `cmdr-fs`'s gates:
`crates/cmdr-fs/DETAILS.md` § "The browser build".

## Decision: `wasm32-unknown-unknown` + `wasm-bindgen`, on the page's event loop

**Decision:** the target is `wasm32-unknown-unknown` with `wasm-bindgen`. The engine's executor and clock go through one
crate-private seam, `cmdr-listing`'s `src/rt/`: natively it re-exports `tokio::runtime::Handle`, `tokio::time`, and
`std::time::Instant` (so desktop code is the same code), and in the browser it spawns with
`wasm_bindgen_futures::spawn_local`, runs blocking work inline, reads `performance.now()` (`web-time`), and sleeps on
`setTimeout`.

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
- `new DemoListing(tree, onEvent)`: `tree` is
  `{ volumeId, root, entries: [{ path, kind: "file" | "dir" | "symlink", size?, modifiedMs?, createdMs?, symlinkTarget? }] }`,
  every parent folder listed. Throws on a bad tree (a relative path, a duplicate, a missing parent).
  `onEvent(name: string, payload: unknown)` hears every event by its Tauri name: `listing-opening`, `listing-progress`,
  `listing-read-complete`, `listing-complete`, `listing-error`, `listing-cancelled`, `listing-stalled`,
  `directory-diff`, `listing-respelled`, `directory-deleted`.
- `demo.invoke(command: string, args: object): Promise<unknown>`: a listing command by its Tauri (snake_case) name with
  its camelCase args. Resolves with the command's answer, rejects with its serialized error
  (`{ type: "gone", listingId }`), or a string for args that don't parse or a command it doesn't answer.
- `DemoListing.commands(): string[]` and `DemoListing.handles(command): boolean`: which commands `invoke` answers
  (`commands.rs`'s `COMMANDS`), so a mock routes those here and fakes the rest.
- `DemoListing.setLogLevel(level)`: the engine's `log` lines on the console, `"warn"` by default.

The tree: dates are JS milliseconds and become `FileEntry`'s seconds; files get mode `0644`, folders `0755`; a symlink
whose `symlinkTarget` is a folder in the tree lists as a folder (as on disk).

## The build

- `pnpm build:listing-wasm` (`scripts/build-listing-wasm.sh [-out DIR] [-smoke]`, a Go program):
  `cargo build -p cmdr-listing-wasm --target wasm32-unknown-unknown --profile wasm-release`, then
  `wasm-bindgen --target web`.
- Output: `crates/cmdr-listing-wasm/pkg/` (gitignored): `cmdr_listing_wasm.js`, `cmdr_listing_wasm_bg.wasm`, their
  `.d.ts`, and a `package.json` saying `"type": "module"`.
- `wasm-release` (root `Cargo.toml`): `opt-level = "s"`, fat LTO, one codegen unit, `panic = "abort"`, stripped. About 2
  MB raw (2026-11-04); ICU collation data is most of it. `wasm-opt` isn't run (no binaryen in the toolchain).
- The `wasm-bindgen` CLI has to be the exact version of the `wasm-bindgen` crate in `Cargo.lock`, or it refuses the
  module; the script reads the lockfile and `cargo install`s that version into `~/.cargo/bin` when it's missing.
- The check: `desktop-rust-listing-wasm` runs the script with `-smoke` into a temp dir, in CI's "Desktop (Rust)" job.
  The smoke test (`smoke.mjs`) loads the module in Node and drives a listing, a fuzzy jump, a refresh, and a typed
  rejection, which is what catches the browser-only traps.

## Where the demo differs from the app

- **A folder the tree doesn't hold lists as empty**, where a disk answers `NotFound`: `InMemoryVolume` filters by
  parent. Pinned by `commands_test.rs`'s `a_missing_folder_lists_empty`.
- **No `listing-progress`**: `InMemoryVolume` ignores its progress callback; an in-memory read is instant anyway.
- **No `~` expansion** on `list_directory_start_streaming` (the desktop expands it for `root`); a browser has no home.
- **`refresh_listing` awaits the re-read in place**: the desktop's two-second cap guards hung mounts, which an in-memory
  read can't be.
- **Owners show as empty, permissions are fixed**, and nothing reads tags, packages, index sizes, or git status.
- **No writes**: the demo answers listing commands only. Copy, move, rename, and create would each need their own
  extraction (#302).
