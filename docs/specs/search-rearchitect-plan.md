# Search rearchitect plan

Split the search feature's large files into focused modules with single responsibilities. Currently `search.rs` (2361
lines), `SearchDialog.svelte` (1552 lines), and `ai_query_builder.rs` (1281 lines) each handle multiple concerns. This
refactoring untangles them into smaller, testable, well-bounded units without changing any behavior.

## Guiding principles

- **Zero behavior changes.** Every milestone must pass all existing tests with no functional delta. This is purely
  structural. No new features, no performance optimizations, no API changes from the frontend's perspective.
- **Move, don't rewrite.** Copy code blocks verbatim into their new homes, update imports, verify tests. Resist the
  temptation to "clean up while we're here" -- that muddies the diff and hides regressions.
- **One reviewable PR per milestone.** Each milestone is independently mergeable. If we abandon halfway, the repo is
  still coherent.
- **Tests follow their code.** When a function moves, its `#[cfg(test)]` block moves with it to the same file.
- **Honest boundaries.** Don't pretend a module is independent when it isn't. `search/` is a read-only consumer of the
  indexing DB -- document that relationship clearly rather than hiding it behind abstractions.

## Current state

| File | Lines | Responsibilities |
|------|-------|------------------|
| `src-tauri/src/indexing/search.rs` | 2361 | Data structures, index lifecycle (load/drop/timers/global state), query types, scope parsing, glob-to-regex, search execution, path reconstruction, scope filtering, format helpers, `fill_directory_sizes` (DB I/O), system dir excludes, tests |
| `src-tauri/src/commands/search.rs` | 564 | IPC commands + AI classification prompt + AI backend resolution + `iso_date_to_timestamp` + dir size post-filtering + IPC-only types (`TranslateResult`, `TranslatedQuery`, `TranslateDisplay`) |
| `src-tauri/src/commands/ai_query_builder.rs` | 1281 | LLM enum->SearchQuery mapping, type->regex, time->timestamp, keyword merging, caveat gen, display gen. Imports `expand_tilde` from `commands::file_system` |
| `src-tauri/src/commands/ai_response_parser.rs` | 403 | LLM key-value response parsing, field validation, fallback keywords |
| `src/lib/search/SearchDialog.svelte` | 1552 | AI input row, pattern input row, scope row, filter row, column headers, results list, keyboard nav, status bar, all CSS |
| `src/lib/search/search-state.svelte.ts` | 292 | Fine as-is |

### External consumers of `indexing::search` (beyond `commands/search.rs`)

These files import directly from `crate::indexing::search` and must be updated when the module moves:
- `mcp/resources.rs` -- imports `format_size`
- `mcp/executor.rs` -- imports `PatternType`, `SearchQuery`, `SearchResult`, `SearchResultEntry`, `format_size`,
  `format_timestamp`, `fill_directory_sizes`, `summarize_query`. Also imports `iso_date_to_timestamp` and
  `TranslateResult` from `crate::commands::search` -- the `iso_date_to_timestamp` import breaks in milestone 2
  when that function moves to `search/ai/query_builder.rs`

## Target state

### Backend: new `src-tauri/src/search/` module

