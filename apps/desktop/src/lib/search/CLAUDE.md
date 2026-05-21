# Search (frontend)

Whole-drive file search dialog. Searches the in-memory index by filename (glob/regex), size, date, and scope (folder
include/exclude) filters. Optional AI mode translates natural language queries into structured filters.

Backend: `src-tauri/src/search/` (index, engine, query, AI pipeline), `src-tauri/src/commands/search.rs` (thin IPC
wrappers).

Dialog width: 1080 px (was 900 px). Internal layout is fluid; no fixed inner widths. The bump leaves room for the filter
chip row and path-pill column landing in later milestones.

## Files

| File                             | Purpose                                                                                      |
| -------------------------------- | -------------------------------------------------------------------------------------------- |
| `SearchDialog.svelte`            | Orchestrator: overlay, mount/unmount, keyboard dispatch, search execution, state wiring      |
| `SearchBar.svelte`               | Unified query input: one `<input>` for AI / filename / regex, placeholder updates per mode   |
| `SearchModeChips.svelte`         | Mode chip row below the bar: AI / Filename / Content (disabled) / Regex, arrow-key navigable |
| `SearchInputArea.svelte`         | Scope row + filter row. M3 will turn these into chips with popovers                          |
| `SearchResults.svelte`           | Column headers + results list + all states (loading, empty, populated) + status bar          |
| `search-state.svelte.ts`         | Module-level `$state` for query fields, results, index readiness, AI state                   |
| `search-state.test.ts`           | Vitest tests for state helpers (`parseSizeToBytes`, `buildSearchQuery`, etc.)                |
| `SearchBar.svelte.test.ts`       | Per-mode placeholder, value mirror, `onInput` callback                                       |
| `SearchModeChips.svelte.test.ts` | Chip set, active marker, click + keyboard activation, focus motion (skipping Content)        |
| `SearchDialog.svelte.test.ts`    | `⌘N` clears, close+reopen preserves, `⌘1`/`⌘2`/`⌘3` mode switch, `⌘Enter` triggers AI        |
| `SearchDialog.a11y.test.ts`      | Tier-3 axe-core audit across loading / index-ready / AI-on macro-states                      |
| `SearchInputArea.a11y.test.ts`   | Tier-3 axe-core audit across scope + filter row variants                                     |
| `SearchResults.a11y.test.ts`     | Tier-3 axe-core audit across result states                                                   |

## State shape (post-M2)

The user's typed text and the active mode are one model:

```ts
let query = $state('') // The text in the bar
let mode = $state<SearchMode>('filename') // 'ai' | 'filename' | 'regex'
```

`buildSearchQuery()` reads both: `mode === 'regex'` produces `patternType: 'regex'`, anything else produces
`patternType: 'glob'`. AI mode is only ever invoked via `executeAiSearch()`, which calls `translateSearchQuery` and then
overwrites `query` + `mode` with the AI's result (so the user sees what was searched). M4 will surface the original
prompt in a transparency strip; for M2 it lives only in the user's memory.

There is **no `aiPrompt` state and no `namePattern` state**. M2 deleted both. Anywhere the old code read `aiPrompt` or
`namePattern`, the new code reads `query`. Anywhere the old code branched on `patternType`, the new code branches on
`mode` (with the mapping `regex => regex`, everything else => glob).

## Keyboard shortcuts (in-dialog, hard-coded)

| Shortcut  | Action                                                            |
| --------- | ----------------------------------------------------------------- |
| `Enter`   | Run search in the active mode (AI in AI mode, manual otherwise)   |
| `⌘Enter`  | Run AI search regardless of active mode (only when AI is enabled) |
| `⌘N`      | Clear all dialog state ("new search")                             |
| `⌘1`      | Switch to AI (AI on) or Filename (AI off)                         |
| `⌘2`      | Switch to Filename (AI on) or Regex (AI off)                      |
| `⌘3`      | Switch to Regex (AI on); no-op when AI is off                     |
| `⌘4`      | Reserved for Content when it ships; not wired now                 |
| `⌥F`      | Set scope to the focused pane's current directory                 |
| `⌥D`      | Clear the scope (search the whole drive)                          |
| `↑` / `↓` | Move the cursor through the results list                          |
| `←` / `→` | When focus is on a mode chip: move between chips (skip Content)   |
| `Tab`     | Trapped within the dialog; cycles through interactive elements    |
| `Escape`  | Close the dialog                                                  |

The Content chip is visible-disabled with a "Coming soon" tooltip. It has **no** shortcut. Wiring a shortcut to a
disabled control is hostile UX (either silent no-op or a popup on every press); reserving `⌘4` is the better contract.
When Content ships, it claims `⌘3` and Regex moves to `⌘4`.

## Data flow

```
User presses ⌘F
  -> +page.svelte sets showSearchDialog = true
  -> SearchDialog mounts, calls prepareSearchIndex() IPC
  -> Backend starts async index load (2-3s), emits "search-index-ready" when done
  -> User types in the bar -> 200ms debounce -> searchFiles(query) IPC (filename/regex modes only)
  -> User presses Enter in AI mode -> translateSearchQuery -> populates filters -> searchFiles
  -> Results displayed, keyboard nav with ↑/↓, Enter navigates to file
  -> Dialog close -> releaseSearchIndex() IPC -> 5 min idle timer -> index dropped
```

## Key patterns

**Command palette pattern**: Own fixed overlay + backdrop, not `ModalDialog`. Needs custom keyboard handling (arrow keys
for results, Tab between filters) that would fight `ModalDialog`'s focus management.

**Two-cursor hover model**: Same as command palette. `cursorIndex` (keyboard) and `hoveredIndex` (mouse) are
independent.

