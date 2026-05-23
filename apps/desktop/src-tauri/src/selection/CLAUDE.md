# Selection module

Backend for the Selection dialog (Select files / Deselect files). Mirrors `crate::search`
but narrower: there is no scope, no system-dir exclusion, no in-memory index, and the
matcher itself runs in JS against the focused folder's entries. This module owns just
the persistent history store and the AI translation pipeline.

## Module structure

| File | Purpose |
|------|---------|
| `mod.rs` | Re-exports the public surface. |
| `history.rs` | `SelectionHistoryEntry`, atomic JSON read/write, canonical-key dedupe, cap eviction, schema-version quarantine. Re-exports `HistoryMode` and `HistoryFilters` from `crate::search::history` so the frontend sees the same mode/filter shape for both consumers. |
| `ai/mod.rs` | Re-exports the AI submodules. |
| `ai/prompt.rs` | `build_classification_prompt(sample_names)` and `format_sample_block`. Pure functions; no IPC. Returns the system-prompt string the LLM receives. |
| `ai/parser.rs` | `parse_selection_response(text)` → `ParsedSelectionLlmResponse`. Key-value line parser; mirrors `search::ai::parser` in style but with the narrower field set. |
| `ai/query_builder.rs` | `build_selection_translate_result(parsed)` → `SelectionTranslateResult`. Assembles the result type that crosses IPC; `generate_caveat` and `build_label` are the supporting helpers. |

The IPC layer is in `crate::commands::selection`.

## History store

Persistent recent-selections store for the dialog's footer + popover. Same atomic-write
story as `crate::search::history`; the key tradeoffs are:

- **Persistence path**: `{app_data_dir}/selection-history.json`. Schema-versioned via
  `_schemaVersion` (currently `1`).
- **In-memory cache + disk lock**: in-memory `Mutex<HistoryStore>` plus a separate
  `OnceLock<Mutex<()>>` (`DISK_LOCK`) that serializes the read-modify-write cycle so
  concurrent IPC commands can't lose writes. Cache guards drop before any `fs` call.
- **Canonical dedupe key**: `mode | normalized_query | filters | case_sensitive`. Four
  segments; Search's key has six (it adds `scope` and `exclude_system_dirs`). Filters
  serialize as alphabetically-keyed `k=v,k=v` pairs with undefined fields omitted. The
  key is never persisted; it only exists at compare time.
- **Recovery**: parse failure or schema-version mismatch → rename file to `.broken`,
  start fresh. The user keeps using the dialog; the corrupted file is preserved for
  one rotation in case debugging is needed.
- **Cap**: configurable via `selection.recentSelections.maxCount` (default 1000).
  `apply_max_count` trims the in-memory store on live-apply; `0` clears everything and
  short-circuits future adds.

### Decision: separate `selection-history.json` from `search-history.json`

Storing both consumers' history in one file with a `kind` discriminator was rejected.
Their schemas already diverge (`scope` and `exclude_system_dirs` are irrelevant for
Selection), and coupling two unrelated migrations forever didn't earn its keep. The
small cost of two files is invisible at runtime.

### Decision: re-export `HistoryMode` and `HistoryFilters` from `search::history`

The two pure data types are identical in intent across the two consumers. The
`SelectionHistoryEntry` struct itself stays separate so the on-disk schema doesn't bind
Selection to Search's canonical-key shape. If Search's mode set or filter shape ever
diverges from Selection's, the re-export drops out and the types fork; the wiring is
already isolated enough that the change is mechanical.

## AI translation

The `translate_selection_query(prompt, sample_names)` IPC orchestrates:

1. Verifies the AI provider is `cloud`. Small local models (4-8K context) can't
   reliably fit a 200+-name folder sample plus the structured prompt and response, so
   the backend hard-errors when provider isn't cloud. The frontend hides the AI chip
   in that case, but this gate is the belt-and-braces check for an MCP caller or a
   misconfigured frontend.
2. Calls `ai::build_classification_prompt(&sample_names)` to assemble the system
   prompt with today's date and the folder sample.
3. Runs `chat_completion` via `crate::ai::client` against the configured cloud backend
   with `temperature: 0.2`, `max_tokens: 300`, `top_p: 0.9`.
4. Parses the response via `ai::parse_selection_response` into a
   `ParsedSelectionLlmResponse`.
5. Builds the wire-result via `ai::build_selection_translate_result`.

### Decision: cloud-only AI for Selection

