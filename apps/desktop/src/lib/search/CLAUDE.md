# Search (frontend)

Whole-drive file search dialog. Searches the in-memory index by filename (glob/regex), size, and date filters. Optional
AI mode translates natural language queries into structured filters.

Backend: `src-tauri/src/indexing/search.rs` (in-memory index), `src-tauri/src/commands/search.rs` (IPC commands). Full
design: `docs/specs/drive-search-plan.md`.

## Files

| File                     | Purpose                                                                       |
| ------------------------ | ----------------------------------------------------------------------------- |
| `SearchDialog.svelte`    | Dialog UI: input, filters, results list, keyboard nav, AI mode, accessibility |
| `search-state.svelte.ts` | Module-level `$state` for query fields, results, index readiness, AI mode     |
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

**Index not available state**: When indexing is disabled or not started, `prepareSearchIndex()` errors. The dialog shows
a message ("Drive index not ready...") with scan progress if available. Inputs and filters are disabled, AI button
hidden.

**AI mode**: `⌘L` or "Ask AI" button toggles AI mode. On Enter, calls `translateSearchQuery` IPC which returns
structured filters. Filters are populated in the UI with a brief highlight animation (radical transparency).

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

## Dependencies

- `$lib/tauri-commands` -- `prepareSearchIndex`, `searchFiles`, `releaseSearchIndex`, `translateSearchQuery`
- `$lib/indexing` -- `isScanning`, `getEntriesScanned` (scan progress for unavailable state)
- `$lib/settings` -- `getSetting('ai.provider')` (AI button visibility)
- CSS variables from `app.css` (`--z-modal`, `--color-accent-subtle`, `--color-bg-secondary`, etc.)
