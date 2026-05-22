# Selection dialog and the QueryDialog unification

Spec for adding "Select files…" / "Deselect files…" to Cmdr, by unifying it with the existing whole-drive Search
dialog under a shared `QueryDialog` primitive.

## Why

Cmdr's selection model is rich (Space, Insert, Shift+arrow toggle-and-fill, ⌘A, ⌘⇧A), but power users have no way to
select files by a pattern. Total Commander and Double Commander both ship an "Expand selection" dialog (`+` / `-`) that
lets the user select all files matching a wildcard. David wants the same in Cmdr, plus an AI mode: "Select all
rymdskottkärra files for me" should infer `*rymd*` from a sample of the current folder.

We already shipped a tightly-polished filter UI in `lib/search/`: unified query bar with AI / Filename / Regex chips,
filter chips with popovers for size and modified date, recent-searches footer and popover, AI transparency strip,
deferred loading indicator, IME composition guard, ⌘N reset, ⌘H recent-history fuzzy search, auto-apply debounce.
Building a second dialog from scratch would either duplicate that UI (poor) or skim it (worse). The right move is to
recognize that filter-and-act-on is one primitive, and ship Selection as a second consumer of the same primitive.

The natural shape: extract a `QueryDialog` component out of today's `SearchDialog`, with the data source, AI prompt
context, history store, and apply action as injected behavior. Search becomes the first consumer; Selection is the
second. The same query bar, same mode chips, same filter chips, same recent items, same keyboard shortcuts, same
"Press Enter" hint, all share the same code by construction.

This is the elegance-over-duplication call from `AGENTS.md` § Principles: invest in finding the right tradeoff.

## Goals

- Add commands `selection.selectFiles` (shortcut `=`, no modifier) and `selection.deselectFiles` (`-`, no modifier).
  Both open the same dialog with a `mode` prop that flips the title, the verb on the apply button, and which way
  matched indices flow into the focused pane's selection set.
- New top-level macOS menu **Select** between Edit and View. Move existing `selection.selectAll` (⌘A) and
  `selection.deselectAll` (⌘⇧A) into it. Add the two new items.
- One `QueryDialog` component shared between Search and Selection. ~95% of the dialog (every visible part except the
  footer verb, the title, the data source, and the AI prompt's context) is identical across consumers.
- Same UX affordances in both consumers: filter chips (size, modified), recent-items footer with a fuzzy popover, AI
  transparency strip when AI is on, IME guard, auto-apply gated by `search.autoApply`, ⌘N reset, ⌘H popover.
- Refactor `lib/settings/components/SettingToggleGroup.svelte`'s implementation to use a new generic
  `lib/ui/ToggleGroup.svelte` so Settings and the new mode chips share the same segmented-control primitive (no UX
  drift, and one component to maintain).
- Selection's AI sees a sample of the focused folder's filenames as context (first ~200, ~20 around the cursor, ~20 at
  the end if longer). Cost is fine; David has $2.5k in credits expiring soon.
- Recent selections persist (`selection-history.json`) with the same schema shape as recent searches, but a separate
  file (no shared budget, no mixing in the popover).
- Selection works in `search-results://` snapshot panes too: matching runs against the full path (which is what the
  user sees in those panes) instead of the basename.

## Non-goals

- Recursive selection across subfolders. Always current folder only.
- "Select only folders / only files" filter. Defer.
- Case-sensitivity toggle in the dialog UI. Defer; off by default. We'll add it to the bar later when we add it to
  Search too.
- An "open in pane" action from the Selection dialog. Selection's only commit action is "apply to focused pane".
- MCP entry point for Selection. Search has one (`open_search_dialog`); Selection can get one later if needed.
- Live preview that mutates the focused pane's selection as the user types. Apply only on commit (⏎ or button click).
- Switching Search's results column layout to match Selection or vice versa. Both use the existing virtualized
  `SearchResults` list verbatim (full path column shows nothing useful for Selection-in-current-folder, but the same
  component renders fine; the path column just stays empty for those entries). See Risk register R4.

## Design summary

### Shape: one QueryDialog, two consumers

`SearchDialog.svelte` today is the orchestrator: overlay, mount/unmount, keyboard dispatch, AI translation, search
execution, snapshot-store wiring, MCP listener, `runOnMount` for prefill. We rename and shrink it: the orchestration
moves into a new `QueryDialog.svelte` that takes a `QueryDialogConfig` prop carrying everything that diverges per
consumer.

The full diff between Search and Selection:

| Aspect                                | Search                                                    | Selection                                                     |
|---------------------------------------|-----------------------------------------------------------|---------------------------------------------------------------|
| Title                                 | "Search"                                                  | "Select files" / "Deselect files"                             |
| Max width                             | `min(1080px, 80vw)`                                       | `min(720px, 60vw)`                                            |
| Data source                           | `searchFiles` IPC against the drive index                 | JS matcher over `pane.getEntries()` (sync)                    |
| AI translate IPC                      | `translate_search_query(prompt)`                          | `translate_selection_query(prompt, sampledNames[])`           |
| AI context                            | none                                                      | sampled filenames from the focused folder                     |
| Primary action                        | "Show all in main window ⌥⏎"                              | "Select these files ⏎" / "Deselect these files ⏎"             |
| Secondary action                      | "Go to file ⏎"                                            | none                                                          |
| Empty-state hint about index size     | "Index ready: 5,000,000 files"                            | (omitted)                                                     |
| Empty-state example chips             | "PDFs from this week", "*.svg", "/log\d/"                 | "all image files", "logs newer than a week", "files >5 MB"    |
| Scope filter chip ("Search in")       | yes                                                       | hidden                                                        |
| Pattern filter chip                   | yes                                                       | yes (same role: shows AI-translated pattern in AI mode)       |
| History store                         | `search-history.json`                                     | `selection-history.json`                                      |
| Index prepare / release lifecycle     | yes                                                       | no                                                            |
| MCP listener                          | yes                                                       | no                                                            |
| `search-results://` snapshot promotion| yes                                                       | no                                                            |
| Title bar                             | new, "Search"                                             | new, "Select files" / "Deselect files"                        |

Everything else is shared verbatim: query bar, mode chips (AI / Filename / Regex), AI transparency strip, size + date
filter chips with popovers, recent-items footer with fuzzy popover, results table with path pills and row menus, all
keyboard shortcuts (⌘N, ⌘H, ⌘1/⌘2/⌘3, ⌥A/⌥F/⌥R, ⌥←/⌥→, ↑/↓, IME guard, ⏎ ownership swap via `deriveEnterAction`),
auto-apply debounce, "Press Enter to search" hint logic, `lastDialogEvent` lifecycle, IME guard, `runOnMount` prefill
hook (Search uses it for MCP, Selection ignores it but the hook exists in the shared component).

### Why this is the right abstraction

A "share primitives only" approach (separate dialogs, shared subcomponents) was the obvious first move. We reject it
because:

1. The dialog orchestrator is where 90% of the polish lives: keyboard contract, `deriveEnterAction`, IME guard,
   auto-apply gates, `lastDialogEvent` driving Enter ownership, the title-bar / chip / strip / list / footer layout
   contract. Duplicating that for Selection means duplicating drift surface. The two dialogs would diverge over time
   no matter the discipline.
2. Every UX improvement we ship to Search should land in Selection for free, and vice versa. Shared primitives don't
   guarantee that; shared orchestration does.
3. The set of things that genuinely differs (data source, AI context, apply verb, history file, title, width) is small
   and easy to express as injected behavior. The leaky-abstraction risk is low.

A third consumer would slot in cleanly: e.g. a future "Filter visible" dialog that applies a temporary filter to the
focused pane's list. Two consumers don't justify the abstraction on their own; this third future consumer is the
sanity check that we're not over-engineering.

### Naming

- `lib/query-ui/` is the home of the shared primitives.
- `lib/query-ui/QueryDialog.svelte` is the orchestrator.
- `lib/query-ui/QueryBar.svelte` (renamed from `SearchBar`).
- `lib/query-ui/QueryModeToggleGroup.svelte` (renamed from `SearchModeChips`; built on top of
  `lib/ui/ToggleGroup.svelte`).
- `lib/query-ui/AiPromptStrip.svelte` (renamed from `AiTransparencyStrip`).
- `lib/query-ui/FilterChips.svelte` (renamed from `SearchFilterChips`; scope chip and Pattern chip become props).
- `lib/query-ui/recent-items/` for the recent-items footer, popover, and factory store.
- `lib/query-ui/query-filter-state.svelte.ts` (factory replacing the module-singleton `search-state.svelte.ts`).
- `lib/search/` keeps Search-specific code: `SearchDialog.svelte` wrapper (thin) that wires Search's config; snapshot
  store; `search-results://` machinery; MCP listener wiring; index lifecycle.
- `lib/selection-dialog/` is the new feature: `SelectionDialog.svelte` (thin wrapper), `selection-matching.ts` (pure
  matcher), `folder-sampler.ts` (pure sampler), `selection-history-state.svelte.ts` (consumer of the recent-items
  factory).

Why "QueryDialog" and not "SearchDialog" for the shared name: calling Selection a "search" misleads readers. "Query"
covers filter-and-act-on across both consumers and any future ones. The component is responsible for showing the
query, not for what the query searches over.

### Match semantics

The matcher is a pure function `match(name: string, pattern: string, opts: { mode: 'glob' | 'regex'; caseSensitive: boolean }): boolean`.

For Selection, it takes a `getNameFor(index: number) → string` accessor passed by the dialog: regular panes return
`entry.name`, `search-results://` panes return `entry.path` (which is what the user sees in those panes' list, per the
M8 fixup). Same matcher, different accessor.

Glob mode reuses the same translation as Search's filename mode: `*` and `?` only, anchored. Regex mode uses
JavaScript's `RegExp` directly (Search currently delegates to Rust for regex; Selection doesn't need to round-trip
through IPC because the matcher runs in JS).

