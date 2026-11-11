# cmdr-multi-rename

The Multi-Rename Tool's engine (⌃M, Total Commander's mask renamer), Tauri-free so the desktop and the browser demo run
the same sheet. Hosts: `apps/desktop/src-tauri/src/multi_rename/` and `crates/cmdr-listing-wasm/src/multi_rename.rs`.

## Module map

- `mask.rs` (+ `mask/media_token.rs`) placeholders; `transform.rs` search & replace, case and character steps;
  `transliterate.rs` Greek to Latin; `plan.rs` the spec, `Compiled`, rows and statuses; `error.rs`.
- `presets.rs`, `history.rs` types and pure list ops; `names_file.rs` the Results file; `rule_kinds.rs` which rules a
  spec uses (the app's `multi_rename_applied` analytics).
- `session.rs` `MultiRename` (sessions, over a host's `Listings`, runtime, and `MultiRenameHost` in `host.rs`);
  `media.rs` the media cache and loader; `run.rs` apply; `started.rs` its result, apart so `host` never imports `run`.

## Must-knows

- **The selection becomes names ONCE, at open**, under the listing's sequence guard. ❌ Never map row numbers through
  the live listing again: a new file shifts rows onto files the user never picked.
- **Names stay in the backend.** Apply takes `(session, preview)`, recomputes, and refuses `PreviewOutOfDate` unless its
  ready rows are EXACTLY that preview's. A row move retires the preview.
- **Results names match by OLD name, never line**, and go through every status check.
- **❌ No file read under the sessions lock or the media cache's lock**; `Session`'s `Drop` stops the loader.
- **`a|b` → `x|y` replaces in ONE pass.** ❌ Never chain the pairs (`a` became `c`).
- **TC's order is fixed**: mask, search & replace, case, Greek to Latin, diacritics, Normalize Unicode. Positions count
  from 1; a range past the end is empty. A folder has no extension; names are NFC before the mask.
- **A preset's number is its place**: new goes last, edits keep it. ❌ Not a recents list.
- **Statuses compare folded names**; a block cascades through a worklist.
- **Wire types' doc comments land in the app's `bindings.ts`**: ❌ never reword one casually.
- **No `tauri`, app paths, ambient runtime, or `std` clock.** Work goes on the held runtime
  (`JoinSet::spawn_blocking_on`, `deadline::*_on`); `clippy.toml` bans what panics in the browser.
- **The host trait carries only what differs.** Progress `emit` is the caller's `Loader` field: the MCP tool's read
  sends no event. ❌ Never a host-level emit. `resultsAvailable` comes from `results_supported`, ❌ never
  `results_path(..).is_some()`.
- **Persistence is the host's**: files, statics, `RecentEntry` impls.
- **`*_token_vectors.json` are shared with the frontend's `*-token.test.ts`**: change the grammar here first.
- Log calls keep explicit `target:` strings (`multi_rename::…`).

TC semantics, decisions, the host seam, and the browser build: `DETAILS.md`.