**Live search with debounce**: 200ms debounce on filename/regex modes only. AI mode never auto-applies: the AI call
costs money and the user must explicitly opt in via Enter / `⌘Enter` / the chip's Run action. Enter bypasses debounce
for immediate search.

**Scope row**: Below the chips. Comma-separated folder paths with `!` prefix for exclusions. Parsed via
`parseSearchScope()` IPC call in `executeSearch()` (async, so not part of `buildSearchQuery()`). ⌥F sets scope to the
focused pane's current directory, ⌥D clears it. Info button `(i)` shows syntax help tooltip.

**Index not available state**: When indexing is disabled or not started, `prepareSearchIndex()` errors. The dialog shows
a message ("Drive index not ready...") with scan progress if available. Inputs and filters are disabled.

**AI single-pass flow**: `executeAiSearch()` calls `translateSearchQuery()` once (LLM classifies intent into enums +
extracts keywords, Rust builds the query deterministically), then runs `executeSearch()`. No preflight, no refinement
pass. The previous two-pass system caused ~15% regressions; deterministic structure means there's nothing to refine.

**AI overwrites the bar**: After AI translates, the bar shows the AI's translated pattern (filename / regex), and `mode`
flips accordingly. The user sees what was searched and can keep iterating. The original natural-language prompt is
preserved only in the user's memory until M4 ships the transparency strip.

**Auto mode fallback when AI gets disabled mid-session**: If the AI provider is switched off while the dialog is open
and the active mode is `ai`, the dialog quietly flips to `filename`. The user wouldn't be able to run a search
otherwise.

**Deferred loading indicator**: The "Loading drive index..." message in the results area only appears when the user has
triggered a search while the index is still loading. On initial open, the results area is empty (no loading message)
since the user is still typing their query.

**State preservation across close + reopen**: The module-level `$state` in `search-state.svelte.ts` survives dialog
unmount. Closing the dialog (Escape or overlay click) does NOT wipe query, mode, filters, scope, results, or cursor.
Reopening the dialog lands the user back where they left off. The only reset path is `⌘N` ("new search") inside the
dialog, which calls `clearSearchState()` and refocuses the bar.

**`⌘N` shortcut**: Hard-coded in `SearchDialog.svelte`'s `handleModifierShortcuts`. Captured before the dialog's global
`stopPropagation` would let it reach the route-level `⌘N` (new tab) handler. The choice of `⌘N` matches the macOS "new
X" idiom (new tab, new document) for the same reason the user reads "fresh search" the same way.

## Key decisions

**Decision**: Unified search bar plus mode chips instead of two separate input rows. **Why**: The AI prompt and the
filename pattern were two ways to ask the same question, sitting in two rows, each with its own input outlined. That
made them feel like competing features. Collapsing to one input with a chip-row discriminator below mirrors the modal
patterns of Spotlight and Raycast, halves the visual weight of the dialog's top, and lets the chip-row carry visible
keyboard hints (`⌘1`/`⌘2`/`⌘3`) without crowding the input.

**Decision**: No shortcut wired to the disabled Content chip. **Why**: A shortcut that silently no-ops on a disabled
control is worse than no shortcut. Reserving `⌘4` for Content when it ships keeps the user's mental model intact (the
numbers match the visible chip positions) and avoids "I pressed the key and nothing happened" moments.

**Decision**: Dialog, not a panel or sidebar. **Why**: Search is a focused, transient task. A command-palette-style
overlay matches this usage pattern and doesn't consume permanent screen real estate.

**Decision**: Structured filters always visible (not hidden behind "advanced"). **Why**: The filter row is compact (one
line) and makes the query model transparent. Users see exactly what's being searched. M3 will move them into chips with
popovers, but they stay always-on.

## Gotchas

**Gotcha**: `stopPropagation()` on every `keydown`. **Why**: Without this, keys propagate to the file explorer behind
the dialog and trigger quick-search or navigation.

**Gotcha**: `prepareSearchIndex()` failure means index unavailable. **Why**: The backend returns an error when
`get_read_pool()` returns `None` (indexing disabled or not started). The dialog catches this and enters the disabled
state.

**Gotcha**: Don't call `clearSearchState()` from `onDestroy`. **Why**: The dialog's lifecycle (mount on open, unmount on
close) doesn't match the user's mental model of "the search I was working on." Wiping state on unmount turned every
close + reopen into a lost-work moment. The only sanctioned reset path is `⌘N`. If you find yourself wanting to wipe
state from a lifecycle hook, you probably want a user-initiated action instead.

**Gotcha**: The AI's translation overwrites `query` and `mode`. **Why**: We want the bar to show what was searched, not
the natural-language prompt. Until M4 ships the transparency strip, the original prompt is only in the user's memory.
Anyone building on top of this should not assume `query` still contains the user's natural-language input after an AI
run.

## References

- [AI search eval history](../../../../../docs/notes/ai-search-eval-history.md) -- Four rounds of prompt tuning for the
  AI natural language to structured query translation, with a 30-query test catalog and lessons learned.

## Dependencies

- `$lib/tauri-commands` -- `prepareSearchIndex`, `searchFiles`, `releaseSearchIndex`, `translateSearchQuery`,
  `parseSearchScope`
- `$lib/indexing` -- `isScanning`, `getEntriesScanned` (scan progress for unavailable state)
- `$lib/settings` -- `getSetting('ai.provider')` (AI chip visibility, ⌘ shortcut numbering)
- `$lib/tooltip/tooltip` -- chip tooltips (Content chip's "Coming soon" copy)
- CSS variables from `app.css` (`--z-modal`, `--color-accent-subtle`, `--color-bg-secondary`, etc.)