Size and date filters from the chip strip apply as additional predicates. Selection's matcher composes all three:
pattern + size + modified.

### Apply action

The dialog returns a list of matched indices to its parent. For Search, this is the existing snapshot-promotion path
(unchanged). For Selection, the parent (`+page.svelte`) calls `explorerRef.applyIndicesToFocusedPaneSelection(indices, mode)`
where `mode: 'add' | 'remove'`. Implementation in `FilePane.selection`:

```ts
// In selection-state.svelte.ts
export function applyIndices(idxs: number[], mode: 'add' | 'remove'): void {
  for (const i of idxs) {
    if (mode === 'add') selectedIndices.add(i)
    else selectedIndices.delete(i)
  }
  onChanged?.()
}
```

Range anchor / end state stays untouched.

### AI translation for Selection

A new Rust IPC `translate_selection_query(prompt: String, sample_names: Vec<String>)` returns a
`SelectionTranslateResult { pattern, kind: 'glob' | 'regex', size_filter, date_filter, caveat?, label? }`. Same shape
as Search's `TranslateResult` minus `scope`, `exclude_system_dirs`, and `folders`.

The prompt is simpler than Search's. It receives the sampled filenames and asks the model to identify a glob or regex
that matches the user's intent, plus optional size / modified filters. Prompt skeleton (the implementing agent will
refine; the structure is locked):

```text
You're helping select files in a folder. Below is a sample of the folder's filenames.

Sample:
{sampled_names}

User intent: {prompt}

Return the smallest glob or regex that selects the matching files.
{key-value response schema same as search/ai/parser.rs uses}
```

Folder sampling strategy (`folder-sampler.ts`, pure):

- 0–200 entries: all of them.
- 201+ entries: first 200, plus 20 around the cursor (cursorIndex ± 10), plus 20 from the end of the list.
- De-dup; order doesn't matter for the LLM.
- Return at most 240 names.

### History store

A new Rust module `src-tauri/src/selection/history.rs` mirrors `src-tauri/src/search/history.rs` line-for-line, with a
narrower entry schema:

```rust
pub struct SelectionHistoryEntry {
  pub id: String,
  pub timestamp: i64,
  pub mode: HistoryMode, // reuse the existing enum from search/history.rs? See decision below
  pub query: String,
  pub filters: HistoryFilters, // reuse from search/history.rs
  pub case_sensitive: bool,
  pub match_count: u32,
}
```

**Decision**: re-export `HistoryMode` and `HistoryFilters` from `search/history.rs` rather than copying. Reusing the
two pure data types is fine. The entry struct itself is separate so the schema doesn't bind selection to search's
canonical-key shape.

File: `{app_data_dir}/selection-history.json`. Schema v1. Cap setting:
`selection.recentSelections.maxCount` (default 1000), live-applied via the same applier pattern.

IPC: `get_recent_selections`, `add_recent_selection`, `remove_recent_selection`, `clear_recent_selections`,
`apply_recent_selections_max_count`. specta regenerates the bindings.

Frontend mirrors: `selection-history-state.svelte.ts` uses the recent-items factory store (new) which reads from
whichever set of IPC functions the consumer passes in. The factory makes both Search and Selection consume their
respective IPCs through the same store shape.

### Title bar

A new minimal title bar at the top of the dialog: 32 px tall, `--font-size-md` weight 500, centered. Only the title
text and (optional) a close button on the right. Both Search and Selection render it. Spec:

```svelte
<header class="query-dialog__title">
  <span>{title}</span>
</header>
```

```css
.query-dialog__title {
  height: 32px;
  padding: 0 var(--spacing-lg);
  border-bottom: 1px solid var(--color-border-subtle);
  font-size: var(--font-size-md);
  font-weight: 500;
  color: var(--color-text-secondary);
  display: flex;
  align-items: center;
  justify-content: center;
}
```

(We will NOT use `ModalDialog`. The search dialog explicitly avoids it because the custom keyboard handling fights
`ModalDialog`'s focus management. The new title bar is a thin addition to the existing custom overlay.)

### Mode chips: `lib/ui/ToggleGroup.svelte`

A new generic primitive in `lib/ui/ToggleGroup.svelte` with props
`{ value: string; options: ToggleOption[]; onChange: (value: string) => void; ariaLabel: string; disabled?: boolean }`
where `ToggleOption = { value: string; label: string; badge?: string; hint?: string; disabled?: boolean; tooltip?: string; ariaLabel?: string }`.

`SettingToggleGroup.svelte` becomes a thin wrapper that reads a setting definition and builds the options array.
`QueryModeToggleGroup.svelte` (the new home of the AI / Filename / Regex chips) is another thin wrapper.

The ToggleGroup uses Ark UI's `ToggleGroup` primitive under the hood (same as today's SettingToggleGroup) so a11y and
keyboard navigation come for free. CSS uses the same scoped tokens already in SettingToggleGroup.svelte; the new
features (badge, hint, disabled-with-tooltip) extend the option-cell template.

