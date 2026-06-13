# Selection module

Backend for the Selection dialog (Select files / Deselect files). Mirrors
`crate::search` but narrower: no scope, no system-dir exclusion, no in-memory
index, and the matcher itself runs in JS against the focused folder's entries.
This module owns just the persistent history store and the AI translation
pipeline.

## Module structure

- **`mod.rs`**: Re-exports the public surface.
- **`history.rs`**: `SelectionHistoryEntry`, atomic JSON read/write, canonical-key dedupe, cap eviction, schema-version quarantine. Re-exports `HistoryMode` and `HistoryFilters` from `crate::search::history` so the frontend sees the same mode/filter shape for both consumers.
- **`ai/`**: NL → glob/regex translation. Cloud-only. See [`ai/CLAUDE.md`](ai/CLAUDE.md) for the prompt / parser / builder split and the real-LLM eval.

The IPC layer is in `crate::commands::selection`.

## History store

Persistent recent-selections store for the dialog's footer + popover. Same
atomic-write story as `crate::search::history`; the key tradeoffs are:

- **Persistence path**: `{app_data_dir}/selection-history.json`. Schema-versioned
  via `_schemaVersion` (currently `1`).
- **In-memory cache + disk lock**: in-memory `Mutex<HistoryStore>` plus a
  separate `OnceLock<Mutex<()>>` (`DISK_LOCK`) that serializes the
  read-modify-write cycle so concurrent IPC commands can't lose writes. Cache
  guards drop before any `fs` call.
- **Canonical dedupe key**: `mode | normalized_query | filters | case_sensitive`.
  Four segments; Search's key has six (it adds `scope` and
  `exclude_system_dirs`). Filters serialize as alphabetically-keyed `k=v,k=v`
  pairs with undefined fields omitted. The key is never persisted; it only
  exists at compare time.
- **Recovery**: parse failure or schema-version mismatch → rename file to
  `.broken`, start fresh. The user keeps using the dialog; the corrupted file
  is preserved for one rotation in case debugging is needed.
- **Cap**: configurable via `selection.recentSelections.maxCount` (default
  1000). `apply_max_count` trims the in-memory store on live-apply; `0` clears
  everything and short-circuits future adds.

### Decision: separate `selection-history.json` from `search-history.json`

Storing both consumers' history in one file with a `kind` discriminator was
rejected. Their schemas already diverge (`scope` and `exclude_system_dirs` are
irrelevant for Selection), and coupling two unrelated migrations forever
didn't earn its keep. The small cost of two files is invisible at runtime.

### Decision: re-export `HistoryMode` and `HistoryFilters` from `search::history`

The two pure data types are identical in intent across the two consumers. The
`SelectionHistoryEntry` struct itself stays separate so the on-disk schema
doesn't bind Selection to Search's canonical-key shape. If Search's mode set
or filter shape ever diverges from Selection's, the re-export drops out and
the types fork; the wiring is already isolated enough that the change is
mechanical.

## AI translation

`translate_selection_query(prompt, sample_names, current_type)` orchestrates:

1. Verifies the AI provider is `cloud`. Hard-errors otherwise; the frontend
   hides the AI chip in that case, but this gate is the belt-and-braces check
   for an MCP caller or a misconfigured frontend.
2. Calls `ai::build_classification_prompt(&sample_names, current_type)` to
   assemble the system prompt with today's date, the folder sample, and the
   user's current `Both | Files | Folders` type as context (`current_type`:
   `Some(true)` folders / `Some(false)` files / `None` both). The model may set
   `type` or omit it; an omitted `type` keeps the user's choice (leave-alone).
3. Runs `chat_completion` via `crate::ai::client` with `temperature: 0.2`,
   `max_tokens: 300`, `top_p: 0.9`.
4. Parses via `ai::parse_selection_response` into `ParsedSelectionLlmResponse`.
5. Builds the wire-result via `ai::build_selection_translate_result`.

See [`ai/CLAUDE.md`](ai/CLAUDE.md) for the prompt design, parser tolerances,
caveat / kind defaulting rules, and the real-LLM eval.

### Decision: cloud-only AI for Selection

Folder samples weigh 1-3k tokens; the prompt plus completion lives ~4-5k
tokens. Local 4-8K context models often can't fit the full payload, and
quality on small models is unreliable for pattern inference. The frontend
surfaces a tooltip on the gated UI ("AI selection needs a cloud provider. Set
one in Settings > AI."); the backend returns the same message as a hard error
for any non-cloud caller.

### Decision: `pattern` + `kind` instead of structured filter types

The matcher runs on the frontend in JS. There's no benefit to round-tripping
a typed glob through Rust; the parsed string IS the contract. The kind is
`glob` (full-name match, `*` and `?` only) or `regex` (JS RegExp). The result
also carries optional `is_directory` (the file-vs-folder dimension), `size_*`,
and `modified_*`, which the frontend paints onto the chips.

## IPC surface

All commands live in `crate::commands::selection`:

- **`translate_selection_query(prompt, sample_names, current_type)`**: AI translation; cloud-only. `current_type` (the dialog's type toggle as `Option<bool>`) is passed as prompt context. Returns `SelectionTranslateResult` (now carrying optional `is_directory`) or a typed `AiTranslateError { kind, message }` (shared with Search; see `crate::ai::translate_error`) so the dialog toasts a specific reason. The cloud-only gate maps to `kind = notConfigured`.
- **`get_recent_selections(limit)`**: Returns persisted entries (newest first).
- **`add_recent_selection(entry, max_count)`**: Adds + dedupes + caps.
- **`remove_recent_selection(id)`**: Removes by id; no-op when missing.
- **`clear_recent_selections()`**: Drops every entry.
- **`apply_recent_selections_max_count(max_count)`**: Live-applies a freshly-tuned cap.

All six are registered in `crate::ipc::builder` (runtime dispatch) and
`crate::ipc_collectors::collect_cross_platform_types` (specta). The bindings
appear in `apps/desktop/src/lib/ipc/bindings.ts`; the typed wrappers live in
`apps/desktop/src/lib/tauri-commands/selection.ts`.

## Coupling to other modules

- `crate::search::history`: re-exports `HistoryMode` and `HistoryFilters`. One-way.
- `crate::ai::manager` + `crate::ai::client`: backend resolution and chat completion.
  Mirrors `crate::commands::search`'s usage exactly.
- `crate::config::resolved_app_data_dir`: shared persistence-path resolver.

No other modules depend on `selection`; the dialog frontend and command-dispatch wiring live in
`apps/desktop/src/lib/selection-dialog/` and `apps/desktop/src/lib/commands/`.
