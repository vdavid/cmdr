# Search (frontend)

Whole-drive file search dialog. Searches the in-memory index by filename (glob/regex), size, date, and scope (folder
include/exclude) filters. Optional AI mode translates natural language queries into structured filters.

Backend: `src-tauri/src/indexing/search.rs` (in-memory index), `src-tauri/src/commands/search.rs` (IPC commands). Full
design: `docs/specs/drive-search-plan.md`.

## Files

| File                     | Purpose                                                                       |
| ------------------------ | ----------------------------------------------------------------------------- |
| `SearchDialog.svelte`    | Dialog UI: input, filters, results list, keyboard nav, AI mode, accessibility |
| `search-state.svelte.ts` | Module-level `$state` for query fields, results, index readiness, AI state    |
| `search-state.test.ts`   | Vitest tests for state helpers (`parseSizeToBytes`, `buildSearchQuery`, etc.) |

## Data flow

```
User presses ⌘F
  -> +page.svelte sets showSearchDialog = true
  -> SearchDialog mounts, calls prepareSearchIndex() IPC
  -> Backend starts async index load (2-3s), emits "search-index-ready" when done
  -> User types pattern / sets filters -> 200ms debounce -> searchFiles(query) IPC
  -> Results displayed, keyboard nav with ↑/↓, Enter navigates to file
  -> Dialog close -> releaseSearchIndex() IPC -> 5 min idle timer -> index dropped
```

## Key patterns

**Command palette pattern**: Own fixed overlay + backdrop, not `ModalDialog`. Needs custom keyboard handling (arrow keys
for results, Tab between filters) that would fight `ModalDialog`'s focus management.

**Two-cursor hover model**: Same as command palette. `cursorIndex` (keyboard) and `hoveredIndex` (mouse) are
independent.

**Live search with debounce**: 200ms debounce on any input change. Enter bypasses debounce for immediate search.

**Scope field**: Between pattern and filter rows. Comma-separated folder paths with `!` prefix for exclusions. Parsed
via `parseSearchScope()` IPC call in `executeSearch()` (async, so not part of `buildSearchQuery()`). ⌥F sets scope to
the focused pane's current directory, ⌥D clears it. Info button `(i)` shows syntax help tooltip.

**Index not available state**: When indexing is disabled or not started, `prepareSearchIndex()` errors. The dialog shows
a message ("Drive index not ready...") with scan progress if available. Inputs and filters are disabled, AI button
hidden.

**AI row visibility**: When `ai.provider !== 'off'` and the index is available, the AI prompt row is always visible (top
row, with "AI" label and accent border) and focused by default. The pattern row (bottom, with search icon) is always
visible too. Enter in the AI prompt row triggers AI translation; Enter in the pattern row runs manual search. `⌘Enter`
from anywhere triggers AI search. When AI is disabled, only the pattern row is shown.

**AI single-pass flow**: `executeAiSearch()` calls `translateSearchQuery()` once (LLM classifies intent into enums +
extracts keywords, Rust builds the query deterministically), then runs `executeSearch()`. No preflight, no refinement
pass. See `docs/specs/ai-search-v2-plan.md` for the full design.

**AI prompt state**: `aiPrompt` in `search-state.svelte.ts` holds the natural language query separately from
`namePattern` (the glob/regex pattern).

**Deferred loading indicator**: The "Loading drive index..." message in the results area only appears when the user has
triggered a search while the index is still loading. On initial open, the results area is empty (no loading message)
since the user is still typing their query.

## Key decisions

**Decision**: Dialog, not a panel or sidebar. **Why**: Search is a focused, transient task. A command-palette-style
overlay matches this usage pattern and doesn't consume permanent screen real estate.

**Decision**: Structured filters always visible (not hidden behind "advanced"). **Why**: The filter row is compact (one
line) and makes the query model transparent. Users see exactly what's being searched.

## Gotchas

**Gotcha**: `stopPropagation()` on every `keydown`. **Why**: Without this, keys propagate to the file explorer behind
the dialog and trigger quick-search or navigation.

**Gotcha**: `prepareSearchIndex()` failure means index unavailable. **Why**: The backend returns an error when
`get_read_pool()` returns `None` (indexing disabled or not started). The dialog catches this and enters the disabled
state.

## References

- [AI search eval history](../../../../../docs/notes/ai-search-eval-history.md) -- Four rounds of prompt tuning for the
  AI natural language to structured query translation, with a 30-query test catalog and lessons learned.

## Dependencies

- `$lib/tauri-commands` -- `prepareSearchIndex`, `searchFiles`, `releaseSearchIndex`, `translateSearchQuery`,
  `parseSearchScope`
- `$lib/indexing` -- `isScanning`, `getEntriesScanned` (scan progress for unavailable state)
- `$lib/settings` -- `getSetting('ai.provider')` (AI button visibility)
- CSS variables from `app.css` (`--z-modal`, `--color-accent-subtle`, `--color-bg-secondary`, etc.)