### Where this all goes

```
apps/desktop/src/lib/
  ui/
    ToggleGroup.svelte              ← new generic primitive
    ToggleGroup.a11y.test.ts
    ToggleGroup.test.ts
  settings/components/
    SettingToggleGroup.svelte       ← refactored to use lib/ui/ToggleGroup
  query-ui/                         ← new shared home
    QueryDialog.svelte              ← was SearchDialog.svelte
    QueryDialog.svelte.test.ts
    QueryDialog.a11y.test.ts
    QueryBar.svelte                 ← was SearchBar.svelte
    QueryBar.svelte.test.ts
    QueryBar.a11y.test.ts
    QueryModeToggleGroup.svelte     ← was SearchModeChips.svelte
    QueryModeToggleGroup.svelte.test.ts
    QueryModeToggleGroup.a11y.test.ts
    AiPromptStrip.svelte            ← was AiTransparencyStrip.svelte
    AiPromptStrip.svelte.test.ts
    AiPromptStrip.a11y.test.ts
    FilterChips.svelte              ← was SearchFilterChips.svelte
    FilterChips.svelte.test.ts
    FilterChips.a11y.test.ts
    FilterChip.svelte               ← unchanged content; relocated
    FilterChipPopover.svelte        ← unchanged content; relocated
    filter-chip-state.ts            ← unchanged content; relocated
    filter-popover-helpers.ts       ← unchanged content; relocated
    QueryResults.svelte             ← was SearchResults.svelte; uses entry adapter
    QueryResults.a11y.test.ts
    PathPills.svelte                ← unchanged; relocated
    PathPills.svelte.test.ts
    SearchRowMenu.svelte            ← unchanged; relocated, retain name (it's a row menu)
    EmptyState.svelte               ← unchanged shape; examples now come from config
    EmptyState.svelte.test.ts
    recent-items/
      RecentItemsFooter.svelte      ← was RecentSearchesFooter.svelte
      RecentItemsPopover.svelte     ← was RecentSearchesPopover.svelte
      recent-items-state.svelte.ts  ← factory; was recent-searches-state.svelte.ts
      recent-chips-layout.ts        ← unchanged; relocated
      recent-searches-utils.ts      ← renamed recent-items-utils.ts (pure helpers)
    query-filter-state.svelte.ts    ← factory; was search-state.svelte.ts
    enter-action.ts                 ← unchanged; relocated
    snapshot-label.ts               ← STAYS in lib/search/ (search-specific)
    capabilities.ts                 ← STAYS in lib/search/ (search-specific)
  search/
    SearchDialog.svelte             ← thin wrapper now: builds QueryDialogConfig, passes through
    SearchFooterActions.svelte      ← stays (Go to file / Show all in main window are search-specific)
    SearchResultsView.svelte        ← stays (search-results pane view)
    snapshot-store.svelte.ts        ← stays
    snapshot-label.ts               ← stays
    capabilities.ts                 ← stays
    CLAUDE.md                       ← shrinks; defers to query-ui
  selection-dialog/                 ← new feature
    SelectionDialog.svelte          ← thin wrapper now: builds QueryDialogConfig, passes through
    SelectionFooterAction.svelte    ← single "Select these files" / "Deselect these files" button
    selection-matching.ts           ← pure matcher
    selection-matching.test.ts
    folder-sampler.ts               ← pure
    folder-sampler.test.ts
    selection-history-state.svelte.ts ← consumes the recent-items factory
    CLAUDE.md
  commands/
    command-registry.ts             ← add selection.selectFiles, selection.deselectFiles; relabel scope
  shortcuts/
    shortcuts-store.ts              ← register new menu commands
apps/desktop/src-tauri/src/
  search/
    history.rs                      ← extract HistoryMode/HistoryFilters to common place? See Decision below
  selection/                        ← new module
    mod.rs
    history.rs                      ← mirror of search/history.rs with narrower entry
    ai/
      mod.rs
      prompt.rs                     ← selection-specific classification prompt
      parser.rs                     ← key-value parser; reuses search/ai/parser.rs's parse helpers
      query_builder.rs              ← assembles SelectionTranslateResult
  commands/
    selection.rs                    ← new IPC: translate_selection_query, history CRUD
  menu/                             ← extend with the new "Select" top-level menu
```

### IPC contract changes

- Add: `translate_selection_query(prompt, sample_names)`, `get_recent_selections`, `add_recent_selection`,
  `remove_recent_selection`, `clear_recent_selections`, `apply_recent_selections_max_count`.
- Modify: none. Search's existing IPCs stay byte-compatible.
- specta regen runs after these land. `bindings-fresh` CI check must pass.

### Keyboard contract

Per `lib/search/CLAUDE.md` (the existing table), all shortcuts stay as they are in Search. Selection inherits all of
them. Two new globals route through the existing dispatch in `command-dispatch.ts`:

| Shortcut | Where | Action |
|----------|-------|--------|
| `=` (Shift+=, key `'+'`) | focused pane | Opens Selection dialog in `add` mode |
| `-` | focused pane | Opens Selection dialog in `remove` mode |

Both bind via `event.key === '+'` / `event.key === '-'` so non-QWERTY layouts that produce the same key event also
work. Must NOT trigger if any modifier (⌘ / ⌥ / ⌃ / ⇧ other than the implicit shift inside `+`) is held. The dispatch
guard checks `!e.metaKey && !e.altKey && !e.ctrlKey` and that the focus is on the pane (not in an input).

`⏎` inside the Selection dialog dispatches via the same `deriveEnterAction` state machine. The two outcomes map to
`'run-query'` (run the matcher and refresh the preview) and `'apply-selection'` (apply to focused pane and close).
Same logic as Search; different verb at the end.

### Menu structure

New top-level menu **Select** between Edit and View:

```
File   Edit   Select   View   Window   Help
                ↓
                Select all                ⌘A
                Deselect all              ⌘⇧A
                ─────────
                Select files…             =
                Deselect files…           -
```

`menu.rs` adds the new top-level submenu. Move `selection.selectAll` and `selection.deselectAll` out of Edit. Add the
two new items. Update `menu_id_to_command` and `command_id_to_menu_id` accordingly. Update `menuCommands` in
`shortcuts-store.ts`.

### Empty state copy

Selection's `EmptyState` uses the same three-example block as Search, with different example chips and no
"Index ready" line. The implementing agent picks final copy in M5; here's the seed:

```
Try…

[ ✨ all image files ]     [ ✨ logs newer than a week ]     [ ✨ files bigger than 5 MB ]
or
[ *.pdf ]                   [ *report* ]                       [ /^\d{4}-/ ]

Press ↓ to navigate matches • ⌘N to start over
```

(Match the existing Search EmptyState's structure exactly; we're just swapping the examples and dropping one line.)

## Decision log

- **`QueryDialog` over `SearchDialog` as the shared name.** Selection isn't a search; it's a filter-and-act-on.
  "Query" covers both. Future consumers slot in without renaming.
- **`lib/query-ui/` over `lib/shared/`.** "Shared" is meaningless ("shared with what?"). `query-ui` names the concept.
- **One `QueryDialog` over two dialogs with shared subcomponents.** The orchestrator carries 90% of the polish; sharing
  only subcomponents wouldn't share the polish.
- **Factory `createQueryFilterState()` over module-singleton state.** Two consumers need two state instances. The
  refactor is mechanical (~100 call sites) but unblocks everything else.
