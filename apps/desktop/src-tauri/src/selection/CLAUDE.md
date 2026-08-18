# Selection module

Backend for the Selection dialog (Select files / Deselect files). Mirrors `crate::search` but narrower: no scope, no
system-dir exclusion, no in-memory index. The matcher itself runs in JS against the focused folder's entries; this
module owns only the persistent history store and the AI translation pipeline. IPC lives in `crate::commands::selection`.

## Module map

- **`mod.rs`**: re-exports the public surface.
- **`history.rs`**: `SelectionHistoryEntry` plus its canonical key, over `crate::recents` (which owns the file, dedupe,
  cap, and quarantine). Re-exports `HistoryMode` and `HistoryFilters` from `crate::search::history` so both consumers
  share one mode/filter shape, and reuses its key-building helpers.
- **`ai/`**: NL → glob/regex translation, cloud-only. See `ai/CLAUDE.md`.

## Must-knows

- **History persistence** is `crate::recents` (`apps/desktop/src-tauri/src/recents/CLAUDE.md`): `{app_data_dir}/selection-history.json`,
  schema-versioned, quarantined on a file it can't read.
- **Canonical dedupe key**: `mode | normalized_query | filters | case_sensitive` (four segments; Search's has six,
  adding `scope` and `exclude_system_dirs`). Filters serialize as alphabetically-keyed `k=v,k=v` with undefined fields
  omitted. The key is never persisted, only computed at compare time.
- **Cap**: `selection.recentSelections.maxCount` (default 1000). `apply_max_count` trims in-memory on live-apply; `0`
  clears everything and short-circuits future adds.
- **AI is cloud-only**: `translate_selection_query` hard-errors when the provider isn't `cloud` (mapped to
  `kind = notConfigured`). The frontend hides the AI chip in that case, but this gate is the belt-and-braces check for
  an MCP caller or a misconfigured frontend. Errors are the typed `AiTranslateError { kind, message }` shared with
  Search (`crate::ai::translate_error`); the dialog toasts a specific reason. Don't branch on the message
  (`no-error-string-match`).
- **Result shape is `pattern` + `kind`**, not structured filter types: the matcher runs in JS, so the parsed string IS
  the contract. `kind` is `glob` (full-name, `*` and `?`) or `regex` (JS RegExp). The result also carries optional
  `is_directory`, `size_*`, and `modified_*` for the chips. An omitted `type` from the model leaves the user's
  `Both | Files | Folders` choice alone.

## Coupling (all one-way; nothing depends on `selection`)

- `crate::search::history`: re-exports `HistoryMode` / `HistoryFilters` and the canonical-key helpers.
- `crate::recents`: the persisted-list machinery behind `RECENT_SELECTIONS`.
- `crate::ai::manager` + `crate::ai::client`: backend resolution and chat completion (mirrors `commands::search`).

The six IPC commands are registered in `crate::ipc::builder` and `crate::ipc_collectors::collect_cross_platform_types`;
typed wrappers in `apps/desktop/src/lib/tauri-commands/selection.ts`. Dialog frontend in
`apps/desktop/src/lib/selection-dialog/`.

Full details (IPC signatures, AI pipeline steps, the why behind separate files and the re-export):
`DETAILS.md`.
