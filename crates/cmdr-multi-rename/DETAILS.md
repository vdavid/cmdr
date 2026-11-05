# cmdr-multi-rename details

## Why its own crate

The browser demo (`crates/cmdr-listing-wasm`) should run the real sheet: the same masks, statuses, and presets as the
desktop, through the same code. A crate with no `tauri` in its tree is what lets the wasm build link it. The pipeline,
placeholder grammar, and TC semantics are documented in `apps/desktop/src-tauri/src/multi_rename/DETAILS.md` for now.

## The host split

- **Here**: the pure engine (mask, transform, plan, error), the preset and history types with their list ops (`save_in`,
  `seed_in`, `history_entries`, …, and the `cmdr_recents::RecentItem` impls), and the Results file's text and its
  private write and read (`names_file.rs`).
- **The desktop host**: `app_files.rs` (the `LAST_SPEC` and `FIELD_HISTORY` recents files, their `RecentEntry` impls,
  and `results_path` in the app's cache dir), `preset_store.rs` (the presets file), and the sessions, media reads, and
  apply. It re-exports this crate (`pub use cmdr_multi_rename::*`), so app code names everything as
  `crate::multi_rename::…`.
- **The public surface is capped** (`index-crate-isolation`, measured at the extraction): seven public modules, 74
  items. A module's `pub(crate)` item turns `pub` only when a host names it.
- **Decision/Why: `RecentEntry` is implemented in the host**: it names the app's file and its `state_recovery` key, and
  the orphan rule allows it because the trait is the app's.

## The browser build

- `cargo build -p cmdr-multi-rename --target wasm32-unknown-unknown` passes with `cmdr-media-meta`'s default features
  (its readers, `lofty`, `kamadak-exif`, and `tokio/rt`, compile there; verified 2026-11-05 with that command). Whether
  to drop the readers from the browser build is a size question, measured when the demo links the crate.
- Until the demo links it, `scripts/build-listing-wasm/main.go`'s `browserBound` compiles it for wasm32 so a host-only
  dependency can't creep in.
- `names_file.rs` uses `libc` and `std::os::unix` only under `cfg(unix)`; the browser never writes a Results file.
- `chrono::Local` reads the offset from JS `Date` there (chrono's default `wasmbind`), and uuid's `js` feature comes
  from `cmdr-fs`.

## Tests

The pure tests live beside their modules (`*_test.rs`). The session-level tests run over a real disk through the
desktop's listing host, so they stay desktop host tests (`apps/desktop/src-tauri/src/multi_rename/`), along with the
wire-shape pins (`wire_test.rs`).