- **Move `SearchModeChips` into a generic `ToggleGroup` even though Settings's version is narrower.** Both Settings and
  Query benefit from a single segmented-control primitive. The generic carries the union of features (badge, hint,
  per-option disabled with tooltip); Settings just doesn't use the extras.
- **Frontend-only matcher for Selection (no Rust IPC for matching).** Current-folder match against in-memory entries
  is microseconds in JS. A Rust IPC would add IPC overhead with no benefit.
- **Same RegExp engine in JS for Selection's regex mode; no IPC round-trip.** Search's regex mode goes to Rust because
  it runs against millions of indexed names; Selection runs against hundreds, so JS is fine.
- **Separate `selection-history.json` over a shared file with a `kind` discriminator.** Schemas diverge (scope,
  exclude_system_dirs irrelevant for Selection). Storing them together would couple two unrelated migrations forever.
- **Reuse `HistoryMode` and `HistoryFilters` types from `search/history.rs`.** The two pure data types are identical
  intent; the entry struct stays separate.
- **Move filter chips (`FilterChips.svelte`) into `query-ui/` despite the scope chip being search-only.** Render the
  scope chip conditionally. Keeps all chips in one component; Selection just hides the scope chip via the visibility
  prop. Same goes for the Pattern chip (Selection uses it the same way: shows AI-translated pattern in AI mode).
- **Apply on commit, not live during preview.** Live-apply would mutate the focused pane's selection as the user types
  and would need an undo path on Esc. Apply on commit (⏎ or button) is simple, predictable, and matches user
  expectations from Total Commander.
- **Selection's AI receives a folder sample.** Without context, the model can't infer a pattern from "all
  rymdskottkärra files". Sampled filenames cost a few cents per call but make the AI mode actually work for the
  user's intent.
- **Drop the case-sensitivity toggle from v1.** Default off matches Double Commander's default; we'll add the toggle
  to the bar later when we add it to Search too. Avoids per-consumer divergence.
- **Add a title bar to the existing custom overlay rather than wrapping in `ModalDialog`.** The custom overlay's
  keyboard handling is exactly what this dialog needs; ModalDialog would fight it. 30 lines of title-bar CSS is
  cheaper than re-litigating focus management.
- **Bare `=` and `-` shortcuts, no modifier.** David confirmed `=` is unused and `-` is safe (⌘- is zoom out; bare `-`
  is free).
- **No "Open in pane" for Selection.** Selection's terminal action is "apply to the pane". There's no equivalent of
  Search's snapshot promotion. The footer's secondary slot stays empty.

## Milestones

Each milestone:

- One commit (or a small handful), FF-merged to main without review per David.
- Tests written ahead of code where reasonable (TDD).
- `./scripts/check.sh` green before the milestone closes. `--include-slow` only at the very end (M11).
- CLAUDE.md files updated in lockstep with the code they describe.

### M1: Generic `ToggleGroup` in `lib/ui/`

**Why.** Phase 0 of the design: one segmented-control primitive used by Settings and the mode chips. Lets us replace
`SearchModeChips`'s bespoke button row in a later milestone without rebuilding the keyboard / a11y story.

**What.**

- Create `lib/ui/ToggleGroup.svelte` with the option shape `{ value, label, badge?, hint?, disabled?, tooltip?, ariaLabel? }`.
- Built on Ark UI's `ToggleGroup` (same as today's SettingToggleGroup, so a11y and keyboard nav come for free).
- Same scoped CSS tokens as today's SettingToggleGroup. Visual diff: zero unless the option carries a badge or hint.
- Refactor `lib/settings/components/SettingToggleGroup.svelte` to be a thin wrapper that builds the options array from
  the setting definition and delegates to `lib/ui/ToggleGroup`. Its public API (`{ id, disabled, labelOverrides }`)
  doesn't change.
- Add `ToggleGroup.test.ts` and `ToggleGroup.a11y.test.ts` (tier 3) covering: option rendering, badge rendering, hint
  rendering, disabled cell with tooltip, click activation, arrow-key motion, focus order.

**Docs.**

- Add a short section to `lib/ui/CLAUDE.md` documenting the new primitive and how to use it.

**Checks.**

- `./scripts/check.sh --fast` after each substantial edit.
- `./scripts/check.sh` before commit.

**Definition of done.**

- New primitive in place, settings ToggleGroup migrated, all existing settings tests green, new tests green, no visual
  regression to Settings (verify by running the app, opening Settings > Appearance > File and folder sizes which uses
  the size-unit toggle group).

**Risk.** None significant. Settings has tests; if Ark's behavior shifts, those tests will catch it.

### M2: Factory `createQueryFilterState()` in `lib/query-ui/`

**Why.** Today's `lib/search/search-state.svelte.ts` is a 713-line module-singleton. Two consumers need two state
instances. We convert it to a factory and Search becomes a consumer of one instance. Without this, none of the
following milestones work.

**What.**

- Create `lib/query-ui/query-filter-state.svelte.ts` exporting `createQueryFilterState(options): QueryFilterState`.
  Options carry consumer-specific defaults (e.g. `defaultMode: 'filename'`, the auto-apply debounce constant,
  initial state).
- `QueryFilterState` exposes the same shape as today's named exports from `search-state.svelte.ts`, but as instance
  methods / getters. Naming follows the existing public API (`getQuery`, `setQuery`, `setQueryFromUserInput`, etc.).
- `lib/search/search-state.svelte.ts` becomes a thin file: `export const searchQueryState = createQueryFilterState({...search-defaults...})` plus
  re-exports of helper functions and types for backward compat during transition.
- Update Search's call sites to import from `searchQueryState` (e.g. `searchQueryState.getQuery()`). About 100 sites.
- Move the pure helpers (`enter-action.ts`, `filter-chip-state.ts`, `filter-popover-helpers.ts`, `recent-chips-layout.ts`,
  `snapshot-label.ts`, `searchable-folder.ts`) out of `lib/search/` and into `lib/query-ui/` (except `snapshot-label.ts`
  and `searchable-folder.ts` and `capabilities.ts` which stay in `lib/search/`).
- All existing Search tests stay verbatim, just with import paths updated. Add `query-filter-state.test.ts` mirroring
  the existing `search-state.test.ts` against a factory instance.

**Docs.**

- New `lib/query-ui/CLAUDE.md` documenting the factory's contract and how to instantiate it.
- Update `lib/search/CLAUDE.md` to reflect the new state location and import pattern.

**Checks.**

- `./scripts/check.sh --fast` after each batch of import-rewrites.
- `./scripts/check.sh` before commit.
- Run all of Search's existing Vitest tests (`pnpm vitest run --testPathPattern='lib/search'`) and the new
  query-ui tests.
- Run `pnpm vitest run --testPathPattern='enter-action|filter-chip-state|filter-popover-helpers|snapshot-label|searchable-folder|recent-chips-layout'`
  to confirm relocated pure-helper tests still pass.

**Definition of done.**

- Search's behavior unchanged (verified by full Vitest run and a manual smoke test of the search dialog through MCP).
- All ~100 call sites use the factory instance.
- New tests for the factory shape pass.

**Risk.** This is the riskiest milestone. ~100 call sites is a lot of mechanical change; one missed rename produces a
runtime error. Mitigation: TypeScript's compiler catches all of them; after the rename, `tsc --noEmit` must be clean
before the commit. Search's own test suite is the safety net for behavior.