```
src-tauri/src/search/
+-- mod.rs              # Re-exports from submodules (flat re-exports so consumers can
|                       #   `use crate::search::{SearchQuery, search, ...}`)
+-- index.rs            # SearchIndex, SearchEntry, SearchIndexState, SEARCH_INDEX global,
|                       #   load_search_index, drop_search_index, touch_activity, timers
+-- engine.rs           # search() — PURE (no I/O). ScopeFilter, prepare_scope_filter,
|                       #   reconstruct_path_from_index, derive_icon_id
+-- types.rs            # SearchQuery, SearchResult, SearchResultEntry, PatternType, ParsedScope,
|                       #   default_limit — pure data definitions, no logic
+-- query.rs            # parse_scope, glob_to_regex, SYSTEM_DIR_EXCLUDES, summarize_query,
|                       #   resolve_include_paths (DB pre-query), fill_directory_sizes (DB post-query),
|                       #   format_size, format_timestamp — operations on the types
+-- ai/
|   +-- mod.rs          # Re-exports
|   +-- prompt.rs       # CLASSIFICATION_PROMPT, build_classification_prompt
|   +-- parser.rs       # ParsedLlmResponse, parse_llm_response, validate_*, fallback_keywords
|   +-- query_builder.rs # type_to_filter, time_to_range, size_to_filter, scope_to_paths,
|                       #   keywords_to_pattern, merge_keyword_and_type, build_search_query,
|                       #   build_translate_display, build_translated_query, generate_caveat,
|                       #   iso_date_to_timestamp
+-- CLAUDE.md           # New -- documents the search module architecture
```

### Design rationale

**`engine.rs` is truly pure -- no I/O, no DB access.** It takes an `&SearchIndex` + `&SearchQuery`, scans in-memory with
rayon, and returns results. Trivially testable with synthetic data, no mocks needed. This is the key architectural win:
the hot path is isolated from all side effects.

**`query.rs` owns the search data types and all DB-touching operations on them.** This includes `resolve_include_paths`
(pre-query: resolves path strings to entry IDs via SQLite) and `fill_directory_sizes` (post-query: enriches directory
results with recursive sizes from `dir_stats`). Both operate on `SearchQuery`/`SearchResult` which are defined here.
The data flow is: `query.rs` prepare -> `engine.rs` scan -> `query.rs` enrich.

`format_size` and `format_timestamp` also live here because `summarize_query` uses them. They're general-purpose
formatting helpers that happen to live in the search module -- if more modules start needing them, extract to a shared
utils module later.

**`index.rs` owns mutable global state.** Loading the index from DB, dropping it, idle/backstop timers, cancellation.
Completely separate concern from the pure search scan.

**`ai/` sub-module is a self-contained pipeline** that only connects to the rest of search at the `SearchQuery`
boundary. Prompt tuning, new enum values, and parser changes are isolated here.

**`expand_tilde` stays in `commands/file_system.rs`.** It's used by 20+ call sites across 4 files (`file_system.rs`,
`rename.rs`, `file_viewer/session.rs`, and the AI query builder). Moving it to a shared module would be clean but
touches too many files for this refactoring. The AI query builder imports it via `crate::commands::file_system::expand_tilde` -- not ideal architecturally (business logic importing from IPC layer), but pragmatic. Worth a
separate cleanup PR later.

### Backend: slim `commands/search.rs`

After the move, `commands/search.rs` becomes a truly thin IPC layer:
- `prepare_search_index` -- delegates to `search::index`
- `search_files` -- delegates to `search::engine` + `search::query` (the dir size post-filter loop stays here since
  it's orchestration logic specific to the IPC command)
- `release_search_index` -- delegates to `search::index`
- `translate_search_query` -- delegates to `search::ai` (still owns `resolve_ai_backend` since that touches `crate::ai`
  and `crate::settings`, which are Tauri-app concerns)
- `parse_search_scope` -- delegates to `search::query`
- `get_system_dir_excludes` -- delegates to `search::query`
- IPC-only types: `PrepareResult`, `TranslateResult`, `TranslatedQuery`, `TranslateDisplay`

`ai_query_builder.rs` and `ai_response_parser.rs` are deleted from `commands/`.

### Frontend: split `SearchDialog.svelte` into 3 components

```
src/lib/search/
+-- SearchDialog.svelte        # Orchestrator: overlay, mount/unmount, keyboard dispatch,
|                              #   search execution, state wiring (~350-400 lines)
+-- AiSearchRow.svelte         # AI prompt input + Ask AI button + caveat + status/error
+-- SearchInputArea.svelte     # Pattern row + scope row + filter row (all query configuration)
+-- SearchResults.svelte       # Column headers + results list + empty/loading states + status bar
+-- search-state.svelte.ts     # Unchanged
+-- search-state.test.ts       # Unchanged
```

**Why 3, not 5?** Pattern, scope, and filters are all "how you configure your search query." They share props
(`disabled`, `highlightedFields`, `scheduleSearch` callback) and change together conceptually. Splitting them into 3
separate files would add prop-passing ceremony for thin components (~80-100 lines each) that don't justify their own
files. Combined as `SearchInputArea` (~350 lines), each row is still a clear section within the file, and the component
is cohesive.

AI search is a separate component because it's a distinct feature (toggleable, has its own execution path, own
status/error state). Results are separate because they're purely display-oriented with their own complex states
(unavailable, loading, searching, empty, populated) and own interaction model (keyboard nav, mouse hover, column resize).

