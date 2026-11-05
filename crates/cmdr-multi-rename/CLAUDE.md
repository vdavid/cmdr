# cmdr-multi-rename

The Multi-Rename Tool's engine (⌃M, Total Commander's mask renamer), Tauri-free so the browser demo can run the same
sheet. The desktop host is `apps/desktop/src-tauri/src/multi_rename/` (sessions, media reads, apply, and the app's
files); its `CLAUDE.md` + `DETAILS.md` hold the feature's must-knows and TC semantics until they move here.

## Module map

- `mask.rs` (+ `mask/media_token.rs`) the placeholders, parsed once, rendered per row. `transform.rs` search & replace
  and the case and character steps; `transliterate.rs` Greek to Latin.
- `plan.rs` the spec, `Compiled`, and the preview's rows and statuses. `error.rs` `MultiRenameError`.
- `presets.rs` and `history.rs` the presets, last settings, and field history as types and pure list ops.
  `names_file.rs` the Results file (render, parse, private write and read).

## Must-knows

- **Wire types cross IPC through the app's `bindings.ts`**: specta copies their doc comments there, so ❌ never reword
  one while moving or tidying it. `pnpm check desktop-bindings-fresh` must stay a zero diff.
- **No `tauri`, no app paths, no ambient runtime, no `std` clock.** `clippy.toml` bans the calls that panic in the
  browser; `index-crate-isolation` bans `tauri` and caps the public surface.
- **Persistence is the host's**: files, statics, and `RecentEntry` impls live in the host. ❌ No file path decided here;
  the Results file is written wherever the host says.
- **The vector JSONs (`*_token_vectors.json`) are shared with the frontend's `*-token.test.ts`**: change the grammar
  here first, then the TS mirror.
- Log calls keep explicit `target:` strings (`multi_rename::…`), so `RUST_LOG` recipes don't move.

Details: `DETAILS.md`.