### M3: Rename and extract the shared components into `lib/query-ui/`

**Why.** With state factored, we can move the presentational components into their new home without orchestration
changes.

**What.**

- Rename `SearchBar.svelte` → `QueryBar.svelte` in `lib/query-ui/`. Same props, same behavior.
- Rename `SearchModeChips.svelte` → `QueryModeToggleGroup.svelte` in `lib/query-ui/`. Reimplement on top of
  `lib/ui/ToggleGroup.svelte` (uses the new badge and hint slots for `AI` badge + `⌥A`/`⌥F`/`⌥R` hints). Same
  external props; "Content" stays visible-disabled with tooltip.
- Rename `AiTransparencyStrip.svelte` → `AiPromptStrip.svelte` in `lib/query-ui/`. Verbatim move.
- Rename `SearchFilterChips.svelte` → `FilterChips.svelte` in `lib/query-ui/`. New visibility props:
  `scopeChipVisible: boolean`, `patternChipVisible: boolean`. Defaults match Search's current behavior. Selection will
  pass `scopeChipVisible: false`. The "Pattern chip is search-specific" note in the existing CLAUDE.md is wrong
  now (clarification 5): Pattern is used by both consumers in AI mode. The chip stays.
- Rename `SearchResults.svelte` → `QueryResults.svelte` in `lib/query-ui/`. Takes the results from the parent (was
  already prop-driven). The "path column" stays renderable but its content is empty for Selection-against-current-
  folder entries (since `parentPath` is irrelevant when everything is in one folder). Visibility of the path column is
  driven by a new prop `showPathColumn` (default `true` for Search; Selection passes `false`).
- Rename `PathPills.svelte`, `SearchRowMenu.svelte`, `EmptyState.svelte`, `FilterChip.svelte`, `FilterChipPopover.svelte`
  into `lib/query-ui/`. Verbatim moves. (Yes, `SearchRowMenu` keeps the name; it's still a row-menu component, and
  renaming everything labeled "search" hurts grep more than helps.)
- Rename `RecentSearchesFooter.svelte` → `RecentItemsFooter.svelte`, `RecentSearchesPopover.svelte` →
  `RecentItemsPopover.svelte`, in `lib/query-ui/recent-items/`. Wire props for an entry adapter
  `(entry: HistoryEntry) → { label: string; tooltip: string; mode: SearchMode; age: string }` so Search and Selection
  can drive copy and tooltip text from their own history shapes.
- Convert `recent-searches-state.svelte.ts` into a factory `recent-items-state.svelte.ts` that takes the IPC funcs
  (`{ getRecent, addRecent, removeRecent, clearRecent, applyMaxCount }`) and returns the same reactive store shape.
  `lib/search/recent-searches-state.svelte.ts` becomes a thin file that constructs the factory with the
  `getRecentSearches`-family IPCs.
- All renames update every import site in the repo. `cmdr/no-raw-tauri-invoke` and import-cycle checks must stay
  green.
- Tier-3 a11y tests for each renamed component: `QueryBar.a11y.test.ts` etc.

**Docs.**

- `lib/query-ui/CLAUDE.md` grows the table of files we own.
- `lib/search/CLAUDE.md` shrinks accordingly; the load-bearing decisions and gotchas about the bar, chips, strip, and
  results list move into `lib/query-ui/CLAUDE.md`.
- `docs/architecture.md`: update the `lib/search/` row and add a `lib/query-ui/` row.

**Checks.**

- `./scripts/check.sh --fast` then `./scripts/check.sh` before commit.
- `pnpm vitest run` should be green on the full frontend test suite.

**Definition of done.**

- Every `import … from '$lib/search/*'` that points at a relocated file is updated.
- The dialog renders and behaves identically in the running app (verify via MCP: open the Search dialog, run a query,
  check chips, popover, AI strip, recent searches footer).
- Tests pass.

**Risk.** Medium. Many import sites. TypeScript catches most; manual smoke test via MCP catches the rest.

### M4: Convert `SearchDialog.svelte` into `QueryDialog.svelte`

**Why.** The orchestrator becomes the shared primitive. Search becomes the first consumer with its full current
behavior expressed as a config.

**What.**

- Create `lib/query-ui/QueryDialog.svelte` carrying today's `SearchDialog.svelte`'s orchestration (overlay, mount,
  unmount, keyboard dispatch, `lastDialogEvent` lifecycle, IME guard, auto-apply, `deriveEnterAction`, `runOnMount`).
- Define `QueryDialogConfig`:

  ```ts
  interface QueryDialogConfig {
    title: string
    maxWidth: string                                          // e.g. 'min(1080px, 80vw)'
    state: QueryFilterState                                   // the factory instance
    aiEnabled: boolean
    visibleChips: { size: boolean; date: boolean; scope: boolean; pattern: boolean }
    showPathColumn: boolean
    historyStore: RecentItemsStore                            // from the factory
    emptyState: {
      examples: Array<{ kind: 'ai' | 'pattern' | 'regex'; label: string }>
      indexHint?: string                                      // only Search uses this
      keyboardHint?: string
    }
    runQuery: (q: BuiltQuery) => Promise<{ entries: ResultEntry[]; totalCount: number }>
    translateAi?: (prompt: string) => Promise<AiTranslateResult>
    aiContext?: () => string[]                                // Selection passes folder sample
    primaryAction: {
      label: string
      shortcutHint: string                                     // e.g. '⌥⏎' or '⏎'
      handler: (entries: ResultEntry[]) => void | Promise<void>
    }
    secondaryAction?: { label: string; shortcutHint: string; handler: (entry: ResultEntry) => void }
    onMount?: () => void | Promise<void>                     // Search: prepareSearchIndex
    onDestroy?: () => void                                    // Search: releaseSearchIndex
    /* Plus accessibility / aria labels per consumer */
  }
  ```

- Add a title bar to the top of the dialog (the new chrome from Design summary § Title bar). Both consumers render it.
- Move the route-level wiring in `routes/(main)/+page.svelte` to consume `QueryDialog` for Search via the
  `SearchDialog.svelte` thin wrapper that builds the Search-specific config.
- Search's own orchestration concerns (index prepare/release, snapshot promotion, MCP listener, `runOnMount` for MCP
  prefill) stay in `SearchDialog.svelte`; they hook into `QueryDialog` via the config's `onMount` / `onDestroy` /
  `runOnMount` / `primaryAction` / `secondaryAction`.
- `routes/(main)/mcp-listeners.ts`: `mcp-open-search-dialog` event still triggers Search via the existing flag; no
  change at the IPC level. The Search wrapper just calls `applySearchPrefill` on the Search state instance.
- All existing search a11y tests, dialog tests, and Playwright e2e specs must stay green.

**Docs.**

- `lib/query-ui/CLAUDE.md` documents the `QueryDialogConfig` shape and the responsibility split (what lives in the
  shared component vs the consumer wrapper).
- Move the load-bearing decisions about dialog patterns (command-palette overlay, two-cursor hover, auto-apply gates,
  Enter ownership swap, IME guard, snapshot timing) into the new file. `lib/search/CLAUDE.md` keeps Search-specific
  decisions only (snapshot store, virtual volume, MCP open path).

**Checks.**

- `./scripts/check.sh --fast` during, `./scripts/check.sh` before commit.
- Run the Search-related Playwright suite: `pnpm playwright test apps/desktop/test/e2e-playwright/search.spec.ts` (or
  whichever the existing search specs are; the executing agent confirms by listing the test files).