Folder samples weigh 1-3k tokens; the prompt plus completion lives ~4-5k tokens. Local
4-8K context models often can't fit the full payload, and quality on small models is
unreliable for pattern inference. We surface a tooltip on the gated UI in the frontend
("AI selection needs a cloud provider. Set one in Settings > AI."); the backend
returns the same message as a hard error for any non-cloud caller.

### Decision: key-value response format, not JSON

Same rationale as `crate::search::ai`. JSON generation is the #1 failure mode for
small LLMs. Key-value lines are trivial to produce and parse, missing lines are
individually skippable, and malformed lines never void the whole response.

### Decision: `pattern` + `kind` instead of structured filter types

The matcher runs on the frontend in JS. There's no benefit to round-tripping a typed
glob through Rust; the parsed string IS the contract. The kind is `glob` (full-name
match, `*` and `?` only) or `regex` (JS RegExp). When `pattern` is missing or blank,
`kind` is forced to `None` so the frontend doesn't compile a half-built query.

### Decision: default `kind` to `glob` when the model omits it

The model occasionally forgets to emit `kind:` for obvious globs (`*.png`, `*.log`).
Defaulting saves a re-prompt. The parser still drops `kind` to `None` when the value
isn't one of `glob`/`regex`; the builder catches the missing-kind-with-pattern case
and substitutes `glob`.

## Real-LLM eval results

The prompt + parser are pinned by `selection/ai/real_llm_eval_test.rs`, six
`#[ignore]`-gated integration tests against the live OpenAI API. Run them with:

```sh
OPENAI_API_KEY=$(security find-generic-password -a "$USER" -s "OPENAI_API_KEY" -w) \
  cargo nextest run --lib --run-ignored only selection::ai::real_llm_eval_test
```

The default model is `gpt-4o-mini` (cheap, fast, comparable to the model David has
configured in his Settings UI for everyday use). When David's cloud-provider model
changes, edit `MODEL` in the eval file and rerun.

| Intent | Sample shape | Assertions | Status |
|---|---|---|---|
| "all log files" | mixed `.log` / `.txt` / `.md` / `.png` | pattern contains `log`, `kind` set | passing |
| "png and jpg images" | mixed image + text extensions | pattern mentions both png and jpg/jpeg | passing |
| "files bigger than 5 MB" | mixed sizes | `size_min` ∈ [4 MB, 10 MB], pattern present | passing |
| "backups from last week" | `*-backup-*` files plus noise | `modified_after` set | passing |
| "every rymd file" | `rymd-*.pdf` plus noise | pattern matches the keyword | passing |
| "final drafts I haven't shared" | `Final-*` files | pattern OR caveat present (no half-built query) | passing |

The eval also surfaces drift: a prompt change that breaks one of these assertions
shows up before the dialog wraps around it. Iterate the prompt, rerun the eval, ship
the prompt change with green tests.

For ad-hoc debugging (peek at the raw model response), add an `eprintln!` to the
`translate` helper temporarily (allowed in `#[cfg(test)]` blocks for `--no-capture`
runs); revert before commit so the crate-level deny on `print_stderr` stays clean.
Alternatively, run the dialog through the live app and tail
`RUST_LOG=cmdr_lib::selection::ai=debug pnpm dev`.

## IPC surface

All commands live in `crate::commands::selection`:

| Command | Purpose |
|---|---|
| `translate_selection_query(prompt, sample_names)` | AI translation; cloud-only. Returns `SelectionTranslateResult` or an error string. |
| `get_recent_selections(limit)` | Returns persisted entries (newest first). |
| `add_recent_selection(entry, max_count)` | Adds + dedupes + caps. |
| `remove_recent_selection(id)` | Removes by id; no-op when missing. |
| `clear_recent_selections()` | Drops every entry. |
| `apply_recent_selections_max_count(max_count)` | Live-applies a freshly-tuned cap. |

All six are registered in `crate::ipc::builder` (runtime dispatch) and
`crate::ipc_collectors::collect_cross_platform_types` (specta). The bindings appear
in `apps/desktop/src/lib/ipc/bindings.ts`; the typed wrappers live in
`apps/desktop/src/lib/tauri-commands/selection.ts`.

## Coupling to other modules

- `crate::search::history`: re-exports `HistoryMode` and `HistoryFilters`. One-way.
- `crate::ai::manager` + `crate::ai::client`: backend resolution and chat completion.
  Mirrors `crate::commands::search`'s usage exactly.
- `crate::config::resolved_app_data_dir`: shared persistence-path resolver.

No other modules depend on `selection`; the dialog frontend and command-dispatch wiring
land in M7.
