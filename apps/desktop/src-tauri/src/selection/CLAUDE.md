# Selection module

Backend for the Selection dialog (Select files / Deselect files). Mirrors `crate::search` but narrower: no scope, no
system-dir exclusion, no in-memory index. The matcher itself runs in JS against the focused folder's entries; this
module owns only the persistent history store and the AI translation pipeline. IPC lives in `crate::commands::selection`.

## Module map

- **`mod.rs`**: re-exports the public surface.
- **`history.rs`**: `SelectionHistoryEntry`, atomic JSON read/write, canonical-key dedupe, cap eviction, schema-version
  quarantine. Re-exports `HistoryMode` and `HistoryFilters` from `crate::search::history` so both consumers share one
  mode/filter shape.
- **`ai/`**: NL → glob/regex translation, cloud-only. See `ai/CLAUDE.md`.

## Must-knows

- **History persistence path**: `{app_data_dir}/selection-history.json`, schema-versioned via `_schemaVersion`
  (currently `1`). On parse failure or version mismatch, the file is renamed `.broken` and a fresh store starts (the
  user keeps working; the corrupt file is kept one rotation for debugging).
- **Concurrency**: an in-memory `Mutex<HistoryStore>` cache plus a separate `OnceLock<Mutex<()>>` (`DISK_LOCK`) that
  serializes the read-modify-write cycle so concurrent IPC commands can't lose writes. Drop the cache guard before any
  `fs` call.
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

- `crate::search::history`: re-exports `HistoryMode` / `HistoryFilters`.
- `crate::ai::manager` + `crate::ai::client`: backend resolution and chat completion (mirrors `commands::search`).
- `crate::config::resolved_app_data_dir`: shared persistence-path resolver.

The six IPC commands are registered in `crate::ipc::builder` and `crate::ipc_collectors::collect_cross_platform_types`;
typed wrappers in `apps/desktop/src/lib/tauri-commands/selection.ts`. Dialog frontend in
`apps/desktop/src/lib/selection-dialog/`.

Full details (IPC signatures, AI pipeline steps, the why behind separate files and the re-export):
`DETAILS.md`.