- Manual smoke via MCP: open Search, AI mode, Filename mode, Regex mode, filters, recent searches, Open in pane.

**Definition of done.**

- Search behaves identically end-to-end (manual MCP smoke + automated tests).
- `QueryDialog` exists, is documented, and Search is a consumer of it.
- `SearchDialog.svelte` is ~150 lines (down from 1377), almost all config-building and Search-specific glue (index
  lifecycle, snapshot promotion, MCP).

**Risk.** Highest. The orchestrator carries most of the behavior. Mitigation: the existing Search test suite runs
verbatim; if any of those fail, we know what broke. Add a focused integration test for the config-driven paths
(`secondaryAction` callback fires on Enter when `deriveEnterAction === 'go-to-file'`; `primaryAction` callback fires
on ⌥⏎; etc.) so regressions in either consumer surface quickly.

### M5: Selection backend (Rust): history store, AI translation IPC

**Why.** Frontend Selection needs working IPCs to write recents and to call AI translation. Building them first lets
M6 land cleanly without backend churn.

**What.**

- Create `src-tauri/src/selection/` with `mod.rs`, `history.rs`, `ai/{mod,prompt,parser,query_builder}.rs`.
- `selection/history.rs` mirrors `search/history.rs`: same atomic write, schema-versioned, in-memory mutex,
  DISK_LOCK, canonical-key dedupe (over the narrower selection fields), schema-version quarantine. File:
  `{app_data_dir}/selection-history.json`.
- New IPC commands in `src-tauri/src/commands/selection.rs`:
  - `get_recent_selections(limit)`, `add_recent_selection(entry, max_count)`,
    `remove_recent_selection(id)`, `clear_recent_selections()`, `apply_recent_selections_max_count(max_count)`.
  - `translate_selection_query(prompt: String, sample_names: Vec<String>) -> Result<SelectionTranslateResult, String>`.
