# Multi-Rename Tool: the desktop host

The app's host for the ⌃M engine, `crates/cmdr-multi-rename` (its `CLAUDE.md` holds the feature's must-knows and TC
semantics; read it first). The sheet is `src/lib/multi-rename/`, the IPC `commands/multi_rename.rs`.

## Module map

- `mod.rs` re-exports the crate (`pub use cmdr_multi_rename::*`), so the app names it `crate::multi_rename::…`.
- `analytics.rs` `multi_rename_applied`, one per apply via `AppHost::rename_applied` (catalog:
  `analytics/DETAILS.md`).
- `app_host.rs` the app's engine (`app_multi_rename`, a `LazyLock` on Tauri's runtime), its `MultiRenameHost` answers
  (volume reads, Results, `rename_place`, `io_budget`, the write engine), and the `MultiRenameMediaProgress` event.
- `app_files.rs` `LAST_SPEC`, `FIELD_HISTORY`, their `RecentEntry` impls, and `results_path`. `preset_store.rs` the
  presets file (`PRESETS`).
- Tests: the session-level ones over a real disk (`session_test`, `run_test`, `results_test`, `media_test`,
  `media_real_test`), `wire_test` (serde shapes), and the files' (`preset_store_test`, `app_files_test`).

## Must-knows

- **The engine is held, built on Tauri's runtime**: the pool the IPC commands' blocking work always used. ❌ Never
  `Handle::current()` at build time: a test's runtime would die while the static lives on.
- **The app's test build raises `max_sessions`** (its tests share one engine in parallel). ❌ Never in the crate.
- **`rename_place` resolves ONCE** through the volume manager, in the order `DETAILS.md` gives. ❌ Don't split it into a
  read-only check plus a lookup.
- **`MultiRenameMediaProgress` must serialize as the crate's `MediaProgressEvent`** (`wire_test.rs` pins it).
- **The MCP tool builds its own loader** (`volume_loader` over `Handle::current()`) with a no-op emit: it sends no
  progress event.
- **Files keep their names and shapes** (`multi-rename-presets.json`, `-last.json`, `-history.json`); an unreadable
  one goes through `state_recovery`.
- **Wire changes show in `bindings.ts`**: `pnpm check desktop-bindings-fresh` after touching a crossing type.

Host details, the files, IPC deadlines: `DETAILS.md`.