### Svelte component boundary details

- **Element refs across boundaries**: `aiPromptInputElement` and `patternInputElement` (`bind:this`) are used by the
  orchestrator's `focusActiveInput()` but live inside child components. In Svelte 5, children expose these via
  `$bindable()` props: `let { inputElement = $bindable() }: { inputElement: HTMLInputElement | null } = $props()`.
  The parent binds: `<SearchInputArea bind:patternInputElement={patternInputElement} />`.
- **`hoveredIndex` bidirectional state**: Set by mouse events in `SearchResults.svelte`, read/reset in the orchestrator.
  Pass as a `$bindable()` prop with `bind:hoveredIndex`.
- **`hasSearched` flag**: Set in orchestrator's `executeSearch`/`executeAiSearch`, read by `SearchResults.svelte` to
  distinguish "no query yet" from "no results". Pass as a regular (read-only) prop.
- **`executeAiSearch` stays in the orchestrator**: It mutates filter state via many setters and then calls
  `executeSearch`. `AiSearchRow` receives an `onAiSearch: (query: string) => void` callback prop.
- **Keyboard handling stays in the orchestrator**: Captures `keydown` on the overlay. Uses `document.activeElement` or
  focus tracking to determine which input is active for Enter dispatch.
- **CSS**: Each component takes its own scoped CSS. The `.input-row` class (6 lines) is duplicated in `AiSearchRow` and
  `SearchInputArea` (it's trivial). Overlay/dialog container styles stay in the orchestrator.

### `indexing/` cleanup

`search.rs` is removed from `indexing/`. The `indexing/CLAUDE.md` is updated to remove the search.rs documentation. A
new `search/CLAUDE.md` is created. `docs/architecture.md` is updated to reflect the new structure.

The remaining dependencies from `search/` to `indexing/` are:
- `search::index` imports `ReadPool` from `indexing::enrichment` and `WRITER_GENERATION` from `indexing::writer`
- `search::engine` imports `ROOT_ID` and `normalize_for_comparison` from `indexing::store`
  (used by `ScopeFilter::matches()` and `reconstruct_path_from_index`)
- `search::query` imports `store::resolve_path`, `IndexStore`, and `ROOT_ID` from `indexing::store`

This is an intentional, documented coupling. `search/` is a read-only consumer of the indexing DB -- it reads entries
and dir_stats but never writes. The dependency is one-way (`search` -> `indexing`, never reverse) and narrow (3 modules
from `indexing` are used: `enrichment::ReadPool`, `writer::WRITER_GENERATION`, `store`).

### Visibility changes needed

- `SearchIndex::name()` is currently private (`fn`). It must become `pub(crate)` since `engine.rs` (a sibling module)
  calls it on `SearchIndex` which lives in `index.rs`.
- All items in the new submodules that need cross-submodule access use `pub(crate)` -- same visibility as today, just
  across file boundaries instead of within a single file.

## Milestones

### Milestone 1: Create `src-tauri/src/search/` -- move and split `search.rs`

1. Create `src-tauri/src/search/mod.rs`, `index.rs`, `engine.rs`, `query.rs`
2. Move code from `indexing/search.rs` to the three new files:
   - `index.rs`: `SearchEntry`, `SearchIndex` (make `name()` `pub(crate)`), `SearchIndexState`, `SEARCH_INDEX`,
     `DIALOG_OPEN`, `LAST_SEARCH_ACTIVITY`, constants (`IDLE_TIMEOUT`, `BACKSTOP_TIMEOUT`, `CANCEL_CHECK_INTERVAL`),
     `now_secs`, `touch_activity`, `load_search_index`, `drop_search_index`, `start_idle_timer`, `start_backstop_timer`
   - `engine.rs` (pure, no I/O): `search()`, `ScopeFilter`, `prepare_scope_filter`, `reconstruct_path_from_index`,
     `derive_icon_id`, and any private helpers these use. Imports `ROOT_ID` and `normalize_for_comparison` from
     `crate::indexing::store`.
   - `query.rs`: `SearchQuery`, `SearchResult`, `SearchResultEntry`, `PatternType`, `ParsedScope`, `parse_scope`,
     `split_scope_segments`, `glob_to_regex`, `SYSTEM_DIR_EXCLUDES`, `summarize_query`, `resolve_include_paths`,
     `fill_directory_sizes`, `default_limit`, `format_size`, `format_timestamp`
3. Set up `mod.rs` with flat re-exports so consumers can `use crate::search::{SearchQuery, search, format_size, ...}`
4. Move `#[cfg(test)]` blocks alongside their functions in each file
5. Delete `indexing/search.rs`, remove `pub(crate) mod search` from `indexing/mod.rs`
6. Add `pub mod search` to `lib.rs` (it's a binary crate, so `pub` vs `pub(crate)` doesn't matter; use `pub` to match
   the other top-level modules)
7. Update all `use crate::indexing::search::*` paths to `crate::search::*` in:
   - `commands/search.rs`
   - `mcp/executor.rs` (imports `PatternType`, `SearchQuery`, `SearchResult`, `SearchResultEntry`, `format_size`,
     `format_timestamp`, `fill_directory_sizes`, `summarize_query`)
   - `mcp/resources.rs` (imports `format_size`)
   - Any other files found by `grep -r "indexing::search" src-tauri/src/`
8. Run `cargo nextest run` -- all existing search tests must pass
9. Run `./scripts/check.sh --rust` -- clippy + fmt must pass

### Milestone 2: Move AI pipeline from `commands/` to `search/ai/`

1. Create `src-tauri/src/search/ai/mod.rs`, `prompt.rs`, `parser.rs`, `query_builder.rs`
2. Move `commands/ai_response_parser.rs` -> `search/ai/parser.rs`
3. Move `commands/ai_query_builder.rs` -> `search/ai/query_builder.rs`
4. Move `iso_date_to_timestamp` from `commands/search.rs` -> `search/ai/query_builder.rs` (it serves the AI pipeline;
   the `commands/search.rs` tests for it move too)
5. Extract `CLASSIFICATION_PROMPT`, `build_classification_prompt` from `commands/search.rs` -> `search/ai/prompt.rs`
6. `resolve_ai_backend` stays in `commands/search.rs` (it imports from `crate::ai` and `crate::settings`)
7. Move tests:
   - `test_iso_date_to_timestamp*` tests -> `search/ai/query_builder.rs`
   - `test_classification_prompt_*` tests -> `search/ai/prompt.rs`
   - `test_translate_result_serialization` stays in `commands/search.rs` (the serialization types remain there)
8. Delete `commands/ai_query_builder.rs` and `commands/ai_response_parser.rs`, remove their `pub mod` from
   `commands/mod.rs`
9. Update `search/ai/mod.rs` with re-exports, update imports in `commands/search.rs` to use `crate::search::ai::*`
10. Fix import path changes in the moved files:
    - `ai/query_builder.rs`: change `super::file_system::expand_tilde` to `crate::commands::file_system::expand_tilde`
      (it used `super::` when it lived in `commands/`, now needs the full path)
    - `ai/query_builder.rs`: change `super::ai_response_parser` to `super::parser` (sibling under `search/ai/`)
    - `ai/parser.rs`: check for any `super::` imports that need updating
11. Update `mcp/executor.rs`: change `crate::commands::search::iso_date_to_timestamp` to
    `crate::search::ai::iso_date_to_timestamp` (or the re-export path from `crate::search`)
12. Run `cargo nextest run` -- all AI search tests must pass
13. Run `./scripts/check.sh --rust`

### Milestone 3: Split `SearchDialog.svelte` into 3 subcomponents

1. Extract `AiSearchRow.svelte` -- AI prompt input, Ask AI button, caveat row, AI status/error. Props:
   `inputElement: $bindable()`, `aiPrompt`, `onInput`, `onAiSearch`, `disabled`, `caveatText`, `aiStatus`, `aiError`,
   `highlightedFields`
2. Extract `SearchInputArea.svelte` -- pattern row + scope row + filter row. Props:
   `patternInputElement: $bindable()`, `namePattern`, `patternType`, `caseSensitive`, `scope`, `excludeSystemDirs`,
   `currentFolderPath`, `sizeFilter`, `sizeValue`, `sizeUnit`, `sizeValueMax`, `sizeUnitMax`, `dateFilter`,
   `dateValue`, `dateValueMax`, `systemDirExcludeTooltip`, `highlightedFields`, `disabled`,
   plus callbacks: `onInput`, `onSelect`, `onSearch`, `onTogglePatternType`, `onToggleCaseSensitive`,
   `onToggleExcludeSystemDirs`, `onSetScope`, `scheduleSearch`
3. Extract `SearchResults.svelte` -- column headers (with resize handles), results list (all states: unavailable,
   loading, searching, no results, results), status bar. Props: `results`, `cursorIndex`,
   `hoveredIndex: $bindable()`, `isIndexAvailable`, `isIndexReady`, `isSearching`, `hasSearched`, `namePattern`,
   `sizeFilter`, `dateFilter`, `scanning`, `entriesScanned`, `totalCount`, `indexEntryCount`, `gridTemplate`,
   `onResultClick`, `onColumnDragStart`, `iconCacheVersion`
4. `SearchDialog.svelte` becomes the orchestrator -- overlay, keyboard dispatch, mount/unmount, search execution
   functions (`executeSearch`, `executeAiSearch`, `scheduleSearch`, `applyAiFilters*`, `applySizeFilters`,
   `applyDateFilters`), state wiring to child components via props/callbacks, column resize state/handlers
5. Run `cd apps/desktop && pnpm vitest run` -- all search state tests must pass
6. Run `./scripts/check.sh --svelte`
7. Manual test: open search dialog, verify AI mode, manual mode, filters, scope, keyboard nav, results all work

### Milestone 4: Update docs and path references

1. Create `src-tauri/src/search/CLAUDE.md` documenting the new module structure, data flow, key decisions, and gotchas.
   Be honest about the coupling: "`search/` is a read-only consumer of the indexing DB via `ReadPool`,
   `WRITER_GENERATION`, and `store::resolve_path`. This is intentional -- search reads from the index but doesn't
   participate in indexing."
2. Update `indexing/CLAUDE.md` -- remove the `search.rs` paragraph (substantial block starting with "**search.rs** --
   In-memory search index..."), add a one-line cross-reference: "Search module: `src-tauri/src/search/CLAUDE.md`"
3. Update `commands/CLAUDE.md` -- remove `ai_query_builder.rs` and `ai_response_parser.rs` from the file map, update
   `search.rs` description to "Thin IPC wrappers over `search` module. `resolve_ai_backend` for AI provider config."
4. Update `src/lib/search/CLAUDE.md` -- update the file table with new components, update backend path references
5. Update `docs/architecture.md`:
   - Backend table: add `search/` row, update `indexing/` description to remove search mention, update
     `indexing/search.rs` row to point to `search/`
   - Search section: update paths from `indexing/search.rs` to `search/`, from `commands/search.rs` to reflect thin
     IPC layer, mention `search/ai/` for the AI pipeline
6. Update inline code comment in `search-state.svelte.ts` line 50: change `indexing/search.rs` to `search/query.rs`
7. Historical specs (`docs/specs/drive-search-plan.md`, `docs/specs/ai-search-v2-plan.md`) -- leave as-is, they're
   point-in-time design docs, not living references
8. Run `./scripts/check.sh` -- full check pass

## Testing strategy

- **Rust**: All existing `#[cfg(test)]` tests move with their functions, including tests in `commands/search.rs`
  (`iso_date_to_timestamp`, `classification_prompt_*`). No new tests needed -- we're not changing behavior.
  `cargo nextest run` catches any broken imports or moved-but-not-updated references.
- **Svelte**: Existing `search-state.test.ts` is unaffected (state module doesn't change). The component split has no
  unit tests since the components are purely presentational -- manual testing covers the integration.
- **Manual**: After milestone 3, open the app, press Cmd+F, verify: AI row appears/hides based on setting, pattern
  search works, scope field works (opt+F, opt+D, manual input), filters work, keyboard navigation works (up/down/
  Enter/Esc), column resize works, all result states render (loading, no results, results).
- **CI**: `./scripts/check.sh` at the end of each milestone catches clippy, fmt, Svelte lint, and test regressions.

## Risks and mitigations

- **Circular imports**: `search::engine` needs types from `search::query` and `search::index`, plus `ROOT_ID` and
  `normalize_for_comparison` from `indexing::store`. All search submodules are siblings under `search/mod.rs`, so
  intra-module imports are straightforward. The `indexing` -> `search` dependency is one-way.
- **Visibility changes**: `SearchIndex::name()` must become `pub(crate)` for cross-file access. Other items keep their
  existing `pub(crate)` visibility -- just across file boundaries now.
- **Svelte component boundaries**: The tricky parts are element refs (solved with `$bindable()` props), `hoveredIndex`
  bidirectionality (also `$bindable()`), and keyboard dispatch needing to know active input (solved with
  `document.activeElement` checks or focus tracking). All execution functions (`executeSearch`, `executeAiSearch`, etc.)
  stay in the orchestrator.
- **CSS specificity**: Moving CSS to child components is safe since Svelte scopes styles. The `.input-row` class (6
  lines) is duplicated in children that use it.
- **MCP module imports**: `mcp/executor.rs` and `mcp/resources.rs` import from `indexing::search` -- must update in
  milestone 1 step 7 or the build breaks. `mcp/executor.rs` also imports `iso_date_to_timestamp` from
  `commands::search` -- must update in milestone 2 step 11.
- **`expand_tilde` import inversion**: After milestone 2, `search/ai/query_builder.rs` imports
  `crate::commands::file_system::expand_tilde` -- business logic reaching into the IPC layer. This works but is
  architecturally backwards. A follow-up PR could move `expand_tilde` to a shared utils module, but that touches 20+
  call sites across 4 files and is out of scope for this refactoring.

## Future improvements (out of scope)

- **`expand_tilde` to shared utils**: Move from `commands/file_system.rs` to a top-level `utils.rs` or `path_utils.rs`.
  Affects `commands/file_system.rs` (13 uses), `commands/rename.rs` (5 uses), `file_viewer/session.rs` (1 use),
  `search/ai/query_builder.rs` (1 use). Small PR, big diff.
- **`format_size`/`format_timestamp` to shared utils**: If more modules need them beyond search and MCP. Not worth a
  new module for two 10-line functions today.