- `selection/ai/prompt.rs` defines the classification prompt. Same key-value response style as
  `search/ai/prompt.rs` (no JSON). The implementing agent writes the prompt and runs at least 6 manual evaluations
  via the OpenAI API (David's $2.5k credits) on representative folder samples to confirm output quality. Pin the
  prompt with a docstring describing what the evaluations covered.
- `selection/ai/parser.rs` parses key-value response into `ParsedSelectionLlmResponse`. Reuses
  `search/ai/parser.rs::parse_key_value_line` if exported, or re-implements lightly.
- `selection/ai/query_builder.rs` assembles `SelectionTranslateResult { pattern, kind, size_filter?, date_filter?, caveat?, label? }`.
- Add `selection.recentSelections.maxCount` to the settings registry (default 1000). Wire `settings-applier.ts` and
  the matching Rust live-apply hook.
- Register the new commands in `ipc.rs` and `ipc_collectors.rs` for specta.
- Run `pnpm bindings:regen`. Commit the regenerated `bindings.ts` separately or in the same commit (per repo style).

**Tests.**

- Unit: `selection/history.rs` mirror-tests for the search history tests (atomic write, schema migration, dedupe,
  cap eviction, schema-version quarantine).
- Unit: `selection/ai/parser.rs` round-trip tests for representative model responses (`pattern: *.png\nkind: glob\nsize_min: 1048576`).
- Unit: `selection/ai/query_builder.rs` tests covering the three filter combinations (pattern only, pattern + size,
  pattern + date).
- Integration: an offline test that fakes the LLM IPC and verifies `translate_selection_query` end-to-end.
- Real-LLM eval: a `tests/selection_ai_eval.rs` integration test that uses the OpenAI API with David's credentials.
  Behind `#[cfg(feature = "ai-eval")]` so CI doesn't run it. Run manually with `cargo test --features ai-eval -- selection_ai_eval`
  during this milestone to validate the prompt design on the real model.

**Docs.**

- `src-tauri/src/selection/CLAUDE.md` documenting the module shape, prompt, parser, history store, IPC.
- `src-tauri/src/search/CLAUDE.md`: mention that the AI parser helpers are now used by both subsystems.
- `docs/architecture.md`: add a `selection/` row.

**Checks.**

- `./scripts/check.sh --check go-vet --check clippy --check rust-tests --check bindings-fresh` after the Rust
  changes.
- Full `./scripts/check.sh` before commit.

**Definition of done.**

- All new Rust unit tests pass.
- `bindings-fresh` is green.
- Manual API call against `gpt-5.5` returns a parseable response for a sample prompt and a sample folder of ~50 names.

**Risk.** Low for history (it's a mirror of an existing module). Medium for AI prompt (may need iteration). Mitigation:
the real-LLM eval test in this milestone catches prompt drift before the dialog wraps around it.

### M6: Selection backend (Rust): pane-side `applyIndices` plumbing

**Why.** The dialog applies its result by calling a method on the focused pane. We need that method to exist in
`FilePane.selection` first so M7 can wire it.

**What.**

- In `lib/file-explorer/pane/selection-state.svelte.ts`, add
  `applyIndices(idxs: number[], mode: 'add' | 'remove', hasParent: boolean): void`. Skip index 0 if `hasParent`,
  same rule as `selectAll`. Fires `onChanged?.()`.
- In `lib/file-explorer/pane/FilePane.svelte`, expose
  `applyIndices(idxs: number[], mode: 'add' | 'remove')` calling into selection state. Skip `..` per hasParent.
- In `DualPaneExplorer.svelte`, expose `applyIndicesToFocusedPane(idxs, mode)` that resolves the focused pane and
  forwards.
- For `search-results://` panes: the dialog's match runs against `entry.path`, but `applyIndices` still operates on
  indices into the snapshot's `entries[]`. The dialog has to map matched results back to snapshot indices for
  search-results panes. Document this in `lib/selection-dialog/CLAUDE.md` (M7 milestone).

**Tests.**

- Unit: extend `selection-state` tests to cover `applyIndices` (add, remove, hasParent skip, idempotence).
- Unit: extend FilePane keyboard tests to confirm `applyIndices` plumbs through (already covered by selection-state
  unit; no need to add an integration test here).

**Docs.**

- Update `lib/file-explorer/CLAUDE.md` § Selection to add the new API.

**Checks.**

- `./scripts/check.sh --fast` then full.

**Definition of done.**

- API exists, is unit-tested, and integration-smoke works via MCP (open Cmdr, focus a pane, call
  `explorerRef.applyIndices` from the dev console to confirm it toggles the right rows).

**Risk.** Low.

### M7: Build `SelectionDialog.svelte` and matching helpers

**Why.** With the shared `QueryDialog`, the recent-items factory, the IPC backend, and the pane API in place, this
milestone is the actual feature. Mostly assembly.

**What.**

- `lib/selection-dialog/folder-sampler.ts` (pure): `sampleFolderNames(names: string[], cursorIndex: number, max: number)`.
- `lib/selection-dialog/selection-matching.ts` (pure): `matchEntries(getNameFor, total, query) → number[]` where
  `query = { pattern, kind: 'glob' | 'regex', caseSensitive, size?, date? }`. Compiles the glob to a `RegExp` (same
  rules as Search's filename glob) or uses the user's regex. Iterates indices, returns matches.
- `lib/selection-dialog/selection-history-state.svelte.ts`: constructs a recent-items factory store with the
  selection-IPCs. Same shape as `lib/search/recent-searches-state.svelte.ts`.
- `lib/selection-dialog/SelectionFooterAction.svelte`: a single button "Select these files ⏎" / "Deselect these
  files ⏎" depending on mode. Reuses the existing footer-button CSS conventions.
- `lib/selection-dialog/SelectionDialog.svelte`: the thin wrapper. Reads the focused pane, builds the entry iterator,
  builds the matcher, builds the `QueryDialogConfig`, mounts `QueryDialog`. Calls
  `explorerRef.applyIndices(matchedIndices, mode)` on commit.
- Add `selection.selectFiles` and `selection.deselectFiles` to `command-registry.ts` with scope
  `'Main window/Selection dialog'` (or similar; align with the existing naming) and shortcuts `=` and `-`.
- Add the dispatch cases to `handleCommandExecute` in `routes/(main)/command-dispatch.ts`. The handler flips a route
  state flag `showSelectionDialog: 'add' | 'remove' | null` similar to how `showSearchDialog` works.
- `routes/(main)/+page.svelte`: mount `SelectionDialog` when the flag is set; pass the focused pane handle.
- `lib/shortcuts/shortcuts-store.ts`: register the new commands in `menuCommands` (if menu-bound, which they will be
  per M8).
- `lib/file-explorer/pane/FilePane.svelte`: in the keydown handler, if `event.key === '+' || event.key === '-'` AND
  no modifier AND focus is on the pane (not an input), preventDefault and dispatch the matching command. This is a
  unique key-binding because `event.key === '+'` covers Shift+= for QWERTY and most other Latin layouts. Bare `-`
  binds the same way.

**Tests.**

- Unit: `folder-sampler.test.ts` covering the cases (small folder, large folder, cursor near start / middle / end,
  empty folder, dedup).
- Unit: `selection-matching.test.ts` covering glob / regex / case sensitivity / size + date predicates / empty
  pattern / search-results pane (path-based match).
- Component: `SelectionDialog.svelte.test.ts` covering: mounts with the right title per mode; pressing ⏎ applies and
  closes; switching modes preserves the query; Cmd+N clears state; recent selections appear in the footer.
- A11y: `SelectionDialog.a11y.test.ts` mirroring `SearchDialog.a11y.test.ts` (tier 3).
- E2E: a single Playwright spec `selection-dialog.spec.ts` covering the happy path: focus a pane, press `=`, type
  `*.txt`, press Enter, confirm three rows became selected. Must run in <1 s per `AGENTS.md`'s testing rules. Use
  `dispatchMenuCommand` for the dialog open since the test is about the dialog, not the keyboard pathway.

**Docs.**

- `lib/selection-dialog/CLAUDE.md` documenting the dialog's wiring, the matcher, the sampler, the
  search-results-pane path-matching note, and the link to the shared `QueryDialog`.
- Update `lib/file-explorer/CLAUDE.md` § Selection to add a "Select / deselect files dialog" subsection pointing at
  `lib/selection-dialog/CLAUDE.md`.

**Checks.**

- `./scripts/check.sh --fast` during, `./scripts/check.sh` before commit.
- Manual smoke via MCP: open a real folder, hit `=`, try AI mode with a fake key first, then with the real OpenAI
  key (configure via Settings > AI > Cloud, see Risk register R3 for how to configure mid-session).

**Definition of done.**

- All new tests pass.
- E2E spec runs in <1 s.
- Manual MCP smoke confirms the dialog opens, runs queries, applies selection, AI translation works with the real
  model, recent selections appear after commit.

**Risk.** Low-medium. The risk is integration drift: a missing prop, a typo in a config field. Tests catch most;
manual smoke catches the rest.

### M8: New "Select" top-level macOS menu

**Why.** David asked for it. Anchors the new feature in the system menu bar so users discover it without reading
docs.

**What.**

- Create the new `Select` submenu between `Edit` and `View` in `src-tauri/src/menu/menu_structure.rs` (and the macOS
  builder; check the existing layout). Add the four items: Select all (⌘A), Deselect all (⌘⇧A), Select files… (`=`),
  Deselect files… (`-`).
- Remove the same Select all / Deselect all items from the Edit menu (where they live today).
- Update `menu_id_to_command` / `command_id_to_menu_id` in `menu.rs` for the four items.
- Update `shortcuts-store.ts::menuCommands` accordingly.
- Verify macOS shows the menu correctly via the running app + MCP.

**Tests.**

- A focused Vitest test pinning `menuCommands` includes the new four IDs.
- E2E: `dispatchMenuCommand(tauriPage, 'selection.selectFiles')` opens the dialog (already covered by M7's spec).

**Docs.**

- Update `src-tauri/src/menu/CLAUDE.md` with the new top-level entry.
- Update `lib/commands/CLAUDE.md` § Adding a command if the example walkthrough needs refreshing.

**Checks.**

- `./scripts/check.sh --fast` then full.

**Definition of done.**

- Menu renders correctly on macOS (verified by screenshot via MCP).
- Shortcuts work: `⌘A` from a focused pane selects all; `=` opens Select files dialog; `-` opens Deselect files
  dialog. (Bare `=` and `-` bind via the FilePane keydown handler from M7, not via menu accelerators, because macOS
  menu accelerators always carry the ⌘ modifier.)
- Linux fallback: the menu structure also works on Linux (Cmdr ships there). The implementing agent verifies via
  the e2e-linux suite.

**Risk.** Low.

### M9: Settings panel for `selection.recentSelections.maxCount`

**Why.** The setting exists in the registry from M5. It needs a UI surface so users can tune it. Mirror Search's
"Recent searches" row.

**What.**

- Add a row to `lib/settings/sections/SearchSection.svelte` (or a new SelectionSection if the section structure
  changes) for the new setting. Mirror the existing search.recentSearches.maxCount row.
- Live-apply via `settings-applier.ts` calling `apply_recent_selections_max_count`.

**Tests.**

- Add a Vitest test in the same suite as the existing search settings tests.

**Docs.**

- Update `lib/settings/CLAUDE.md` if section structure changes.

**Checks.**

- `./scripts/check.sh --fast` then full.

**Definition of done.**

- The setting appears in Settings UI. Changing it live-applies (verified by MCP).

**Risk.** None.

### M10: Polish and bug-hunt

**Why.** Two new entry points + one big refactor surface latent bugs. Set aside a milestone for the cleanup, not just
"as we go".

**What.**

- Run the full Vitest suite, the Playwright suite, and the e2e-linux suite.
- Run `cargo mutants --file src-tauri/src/selection/history.rs --file src-tauri/src/selection/ai/parser.rs` and
  triage survivors.
- Run `pnpm exec stryker run` on the new TS files (`selection-matching.ts`, `folder-sampler.ts`).
- Review `EmptyState` examples on Selection with a 10-second usability check via the running app.
- Verify the title bar visual on both Search and Selection in light and dark mode. Check `a11y-contrast`.
- Verify keyboard contract: ⌘N inside Selection resets; ⌘H opens the recent-selections popover and fuzzy filter
  works.
- Verify that the search-results-pane Selection path works (focus a snapshot pane, hit `=`, match against full
  paths, apply, confirm the right rows became selected).
- Verify that the AI mode in Selection uses `gpt-5.5` end-to-end and that the AI transparency strip renders the
  prompt and caveat.

**Tests.**

- Any tests added during polish stay in this milestone.

**Docs.**

- Sweep all CLAUDE.mds touched in M1–M9 for accuracy. Verify the `lib/search/CLAUDE.md` and
  `lib/query-ui/CLAUDE.md` split is clean (no duplicate documentation; cross-links work).
- Update `docs/architecture.md` final cross-check.

**Checks.**

- `./scripts/check.sh` full default suite.

**Definition of done.**

- All tests green. No flaky tests. Mutation-test survivors triaged. Manual smoke confirms feature works end-to-end.

**Risk.** Low.

### M11: Slow checks and final acceptance

**Why.** Per `AGENTS.md`: run `--include-slow` before declaring a milestone "done". This is the gate.

**What.**

- `./scripts/check.sh --include-slow`. Allow ~20 minutes.
- If anything fails, fix and re-run.
- Final manual smoke via MCP: open Cmdr, exercise both features end-to-end including AI mode with the real model.
- Verify the `bindings-fresh` check is green.

**Tests.**

- The full slow lane: `desktop-e2e-linux`, `desktop-e2e-playwright`, `rust-tests-linux`, `eslint-typecheck`.

**Docs.**

- None at this stage.

**Checks.**

- `./scripts/check.sh --include-slow`.

**Definition of done.**

- Full slow-lane suite is green. Branch is ready for FF-merge to main.

**Risk.** Low; the milestones above already ran the default suite.

## Risk register

- **R1: M2 factory refactor breaks Search.** ~100 import sites; one missed rewrite is a runtime error. Mitigation:
  TypeScript catches all missed imports; full Vitest run + manual MCP smoke before the commit.
- **R2: M4 orchestration extraction subtly changes Search behavior.** Risky because the orchestrator has the most
  state. Mitigation: Search's existing test suite (tier-3 a11y + dialog tests + Playwright e2e). Add new integration
  tests for the `primaryAction` / `secondaryAction` callback paths so the config-driven contract is pinned.
- **R3: OpenAI API key handling for testing.** David has the key in Keychain. We need to configure Cmdr in dev mode to
  use OpenAI as the cloud AI provider. The implementing agent does this once via the Settings UI through MCP: open
  Settings, select AI → Cloud → OpenAI, paste the key from `security find-generic-password -s OPENAI_API_KEY -a veszelovszki -w`
  output, set the model to `gpt-5.5`. No keys in any committed file.
- **R4: Selection's `QueryResults` view has an empty path column for current-folder entries.** It's fine but visually
  it's a wasted column. Mitigation: a `showPathColumn: boolean` prop on `QueryResults`; Selection passes `false`.
  Search keeps it `true`. (Already in the design.)
- **R5: `event.key === '+'` and `event.key === '-'` clashes with text fields.** If focus is on a text input (rename
  editor, search bar inside the pane, settings), the keystroke should NOT open the dialog. Mitigation: the dispatch
  guard in FilePane checks that focus is on the pane (`event.target === paneRoot` or the cursor row), not on any
  input.
- **R6: AI prompt for Selection produces invalid glob/regex.** The model might emit a malformed regex.
  Mitigation: the parser validates the response; on parse failure, surface a caveat in the AI strip ("Couldn't
  translate; try again or use Filename mode") and don't apply a broken pattern. The frontend matcher already handles
  the empty-pattern case gracefully (returns `[]`).
- **R7: Selection in `search-results://` panes matches against full paths.** Users might expect basename matching.
  Mitigation: the dialog shows a small hint near the bar when the focused pane is a snapshot: "Matching full paths in
  search results". Place in `QueryDialog` as a conditional banner driven by the consumer's config.
- **R8: `selection-history.json` corruption on crash.** Mirrors `search-history.json`'s atomic-write story. Same
  schema-version quarantine on parse failure.
- **R9: The new `Select` menu accelerators `=` and `-` show up in the macOS menu bar with the wrong glyphs.** On
  macOS, menu accelerators always carry the ⌘ key as a modifier. Bare `=` and `-` aren't valid menu accelerators.
  Mitigation: register the menu items WITHOUT accelerators; the shortcut binding lives in the FilePane keydown
  handler. The menu shows the items as "Select files…" with no accelerator badge. This is the same approach as
  Space (for Quick Look) and Insert (for toggleAndDown), neither of which carry a menu accelerator.

## Feedback loops and tooling

Per David's instructions:

- **Don't fly blind.** Run the app via `pnpm dev` between milestones. Use MCP CLI calls (the agents will need to
  curl the running MCP server at `localhost:19225` since auto-connect doesn't work). Inspect the dev console logs via
  the `tauri MCP`'s `read_logs` tool. Take screenshots between milestones to confirm the title bar, the mode chips
  via the new ToggleGroup, and the empty state for both consumers.
- **TDD wherever reasonable.** For pure helpers (`folder-sampler`, `selection-matching`, the matcher's edge cases),
  write tests first. For UI components, write the a11y test alongside the component (tier 3).
- **`./scripts/check.sh` after each milestone.** No exceptions. `--include-slow` only at M11.
- **E2E specs run in <1 s each.** The single Selection spec in M7 must hit that bar. Use `dispatchMenuCommand`,
  `waitForSelector`, and `pollUntil`; never `sleep()`.

## Parallelism notes

Almost everything here is sequential. The factory refactor (M2) gates M3–M10. The Rust backend (M5) gates the
frontend dialog (M7). M6 (pane API) is independent of M5 (backend IPC) and could run in parallel, but the agents
work sequentially per the execute workflow; this isn't a critical-path savings worth chasing.

Two safe-to-parallelize pieces inside individual milestones (single-file edits, no shared state):

- M5: the `history.rs` mirror and the `ai/` module are independent; one agent can build both in parallel
  bash commands, but it's not worth a worktree.
- M7: the pure helpers (`folder-sampler.ts`, `selection-matching.ts`) and the wrapper component (`SelectionDialog.svelte`)
  can be built side by side; tests for the helpers don't depend on the component existing.

These are notes; the executing agents work sequentially by default. Parallelize only when you can run two commands
that genuinely don't touch each other's files.

## Open questions deferred to execution

These don't block the plan; the executing agents resolve them as they go:

- The exact wording of the AI prompt in `selection/ai/prompt.rs`. Seed in M5; refine via the eval test.
- Final copy on the EmptyState examples. Seed in the design summary; refine in M10 with the real running app.
- Whether the title bar in Search should say "Search" or "Search files…" for parity with Selection's "Select
  files…". I (the planner) lean "Search" since it's a verb and reads cleanly; the executing agent confirms when
  building M4.
- Whether the Pattern chip should show in AI mode in Selection. (Decision: yes, same as Search. See Decision log.)
- Whether to keep the "Press Enter to search" hint string as-is in Selection or change it to "Press Enter to
  filter". Decision: change to "Press Enter to filter" only when the dialog is in Selection mode; pass via the
  consumer's config (`runHintCopy: string`). M7.

## Definition of done (whole plan)

- [ ] All 11 milestones committed to `worktree-selection-dialog` and ready to FF-merge.
- [ ] `./scripts/check.sh --include-slow` is green.
- [ ] `bindings-fresh` is green.
- [ ] Manual smoke via MCP: both Search and Selection work end-to-end, including AI mode with `gpt-5.5`, in both
      light and dark mode.
- [ ] No CLAUDE.md is stale.
- [ ] `docs/architecture.md` reflects the new `lib/query-ui/` directory and the new `selection/` Rust module.
- [ ] The `Select` menu shows correctly on macOS.
