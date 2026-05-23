# Selection dialog and the QueryDialog unification

Spec for adding "Select files…" / "Deselect files…" to Cmdr, by unifying it with the existing whole-drive Search dialog
under a shared `QueryDialog` primitive.

## Plan history

- **Round 1 review** (Opus, fresh-eyes): 8 blockers, 17 important gaps, 12 nice-to-haves. Verdict: REVISE.
- **Round 1 revisions** (in this file): all blockers and important gaps addressed. Key corrections: ToggleGroup
  primitive now carries a `semantics: 'tabs' | 'toggles'` prop so Query gets tablist a11y and Settings keeps
  toggle-group a11y while sharing one component (B1, B2). Snapshot panes match against `entry.name` (which is already
  the displayed friendly path), not `entry.path` (B3). Model name is "the configured cloud model"; no `gpt-5.5`
  hard-codes (B4). Keystroke binding is Total Commander parity: `event.key === '+'` and `event.key === '-'` (B5). M4
  drops the line-count target; the wrapper is sized by responsibility, not lines (B6). M2 splits the state factory into
  a cross-consumer core plus a Search-specific extras module (B7, G6). Selection's mode set is locked: AI / Filename /
  Regex (B8). Capability files, CommandScope value, platform menu files, AI provider gating, and the CLAUDE.md split
  sheet are now explicit (G1, G3, G4, G7, G10). `QueryModeToggleGroup` renamed to `ModeChips` (N1).

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
second. The same query bar, same mode chips, same filter chips, same recent items, same keyboard shortcuts, same "Press
Enter" hint, all share the same code by construction.

This is the elegance-over-duplication call from `AGENTS.md` § Principles: invest in finding the right tradeoff.

## Goals

- Add commands `selection.selectFiles` (shortcut `=`, no modifier) and `selection.deselectFiles` (`-`, no modifier).
  Both open the same dialog with a `mode` prop that flips the title, the verb on the apply button, and which way matched
  indices flow into the focused pane's selection set.
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
- Selection works in `search-results://` snapshot panes too: matching runs against the full path (which is what the user
  sees in those panes) instead of the basename.

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

| Aspect                                 | Search                                     | Selection                                                  |
| -------------------------------------- | ------------------------------------------ | ---------------------------------------------------------- |
| Title                                  | "Search"                                   | "Select files" / "Deselect files"                          |
| Max width                              | `min(1080px, 80vw)`                        | `min(720px, 60vw)`                                         |
| Data source                            | `searchFiles` IPC against the drive index  | JS matcher over `pane.getEntries()` (sync)                 |
| AI translate IPC                       | `translate_search_query(prompt)`           | `translate_selection_query(prompt, sampledNames[])`        |
| AI context                             | none                                       | sampled filenames from the focused folder                  |
| Primary action                         | "Show all in main window ⌥⏎"               | "Select these files ⏎" / "Deselect these files ⏎"          |
| Secondary action                       | "Go to file ⏎"                             | none                                                       |
| Empty-state hint about index size      | "Index ready: 5,000,000 files"             | (omitted)                                                  |
| Empty-state example chips              | "PDFs from this week", "\*.svg", "/log\d/" | "all image files", "logs newer than a week", "files >5 MB" |
| Scope filter chip ("Search in")        | yes                                        | hidden                                                     |
| Pattern filter chip                    | yes                                        | yes (same role: shows AI-translated pattern in AI mode)    |
| History store                          | `search-history.json`                      | `selection-history.json`                                   |
| Index prepare / release lifecycle      | yes                                        | no                                                         |
| MCP listener                           | yes                                        | no                                                         |
| `search-results://` snapshot promotion | yes                                        | no                                                         |
| Title bar                              | new, "Search"                              | new, "Select files" / "Deselect files"                     |

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
   contract. Duplicating that for Selection means duplicating drift surface. The two dialogs would diverge over time no
   matter the discipline.
2. Every UX improvement we ship to Search should land in Selection for free, and vice versa. Shared primitives don't
   guarantee that; shared orchestration does.
3. The set of things that genuinely differs (data source, AI context, apply verb, history file, title, width) is small
   and easy to express as injected behavior. The leaky-abstraction risk is low.

A third consumer would slot in cleanly: for example, a future "Filter visible" dialog that applies a temporary filter to
the focused pane's list. Two consumers don't justify the abstraction on their own; this third future consumer is the
sanity check that we're not over-engineering.

### Naming

- `lib/query-ui/` is the home of the shared primitives.
- `lib/query-ui/QueryDialog.svelte` is the orchestrator.
- `lib/query-ui/QueryBar.svelte` (renamed from `SearchBar`).
- `lib/query-ui/ModeChips.svelte` (renamed from `SearchModeChips`; built on top of `lib/ui/ToggleGroup.svelte` with
  `semantics="tabs"`).
- `lib/query-ui/AiPromptStrip.svelte` (renamed from `AiTransparencyStrip`).
- `lib/query-ui/FilterChips.svelte` (renamed from `SearchFilterChips`; scope chip and Pattern chip become props).
- `lib/query-ui/recent-items/` for the recent-items footer, popover, and factory store.
- `lib/query-ui/query-filter-state.svelte.ts` (factory replacing the module-singleton `search-state.svelte.ts`; ONLY
  cross-consumer fields, see M2 for the split).
- `lib/search/search-extras-state.svelte.ts` (Search-only fields: `scope`, `excludeSystemDirs`, `lastAiLabel`,
  `lastAiPattern`, `lastAiPatternKind`; composed alongside the core factory instance, not merged into it).
- `lib/search/` keeps Search-specific code: `SearchDialog.svelte` wrapper (thin) that wires Search's config; snapshot
  store; `search-results://` machinery; MCP listener wiring; index lifecycle.
- `lib/selection-dialog/` is the new feature: `SelectionDialog.svelte` (thin wrapper), `selection-matching.ts` (pure
  matcher), `folder-sampler.ts` (pure sampler), `selection-history-state.svelte.ts` (consumer of the recent-items
  factory).

Why "QueryDialog" and not "SearchDialog" for the shared name: calling Selection a "search" misleads readers. "Query"
covers filter-and-act-on across both consumers and any future ones. The component is responsible for showing the query,
not for what the query searches over.

### Match semantics

The matcher is a pure function
`match(name: string, pattern: string, opts: { mode: 'glob' | 'regex'; caseSensitive: boolean }): boolean`.

For Selection, it takes a `getNameFor(index: number) → string` accessor passed by the dialog. In both pane kinds, the
accessor returns the string the user sees in the list:

- **Regular pane**: returns `entry.name` (the basename, what the file is called).
- **`search-results://` pane**: returns `entry.name` too. Per `lib/file-explorer/CLAUDE.md` § "Search-results virtual
  volume" and search-fixup item 15: in `SearchResultsView`, the adapted entry's `name` IS the friendly full path (home
  folder shown as `~`, mid-truncated for display). That's what `findItemIndex` matches on, and that's what type-to-jump
  operates on. Selection follows the same rule: what the user sees is what the matcher matches.

Same matcher, single accessor rule across pane kinds. No special-casing in the matcher itself; the dialog passes the
right accessor. A unit test in `selection-matching.test.ts` pins the snapshot-pane accessor returning the friendly name
(with `~`), not the raw `entry.path`.

Glob mode reuses the same translation as Search's filename mode: `*` and `?` only, anchored. Regex mode uses
JavaScript's `RegExp` directly (Search currently delegates to Rust for regex; Selection doesn't need to round-trip
through IPC because the matcher runs in JS).

Size and date filters from the chip strip apply as additional predicates. Selection's matcher composes all three:
pattern + size + modified.

### Apply action

The dialog returns a list of matched indices to its parent. For Search, this is the existing snapshot-promotion path
(unchanged). For Selection, the parent (`+page.svelte`) calls
`explorerRef.applyIndicesToFocusedPaneSelection(indices, mode)` where `mode: 'add' | 'remove'`. Implementation in
`FilePane.selection`:

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
`SelectionTranslateResult { pattern, kind: 'glob' | 'regex', size_filter, date_filter, caveat?, label? }`. Same shape as
Search's `TranslateResult` minus `scope`, `exclude_system_dirs`, and `folders`.

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

**Decision**: re-export `HistoryMode` and `HistoryFilters` from `search/history.rs` rather than copying. Reusing the two
pure data types is fine. The entry struct itself is separate so the schema doesn't bind selection to search's
canonical-key shape.

File: `{app_data_dir}/selection-history.json`. Schema v1. Cap setting: `selection.recentSelections.maxCount` (default
1000), live-applied via the same applier pattern.

IPC: `get_recent_selections`, `add_recent_selection`, `remove_recent_selection`, `clear_recent_selections`,
`apply_recent_selections_max_count`. specta regenerates the bindings.

Frontend mirrors: `selection-history-state.svelte.ts` uses the recent-items factory store (new) which reads from
whichever set of IPC functions the consumer passes in. The factory makes both Search and Selection consume their
respective IPCs through the same store shape.

### Title bar

A new minimal title bar at the top of the dialog: 32 px tall, `--font-size-md` weight 500, centered. Only the title
text. **No close button** (Escape closes the dialog; that's the only close path). The title bar is **not in the Tab
order**: text-only header, no interactive elements. Both Search and Selection render it. Spec:

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

### Mode chips: shared visual primitive, two ARIA shapes

Today's `SearchModeChips.svelte` is a bespoke `role="tablist"` (246 lines, hand-rolled arrow-key motion, AI badge,
⌥A/F/R hints, the visible-disabled Content chip with "Coming soon" tooltip). Today's `SettingToggleGroup.svelte` is an
Ark UI `ToggleGroup` (101 lines, single-value toggle). The two have genuinely different a11y semantics:

- Search mode chips drive a UI mode (active = "this is the current mode"). `role="tablist"` + `aria-selected` is
  correct; AT users hear "tab 1 of 4, AI mode, selected".
- Settings toggle groups pick a stored value (active = "this is the chosen setting value"). Ark's `ToggleGroup` with
  single-select is correct; AT users hear "toggle button, kB, pressed".

Forcing both onto Ark's `ToggleGroup` would push the Search mode chips into the wrong a11y model. Splitting into two
primitives would diverge visually. The right answer: one component with one visual contract, two ARIA shapes.

**`lib/ui/ToggleGroup.svelte`** (new):

```ts
interface Props {
  semantics: 'tabs' | 'toggles' // 'tabs' → role="tablist", 'toggles' → Ark ToggleGroup
  value: string
  options: Array<{
    value: string
    label: string
    badge?: string // "AI" pill before the label
    hint?: string // "⌥A" inline shortcut hint
    disabled?: boolean
    tooltip?: string // shown when disabled or always; opt-in
    ariaLabel?: string // overrides the computed accessible name
  }>
  onChange: (value: string) => void
  ariaLabel: string // tablist/toggle-group accessible name
  disabled?: boolean
}
```

Implementation:

- **`semantics: 'tabs'`**: renders as `<div role="tablist">` with each option as `<button role="tab" aria-selected>`.
  Arrow-key motion skips disabled options. Active option is `tabindex=0`; the rest are `tabindex=-1`. Matches today's
  `SearchModeChips` behavior verbatim.
- **`semantics: 'toggles'`**: wraps Ark UI's `ToggleGroup.Root` + `ToggleGroup.Item`. Single-select. Matches today's
  `SettingToggleGroup` behavior verbatim. (We import Ark and use it under the hood for this branch only.)
- Shared CSS: visual chrome (border, radius, hover, active background, focus ring) lives at the component level so both
  ARIA shapes render identically. Badge and hint slots render the same way in both modes.

Two consumer wrappers:

- **`lib/ui/ToggleGroup.svelte`** itself is generic. Both Settings and Query import it.
- **`lib/settings/components/SettingToggleGroup.svelte`** becomes a thin wrapper that reads the setting definition,
  builds the options, and renders `<ToggleGroup semantics="toggles" … />`. Public API unchanged.
- **`lib/query-ui/ModeChips.svelte`** is the Query mode chip row. Renders `<ToggleGroup semantics="tabs" … />`. Same
  external props as today's `SearchModeChips` (mode, aiEnabled, disabled, onSelect). The disabled Content chip (option
  entry with `disabled: true, tooltip: "Coming soon: full-text search inside files"`) carries over.

Tests cover both semantics modes (tier-3 axe-core audit) so the a11y contract is pinned in CI.

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
    ModeChips.svelte                ← was SearchModeChips.svelte
    ModeChips.svelte.test.ts
    ModeChips.a11y.test.ts
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

| Shortcut                   | Where        | Action                                  |
| -------------------------- | ------------ | --------------------------------------- |
| `+` (Shift+= on US QWERTY) | focused pane | Opens Selection dialog in `add` mode    |
| `-` (bare)                 | focused pane | Opens Selection dialog in `remove` mode |

Total Commander parity. Implementation: bind `event.key === '+'` for the add path, `event.key === '-'` for the remove
path. On US QWERTY, the user perceives the add shortcut as "Shift+=" (which fires `event.key === '+'`); on layouts where
`+` is unshifted, plain `+` fires the same event. Either way, the dialog opens.

Constraint: the dispatch guard MUST reject if `event.metaKey || event.altKey || event.ctrlKey` is true. The bare key
itself can carry the shift modifier (since `event.key === '+'` is produced by Shift+= on US), so the guard does NOT test
`event.shiftKey`. We test:

```ts
if (e.metaKey || e.altKey || e.ctrlKey) return false
if (e.key !== '+' && e.key !== '-') return false
if (focusIsInInputElement) return false
// dispatch
```

A unit test pins this exact event filter. M7 adds it.

`⏎` inside the Selection dialog dispatches via the same `deriveEnterAction` state machine. The two outcomes map to
`'run-query'` (run the matcher and refresh the preview) and `'apply-selection'` (apply to focused pane and close). Same
logic as Search; different verb at the end.

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

Selection's `EmptyState` uses the same three-example block as Search, with different example chips and no "Index ready"
line. AI examples pair with non-AI examples that match the same intent so the user sees the parity. Seed (the
implementing agent refines in M10):

```
Try…

[ ✨ all image files ]            [ ✨ logs newer than a week ]     [ ✨ files bigger than 5 MB ]
or
[ *.{jpg,png,gif} ]               [ *.log ]                          [ (use Size > 5 MB) ]

Press ↓ to navigate matches  •  ⌘N to start over
```

The third non-AI slot points at the size filter chip rather than offering a pattern, because size isn't a pattern. Match
the existing Search EmptyState's structure exactly; we're just swapping the examples and dropping the "Index ready"
line.

## Decision log

- **`QueryDialog` over `SearchDialog` as the shared name.** Selection isn't a search; it's a filter-and-act-on. "Query"
  covers both. Future consumers slot in without renaming.
- **`lib/query-ui/` over `lib/shared/`.** "Shared" is meaningless ("shared with what?"). `query-ui` names the concept.
- **One `QueryDialog` over two dialogs with shared subcomponents.** The orchestrator carries 90% of the polish; sharing
  only subcomponents wouldn't share the polish.
- **Factory `createQueryFilterState()` over module-singleton state.** Two consumers need two state instances. The
  refactor is mechanical (15 files, ~471 identifier usages per a fresh grep) but unblocks everything else.
- **Move `SearchModeChips` into a generic `ToggleGroup` even though Settings's version is narrower.** Both Settings and
  Query benefit from a single segmented-control primitive. The generic carries the union of features (badge, hint,
  per-option disabled with tooltip); Settings just doesn't use the extras.
- **Frontend-only matcher for Selection (no Rust IPC for matching).** Current-folder match against in-memory entries is
  microseconds in JS. A Rust IPC would add IPC overhead with no benefit.
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
- **Selection's AI receives a folder sample.** Without context, the model can't infer a pattern from "all rymdskottkärra
  files". Sampled filenames cost a few cents per call but make the AI mode actually work for the user's intent.
- **Drop the case-sensitivity toggle from v1.** Default off matches Double Commander's default; we'll add the toggle to
  the bar later when we add it to Search too. Avoids per-consumer divergence.
- **Add a title bar to the existing custom overlay rather than wrapping in `ModalDialog`.** The custom overlay's
  keyboard handling is exactly what this dialog needs; ModalDialog would fight it. 30 lines of title-bar CSS is cheaper
  than re-litigating focus management.
- **Bare `=` and `-` shortcuts, no modifier.** David confirmed `=` is unused and `-` is safe (⌘- is zoom out; bare `-`
  is free).
- **No "Open in pane" for Selection.** Selection's terminal action is "apply to the pane". There's no equivalent of
  Search's snapshot promotion. The footer's secondary slot stays empty.
- **Select all / Deselect all move to the new Select menu, not Edit.** macOS convention puts them in Edit. We move them
  because Cmdr's `selectAll` operates on files, not text; the Select menu is the more honest home. Edit retains
  Cut/Copy/Paste for text operations. Implementation: remove the items from Edit in both `menu/macos.rs` and
  `menu/linux.rs`; register them under the new Select submenu.
- **Selection's ToggleGroup uses tablist semantics (`semantics="tabs"`); Settings uses toggle-group semantics
  (`semantics="toggles"`).** Different a11y contracts. One visual primitive. See § "Mode chips" for the rationale.
- **State factory splits into core and Search-extras.** `lib/query-ui/query-filter-state.svelte.ts`
  (`createQueryFilterState`) owns ONLY cross-consumer fields: `query`, `mode`, `sizeFilter` + value/unit/max,
  `dateFilter` + value/max, `caseSensitive`, `lastAiPrompt`, `lastAiCaveat`, hand-typed buffers (per mode), `results`,
  `totalCount`, `cursorIndex`, `imeComposing`, `lastDialogEvent`, `runOnMount`. Search-only fields (`scope`,
  `excludeSystemDirs`, `lastAiLabel`, `lastAiPattern`, `lastAiPatternKind`) live in a separate
  `lib/search/search-extras-state.svelte.ts` module the Search wrapper instantiates next to its core instance. The
  search-only `buildSearchQuery()` helper also lives next to that module. Selection's wrapper uses the core factory
  only; no extras. (Resolves B7. Rationale: keeps the shared factory clean and prevents Selection's instance from
  carrying unused fields that confuse readers.)
- **`recordAiTranslation` is on the core factory.** Both consumers' AI translations populate
  `handTyped.{filename|regex}` based on `kind` so switching out of AI to refine works the same way. Selection's wrapper
  does NOT call any Search-extras setters from `recordAiTranslation`; the AI label and the AI pattern (for Search's
  Pattern chip) are the wrapper's job to populate via its extras module. (Resolves B8.)
- **Selection has three modes: AI / Filename / Regex. No Content.** Selection's ModeChips renders the same chip set as
  Search minus the visible-disabled Content chip. The shared `ModeChips` component takes a `modes` option array; Search
  passes `[ai?, filename, content(disabled), regex]`; Selection passes `[ai?, filename, regex]`. The "ai?" entry is
  conditional on `aiEnabled` per the shared rule.
- **Selection AI requires a cloud provider.** Local models (llama-server, 4K-8K context) can't reliably fit a 200+-name
  folder sample plus the prompt and response. When `ai.provider === 'local'`, Selection's AI chip stays hidden (same
  gate as Search's AI chip when AI is off entirely, just with a different reason). The implementing agent surfaces a
  tooltip on the gate: "AI selection needs a cloud provider. Set one in Settings > AI." (Resolves G7.)
- **`event.key === '+'` and `event.key === '-'`, no modifiers other than the implicit Shift on `+`.** Total Commander
  parity. See § "Keyboard contract" for the exact filter.

## Milestones

Each milestone:

- One commit (or a small handful), FF-merged to main without review per David.
- Tests written ahead of code where reasonable (TDD).
- `./scripts/check.sh` green before the milestone closes. `--include-slow` only at the very end (M11).
- CLAUDE.md files updated in lockstep with the code they describe.

### M1: Generic `ToggleGroup` in `lib/ui/`

**Why.** Phase 0 of the design: one visual primitive used by Settings and the mode chips. Lets us replace
`SearchModeChips`'s bespoke button row in M3 without rebuilding the keyboard / a11y story. The two consumers have
different a11y semantics (tab strip vs toggle group), so the primitive supports both via a `semantics` prop; see § "Mode
chips" in Design summary for the rationale.

**What.**

- Create `lib/ui/ToggleGroup.svelte` with props per § "Mode chips":
  `{ semantics: 'tabs' | 'toggles'; value; options; onChange; ariaLabel; disabled? }`.
- For `semantics: 'toggles'`: wraps `@ark-ui/svelte/toggle-group`'s `ToggleGroup.Root` + `ToggleGroup.Item`. Mirrors
  today's `SettingToggleGroup`'s Ark usage. Single-select.
- For `semantics: 'tabs'`: renders `<div role="tablist">` + `<button role="tab" aria-selected>`. Arrow keys skip
  disabled options. Active option is `tabindex=0`; others `tabindex=-1`. Mirrors today's `SearchModeChips` behavior; the
  implementing agent ports the existing arrow-key motion and `chipButtons[]` ref pattern from `SearchModeChips.svelte`
  verbatim (don't rewrite the algorithm; keep it).
- Option cells support `badge` (small uppercase pill, mono font, accent subtle background), `hint` (mono tertiary text,
  for example `⌥A`), `disabled`, and `tooltip`. The disabled-with-tooltip case is "visible-disabled with tooltip" per
  the search redesign's "Coming soon" idiom: `disabled={true}` on the button, but the tooltip on hover/focus is still
  active. Verify Ark's `ToggleGroup.Item` honors tooltip on disabled state for the toggles branch (it does; the disabled
  attribute doesn't block hover events on the parent).
- Shared visual CSS at the component level so both ARIA shapes render identically. Use the existing CSS tokens that
  `SettingToggleGroup.svelte` already defines (border, radius, hover, active background); promote them to
  `lib/ui/ToggleGroup.svelte` and import them from there.
- Refactor `lib/settings/components/SettingToggleGroup.svelte` to be a thin wrapper that builds the options array from
  the setting definition and delegates to `<ToggleGroup semantics="toggles" … />`. Public API
  (`{ id, disabled, labelOverrides }`) unchanged.

**Tests.**

- `ToggleGroup.test.ts`: option rendering (with and without badge/hint), click activation in both semantics, arrow-key
  motion in `semantics="tabs"` skipping disabled options, value update on click in both semantics, disabled root
  short-circuits clicks.
- `ToggleGroup.a11y.test.ts` (tier 3 axe-core): one audit per semantics. Confirms `role="tablist"` shape in tabs mode,
  ToggleGroup.Root attributes in toggles mode, badge + hint don't break the accessible name (axe rule
  `accessible-name-computation`), disabled-with-tooltip passes contrast (tooltip is rendered to body via the singleton;
  only the disabled-button label is in the cell).
- Existing `SettingToggleGroup.a11y.test.ts` MUST stay green.
- Existing `SettingToggleGroup` rendering tests stay green.

**Docs.**

- Add a section to `lib/ui/CLAUDE.md` documenting the primitive, the two semantics modes, and when to use each (tabs =
  drives a UI mode, toggles = picks a stored value).

**Checks.**

- `./scripts/check.sh --fast` after each substantial edit.
- `./scripts/check.sh` before commit.

**Definition of done.**

- Primitive lands, settings ToggleGroup migrated, all existing settings tests green, new tests green, no visual
  regression to Settings (verify by running the app and opening Settings > Appearance > File and folder sizes which uses
  the size-unit toggle group). `SearchModeChips` is NOT yet migrated in this milestone (it migrates in M3 as part of the
  rename); M1 only ships the primitive.

**Risk.** Low. The Ark dependency is already in the tree (`@ark-ui/svelte/toggle-group`). The bespoke tabs branch ports
existing working code.

### M2: Factory `createQueryFilterState()` plus Search-extras split

**Why.** Today's `lib/search/search-state.svelte.ts` is a 713-line module-singleton. Two consumers need two state
instances. We convert it to a factory AND split out the Search-only fields so Selection's instance doesn't carry unused
state. Without this, none of the following milestones work.

**What.**

- Create `lib/query-ui/query-filter-state.svelte.ts` exporting `createQueryFilterState(options): QueryFilterState`.
  Cross-consumer fields ONLY (per the Decision log entry):

  ```
  query, mode, sizeFilter, sizeValue, sizeUnit, sizeValueMax, sizeUnitMax,
  dateFilter, dateValue, dateValueMax, caseSensitive,
  lastAiPrompt, lastAiCaveat, handTyped (per-mode buffers),
  results, totalCount, cursorIndex, isSearching,
  imeComposing, lastDialogEvent, runOnMount, lastRunQuery
  ```

  Methods mirror today's public API but become instance methods: `state.getQuery()`, `state.setQuery(s)`,
  `state.setQueryFromUserInput(s)`, `state.switchMode(m)`, `state.recordAiTranslation({pattern, kind, label})`,
  `state.deriveEnterAction()`, etc. The `recordAiTranslation({label})` argument is preserved but the LABEL is stored in
  the consumer's extras module, not on the core state. The core state writes pattern → `handTyped.X` per the existing
  M2/R3 contract (see `lib/search/CLAUDE.md` § "Round 3 polish").

- Create `lib/search/search-extras-state.svelte.ts` exporting `createSearchExtrasState(): SearchExtrasState` for
  Search-only fields: `scope`, `excludeSystemDirs`, `lastAiLabel`, `lastAiPattern`, `lastAiPatternKind`. Search's
  wrapper calls both factories and composes them. No coupling.
- Move `buildSearchQuery()` (search-only; returns the IPC payload for `searchFiles`) to a
  `lib/search/build-search-query.ts` module next to the extras. Selection has its own `buildSelectionMatchQuery()`
  helper that the matcher consumes (declared in M7).
- **Split `recordAiTranslation`.** Today's function in `search-state.svelte.ts` writes `lastAiPattern`,
  `lastAiPatternKind`, `lastAiLabel`, AND the `handTyped[mode]` buffer in one call. For the core/extras split to work,
  M2 splits this function: the core factory's version updates ONLY `handTyped[mode]` (so Selection's wrapper reuses it
  as-is); Search's extras module exposes `recordAiPatternAndLabel({ pattern, kind, label })` that Search's wrapper calls
  right after the core call. Two calls in sequence from Search's wrapper; one core call from Selection's wrapper. Pin
  this contract with a unit test on each side.
- `lib/search/search-state.svelte.ts` becomes a thin façade: instantiates the core factory + extras, re-exports the
  instances for backward-compat during the transition (so Search's existing call sites work via re-export while M3
  renames them). Drop the façade in M3.
- Update Search's call sites to import from the new instances. Real scope: 15 files importing from
  `search-state.svelte`, ~471 identifier usages. Most usages are inside `SearchDialog.svelte` and
  `SearchFilterChips.svelte`. TypeScript catches every missed rename.
- Move the relocated pure helpers (`enter-action.ts`, `recent-chips-layout.ts`) out of `lib/search/` and into
  `lib/query-ui/`. STAY in `lib/search/`: `snapshot-label.ts`, `searchable-folder.ts`, `capabilities.ts`,
  `snapshot-store.svelte.ts`, the new `search-extras-state.svelte.ts`, the new `build-search-query.ts`. STAY but
  RELOCATE only in M3: `filter-chip-state.ts`, `filter-popover-helpers.ts`, `recent-searches-utils.ts`.
- All existing Search tests stay verbatim, just with import paths updated. Add `query-filter-state.test.ts` (mirrors
  existing `search-state.test.ts`) and `search-extras-state.test.ts` (covers the Search-only state shape).

**Docs.**

- New `lib/query-ui/CLAUDE.md` documenting the factory's contract, the field list, and the "extras live next to the
  consumer" pattern.
- Update `lib/search/CLAUDE.md` to reflect the new state location and the extras-module split. See § "CLAUDE.md split
  sheet" below the milestones for which decisions go where.

**Checks.**

- `./scripts/check.sh --fast` after each batch of import-rewrites.
- `./scripts/check.sh` before commit.
- Run all of Search's existing Vitest tests (`pnpm vitest run --testPathPattern='lib/search'`) and the new query-ui
  tests.

**Definition of done.**

- Search's behavior unchanged (verified by full Vitest run and a manual smoke test of the search dialog through MCP).
- All identifier call sites use the factory or extras instance.
- New tests for the factory shape pass.

**Risk.** This is the riskiest milestone. ~471 identifier usages is a lot of mechanical change; one missed rename
produces a runtime error. Mitigation: TypeScript's compiler catches all of them. The façade trick (re-export instances
from `lib/search/search-state.svelte.ts`) lets us land the factory + extras split first, then rename call sites in M3
batch. After M3, drop the façade. Search's existing test suite is the behavior safety net.

### M3: Rename and extract the shared components into `lib/query-ui/`

**Why.** With state factored, we can move the presentational components into their new home without orchestration
changes.

**What.**

- Rename `SearchBar.svelte` → `QueryBar.svelte` in `lib/query-ui/`. Same props, same behavior.
- Rename `SearchModeChips.svelte` → `ModeChips.svelte` in `lib/query-ui/`. Reimplement on top of
  `lib/ui/ToggleGroup.svelte` with `semantics="tabs"` (uses the new badge and hint slots for `AI` badge + `⌥A`/`⌥F`/`⌥R`
  hints). Same external props; "Content" stays visible-disabled with tooltip in Search; absent entirely in Selection.
- Rename `AiTransparencyStrip.svelte` → `AiPromptStrip.svelte` in `lib/query-ui/`. Verbatim move.
- Rename `SearchFilterChips.svelte` → `FilterChips.svelte` in `lib/query-ui/`. New visibility props:
  `scopeChipVisible: boolean`, `patternChipVisible: boolean`. Defaults match Search's current behavior. Selection will
  pass `scopeChipVisible: false`. The "Pattern chip is search-specific" note in the existing CLAUDE.md is wrong now
  (clarification 5): Pattern is used by both consumers in AI mode. The chip stays.
- Rename `SearchResults.svelte` → `QueryResults.svelte` in `lib/query-ui/`. Takes the results from the parent (was
  already prop-driven). The "path column" stays renderable but its content is empty for Selection-against-current-
  folder entries (since `parentPath` is irrelevant when everything is in one folder). Visibility of the path column is
  driven by a new prop `showPathColumn` (default `true` for Search; Selection passes `false`).
- Rename `PathPills.svelte`, `SearchRowMenu.svelte`, `EmptyState.svelte`, `FilterChip.svelte`,
  `FilterChipPopover.svelte` into `lib/query-ui/`. Verbatim moves. (Yes, `SearchRowMenu` keeps the name; it's still a
  row-menu component, and renaming everything labeled "search" hurts grep more than helps.)
- Rename `RecentSearchesFooter.svelte` → `RecentItemsFooter.svelte`, `RecentSearchesPopover.svelte` →
  `RecentItemsPopover.svelte`, in `lib/query-ui/recent-items/`. Both components become generic over an `Entry` type via
  an adapter callback. Adapter signature:

  ```ts
  type RecentItemsAdapter<E> = (entry: E) => {
    label: string // primary text on the chip (the query, truncated)
    tooltip: string // full text on hover (the full query)
    mode: SearchMode // for the mode badge ("AI"/"FN"/"RX") on the chip
    ageLabel: string // "now" / "5m" / "2h" / "yesterday"
    ariaLabel: string // full accessible name for the chip button
  }
  ```

  Search instantiates with `Entry = SearchHistoryEntry`; Selection with `Entry = SelectionHistoryEntry`. The adapter
  lives next to each consumer's `*-history-state.svelte.ts` file. `recent-chips-layout.ts` (the greedy-fit packing
  helper) only sees the adapted `{label, tooltip}` so packing is the same for both. Tests:
  `RecentItemsFooter.svelte.test.ts` runs against both consumer instantiations.

- Convert `recent-searches-state.svelte.ts` into a factory `recent-items-state.svelte.ts` that takes the IPC funcs
  (`{ getRecent, addRecent, removeRecent, clearRecent, applyMaxCount }`) and returns the same reactive store shape.
  `lib/search/recent-searches-state.svelte.ts` becomes a thin file that constructs the factory with the
  `getRecentSearches`-family IPCs.
- All renames update every import site in the repo. `cmdr/no-raw-tauri-invoke` and import-cycle checks must stay green.
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

**Why.** The orchestrator becomes the shared primitive. Search becomes the first consumer with its full current behavior
expressed as a config.

**What.**

- Create `lib/query-ui/QueryDialog.svelte` carrying today's `SearchDialog.svelte`'s orchestration (overlay, mount,
  unmount, keyboard dispatch, `lastDialogEvent` lifecycle, IME guard, auto-apply, `deriveEnterAction`, `runOnMount`).
- Define `QueryDialogConfig`:

  ```ts
  interface QueryDialogConfig {
    title: string
    maxWidth: string // for example 'min(1080px, 80vw)'
    state: QueryFilterState // the factory instance from M2
    aiEnabled: boolean
    visibleChips: { size: boolean; date: boolean; scope: boolean; pattern: boolean }
    showPathColumn: boolean
    runHintCopy: string // "Press Enter to search" / "Press Enter to filter"
    historyStore: RecentItemsStore // from the recent-items factory
    emptyState: {
      examples: Array<{ kind: 'ai' | 'pattern' | 'regex'; label: string }>
      indexHint?: string // only Search uses this
      keyboardHint?: string
    }
    runQuery: (q: BuiltQuery) => Promise<{ entries: ResultEntry[]; totalCount: number }>
    translateAi?: (prompt: string) => Promise<AiTranslateResult>
    aiContext?: () => string[] // Selection passes folder sample
    primaryAction: {
      label: string
      shortcutHint: string // for example '⌥⏎' or '⏎'
      handler: (entries: ResultEntry[]) => void | Promise<void>
    }
    secondaryAction?: { label: string; shortcutHint: string; handler: (entry: ResultEntry) => void }
    onMount?: () => void | Promise<void> // Search: prepareSearchIndex
    onDestroy?: () => void // Search: releaseSearchIndex
    /* Plus accessibility / aria labels per consumer */
  }
  ```

  **`runHintCopy`**: Search passes `"Press Enter to search"`; Selection passes `"Press Enter to filter"`. The
  `QueryBar`'s right-gutter hint reads this string when shown. Locked in M4. (Resolves G14.)

  **`aiContext`**: called once per AI translation, immediately before the IPC call. NOT called on every keystroke.
  Selection snapshots the focused pane's listing at dialog open and calls `aiContext` to return the sample on each AI
  run. If the focused pane changes mid-dialog (rare; mouse-click on the other pane), the snapshot does NOT refresh; the
  user opened the dialog on a folder, they're filtering that folder. Locked in M4. (Resolves G15.)

  **`lastDialogEvent` ownership**: `QueryDialog` writes to `state.lastDialogEvent` on these events: dialog opened
  (`'opened'`), user typed in the bar (`'query-edited'`), filter chip changed (`'filter-edited'`), `runQuery` promise
  resolved with results (`'results-arrived'`), cursor moved via ↑/↓ (`'cursor-moved'`). The consumer's `runQuery`
  callback DOES NOT write to `lastDialogEvent`; only `QueryDialog` does, after the promise resolves. This keeps the
  Enter ownership swap (`deriveEnterAction`) deterministic.

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
- The Search wrapper (`lib/search/SearchDialog.svelte`) builds the `QueryDialogConfig` and registers Search-specific
  lifecycle hooks (`onMount` calling `prepareSearchIndex`, `onDestroy` calling `releaseSearchIndex`, the MCP
  `mcp-open-search-dialog` listener, `runOnMount` consumer for MCP prefill, `primaryAction.handler` doing the snapshot
  promotion, `secondaryAction.handler` doing "Go to file"). No orchestration logic lives in the wrapper. The wrapper's
  size is whatever Search-specific glue costs; no line-count target.

**Risk.** Highest. The orchestrator carries most of the behavior. Mitigation: the existing Search test suite runs
verbatim; if any of those fail, we know what broke. Add a focused integration test for the config-driven paths
(`secondaryAction` callback fires on Enter when `deriveEnterAction === 'go-to-file'`; `primaryAction` callback fires on
⌥⏎; etc.) so regressions in either consumer surface quickly. Add a unit test pinning the `lastDialogEvent` ownership
contract (QueryDialog writes; consumer's runQuery does not).

### M5: Selection backend (Rust): history store, AI translation IPC

**Why.** Frontend Selection needs working IPCs to write recents and to call AI translation. Building them first lets M6
land cleanly without backend churn.

**What.**

- Create `src-tauri/src/selection/` with `mod.rs`, `history.rs`, `ai/{mod,prompt,parser,query_builder}.rs`.
- `selection/history.rs` mirrors `search/history.rs`: same atomic write, schema-versioned, in-memory mutex, DISK_LOCK,
  canonical-key dedupe (over the narrower selection fields), schema-version quarantine. File:
  `{app_data_dir}/selection-history.json`.
- New IPC commands in `src-tauri/src/commands/selection.rs`:
  - `get_recent_selections(limit)`, `add_recent_selection(entry, max_count)`, `remove_recent_selection(id)`,
    `clear_recent_selections()`, `apply_recent_selections_max_count(max_count)`.
  - `translate_selection_query(prompt: String, sample_names: Vec<String>) -> Result<SelectionTranslateResult, String>`.
- `selection/ai/prompt.rs` defines the classification prompt. Same key-value response style as `search/ai/prompt.rs` (no
  JSON). The implementing agent writes the prompt and runs at least 6 manual evaluations via the OpenAI API on
  representative folder samples to confirm output quality. Use the model configured in Settings > AI > Cloud > OpenAI
  (David's account). The agent reads the model name from the running app's settings, not from this plan. Pin the prompt
  with a docstring describing what the evaluations covered.
- `selection/ai/parser.rs` parses key-value response into `ParsedSelectionLlmResponse`. Reuses
  `search/ai/parser.rs::parse_key_value_line` if exported, or re-implements lightly.
- `selection/ai/query_builder.rs` assembles
  `SelectionTranslateResult { pattern, kind, size_filter?, date_filter?, caveat?, label? }`.
- Add `selection.recentSelections.maxCount` to the settings registry (default 1000). Wire `settings-applier.ts` and the
  matching Rust live-apply hook.
- Register the new commands in `ipc.rs` and `ipc_collectors.rs` for specta. The selection commands are NOT debug
  commands and should NOT be added to the specta exclusion list. After regen, they appear in `bindings.ts` and are
  callable via `commands.translateSelectionQuery(...)`, etc. from `$lib/tauri-commands`.
- Run `pnpm bindings:regen`. Commit the regenerated `bindings.ts` separately or in the same commit (per repo style).
- **Update capability files.** Per `AGENTS.md`: Tauri APIs fail silently without permissions. The six new commands must
  be allowed:
  - `apps/desktop/src-tauri/capabilities/default.json`: add `translate_selection_query`, `get_recent_selections`,
    `add_recent_selection`, `remove_recent_selection`, `clear_recent_selections` for the main window (the Selection
    dialog calls these from the main window).
  - `apps/desktop/src-tauri/capabilities/settings.json`: add `apply_recent_selections_max_count` for the settings window
    (live-apply from the Settings UI).
  - Verify by opening Cmdr's settings window after the change and confirming `apply_recent_selections_max_count` works
    without the "not allowed" error.
- **AI provider gate.** The frontend hides the AI chip when `ai.provider !== 'cloud'`. Surface a tooltip on the gate
  itself when missing ("AI selection needs a cloud provider. Set one in Settings > AI."). The IPC
  `translate_selection_query` returns an error if the cloud provider isn't configured (mirror Search's
  `resolve_ai_backend` error path).

**Tests.**

- Unit: `selection/history.rs` mirror-tests for the search history tests (atomic write, schema migration, dedupe, cap
  eviction, schema-version quarantine, cap=0 disables persistence).
- Unit: `selection/ai/parser.rs` round-trip tests for representative model responses
  (`pattern: *.png\nkind: glob\nsize_min: 1048576`). Mirror `search/ai/parser.rs`'s table-driven test style.
- Unit: `selection/ai/query_builder.rs` tests covering the three filter combinations (pattern only, pattern + size,
  pattern + date) and the broken-LLM-response path (returns caveat, not a half-built query).
- Integration: an offline test that fakes the LLM IPC and verifies `translate_selection_query` end-to-end.
- IPC contract tests (`lib/ipc/*.test.ts` via `installIpcMock()`): per `docs/testing.md` § "When you add X, also add Y",
  destructive and >2-arg commands need contract tests. Cover `clear_recent_selections` (destructive) and
  `apply_recent_selections_max_count` (cross-window live-apply).
- Real-LLM eval: a `tests/selection_ai_eval.rs` integration test that uses the OpenAI API with David's credentials.
  Behind a feature flag. **Reuse the existing AI eval feature flag if one exists.** Check `Cargo.toml` for any `ai-eval`
  / `ai-evaluation` / similar feature; if it doesn't exist yet, add `selection-ai-eval`. Run manually with
  `cargo test --features <flag> -- selection_ai_eval` during this milestone to validate the prompt design on the real
  model. **No `cargo deny` re-run needed**: no new Cargo deps in this milestone.

**Docs.**

- `src-tauri/src/selection/CLAUDE.md` documenting the module shape, prompt, parser, history store, IPC.
- `src-tauri/src/search/CLAUDE.md`: mention that the AI parser helpers are now used by both subsystems.
- `docs/architecture.md`: add a `selection/` row.

**Checks.**

- `./scripts/check.sh --check go-vet --check clippy --check rust-tests --check bindings-fresh` after the Rust changes.
- Full `./scripts/check.sh` before commit.

**Definition of done.**

- All new Rust unit tests pass.
- `bindings-fresh` is green.
- Manual API call against the configured cloud model returns a parseable response for a sample prompt and a sample
  folder of ~50 names. (The agent configures the cloud provider via Cmdr's running Settings UI through MCP first; see
  Risk register R3 for the configuration flow.)

**Risk.** Low for history (it's a mirror of an existing module). Medium for AI prompt (may need iteration). Mitigation:
the real-LLM eval test in this milestone catches prompt drift before the dialog wraps around it.

### M6: Selection backend (Rust): pane-side `applyIndices` plumbing

**Why.** The dialog applies its result by calling a method on the focused pane. We need that method to exist in
`FilePane.selection` first so M7 can wire it.

**What.**

- In `lib/file-explorer/pane/selection-state.svelte.ts`, add
  `applyIndices(idxs: number[], mode: 'add' | 'remove', hasParent: boolean): void`. Skip index 0 if `hasParent`, same
  rule as `selectAll`. Fires `onChanged?.()`.
- In `lib/file-explorer/pane/FilePane.svelte`, expose `applyIndices(idxs: number[], mode: 'add' | 'remove')` calling
  into selection state. Skip `..` per hasParent.
- In `DualPaneExplorer.svelte`, expose `applyIndicesToFocusedPane(idxs, mode)` that resolves the focused pane and
  forwards.
- For `search-results://` panes: the dialog's match runs against `entry.name` (which is the displayed friendly path on
  snapshot panes per § Match semantics; NOT `entry.path`). `applyIndices` operates on indices into the snapshot's
  `entries[]` exactly as for regular panes. Document this in `lib/selection-dialog/CLAUDE.md` (M7 milestone).

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

- `lib/selection-dialog/folder-sampler.ts` (pure):
  `sampleFolderNames(names: string[], cursorIndex: number, max: number)`.
- `lib/selection-dialog/selection-matching.ts` (pure): `matchEntries(getNameFor, total, query) → number[]` where
  `query = { pattern, kind: 'glob' | 'regex', caseSensitive, size?, date? }`. Compiles the glob to a `RegExp` (same
  rules as Search's filename glob) or uses the user's regex. Iterates indices, returns matches.
- `lib/selection-dialog/selection-history-state.svelte.ts`: constructs a recent-items factory store with the
  selection-IPCs. Same shape as `lib/search/recent-searches-state.svelte.ts`.
- `lib/selection-dialog/SelectionFooterAction.svelte`: a single button "Select these files ⏎" / "Deselect these files ⏎"
  depending on mode. Reuses the existing footer-button CSS conventions.
- `lib/selection-dialog/SelectionDialog.svelte`: the thin wrapper. Reads the focused pane, builds the entry iterator,
  builds the matcher, builds the `QueryDialogConfig`, mounts `QueryDialog`. Calls
  `explorerRef.applyIndices(matchedIndices, mode)` on commit.
- Add `selection.selectFiles` and `selection.deselectFiles` to `command-registry.ts` with scope
  `'Main window/File list'` (matches existing `selection.selectAll` / `selection.deselectAll`). Do NOT add a new scope
  literal to `CommandScope` in `lib/commands/types.ts`. Shortcuts: `+` (the add command) and `-` (the remove command)
  per the keystroke binding in § "Keyboard contract".
- Add the dispatch cases to `handleCommandExecute` in `routes/(main)/command-dispatch.ts`. The handler flips a route
  state flag `showSelectionDialog: 'add' | 'remove' | null` similar to how `showSearchDialog` works.
- `routes/(main)/+page.svelte`: mount `SelectionDialog` when the flag is set; pass the focused pane handle.
- `lib/shortcuts/shortcuts-store.ts`: register the new commands in `menuCommands` (if menu-bound, which they will be per
  M8).
- `lib/file-explorer/pane/FilePane.svelte`: in the keydown handler, if (`event.key === '+'` OR `event.key === '-'`) AND
  `!e.metaKey && !e.altKey && !e.ctrlKey` AND focus is on the pane (not an input element), preventDefault and dispatch
  `selection.selectFiles` or `selection.deselectFiles`. Note we do NOT test `event.shiftKey`: Shift+= IS the way US
  QWERTY users produce `event.key === '+'`. Add a unit test pinning the exact filter logic in a new
  `file-pane-keyboard.test.ts` case (or extend the existing test file).
- **Selection's modes are AI / Filename / Regex.** No Content. The `ModeChips` instance in `SelectionDialog.svelte`
  receives a modes array without Content. Same external behavior as Search's mode chips otherwise (⌘1/⌘2/⌘3, ⌥A/F/R,
  arrow-key motion).
- **AI provider gate.** Hide the AI chip when `getSetting('ai.provider') !== 'cloud'`. Surface a tooltip on the chip-row
  gate (or, if the chip is absent entirely, no tooltip needed; the mode just doesn't appear). Subscribe via
  `onSpecificSettingChange('ai.provider', ...)` so the chip appears/disappears live without reopening the dialog.
- **Mid-dialog provider switch.** If the user has Selection open in AI mode and the provider gets switched off in
  another window, mirror Search's existing "auto-mode fallback" gotcha (`lib/search/CLAUDE.md` § Gotchas, "Auto mode
  fallback when AI gets disabled mid-session"): flip `state.mode` to `'filename'`, hand the AI prompt to the
  `handTyped.filename` buffer if Selection's AI just translated something, so the user keeps their work. The AI prompt
  strip clears on the next non-AI search (same lifecycle as Search). Add a Vitest test that pins the fall-back.
- **Recent-selection chip apply.** Clicking a recent-selection chip restores `query`, `mode`, `caseSensitive`,
  `sizeFilter`/value/unit, `dateFilter`/value to the entry. No scope, no excludeSystemDirs (Selection doesn't have
  them). Implement as `applySelectionHistoryEntry(state, entry)` in
  `lib/selection-dialog/selection-history-state.svelte.ts`. Add a unit test that confirms state matches the entry after
  apply.
- **Snapshot-pane interaction with mid-dialog mutation.** Selection's matcher runs at COMMIT time, not at preview time.
  The preview shows live results as the user types; on Enter, the dialog re-runs the matcher against the snapshot's
  CURRENT entries (which may have shrunk via `removeEntryFromAllSnapshots` during the dialog) and applies indices to the
  current shape. Document this in `lib/selection-dialog/CLAUDE.md` with a "Why: snapshot can mutate mid-dialog because
  cross-snapshot delete may fire while the dialog is open." Add an integration test: open dialog on a snapshot pane,
  synthesize a `removeEntryFromAllSnapshots` mutation, commit, confirm the matched indices are computed against the
  post-mutation snapshot shape.

**Tests.**

- Unit: `folder-sampler.test.ts` covering the cases (small folder, large folder, cursor near start / middle / end, empty
  folder, dedup).
- Unit: `selection-matching.test.ts` covering glob / regex / case sensitivity / size + date predicates / empty pattern /
  search-results pane (path-based match).
- Component: `SelectionDialog.svelte.test.ts` covering: mounts with the right title per mode; pressing ⏎ applies and
  closes; switching modes preserves the query; Cmd+N clears state; recent selections appear in the footer.
- A11y: `SelectionDialog.a11y.test.ts` mirroring `SearchDialog.a11y.test.ts` (tier 3).
- E2E (macOS Playwright): a single spec `selection-dialog.spec.ts` covering the Filename mode happy path: focus a pane,
  dispatch `selection.selectFiles` via `dispatchMenuCommand`, type `*.txt`, press Enter, confirm the matching rows
  became selected. Must run in <1 s per `AGENTS.md`'s testing rules. Do NOT cover AI mode in this spec; the cloud
  round-trip is 1-5 s and would flake CI. AI is exercised in M11's manual MCP smoke.
- E2E (Linux Docker, `e2e-linux/`): a separate minimal spec that opens the Selection dialog via the same
  `dispatchMenuCommand` and confirms it renders. Don't test the `+`/`-` keystroke binding on Linux; the Docker keyboard
  model is limited and the macOS Playwright spec covers the keyboard path.
- Proptest on `selection-matching.ts::matchEntries`: returns ≤ totalCount indices, no duplicates, all in
  `[0, totalCount)`, matches a deterministic glob/regex. Cheap insurance for a pure matcher.

**Docs.**

- `lib/selection-dialog/CLAUDE.md` documenting the dialog's wiring, the matcher, the sampler, the search-results-pane
  path-matching note, and the link to the shared `QueryDialog`.
- Update `lib/file-explorer/CLAUDE.md` § Selection to add a "Select / deselect files dialog" subsection pointing at
  `lib/selection-dialog/CLAUDE.md`.

**Checks.**

- `./scripts/check.sh --fast` during, `./scripts/check.sh` before commit.
- Manual smoke via MCP: open a real folder, hit `=`, try AI mode with a fake key first, then with the real OpenAI key
  (configure via Settings > AI > Cloud, see Risk register R3 for how to configure mid-session).

**Definition of done.**

- All new tests pass.
- E2E spec runs in <1 s.
- Manual MCP smoke confirms the dialog opens, runs queries, applies selection, AI translation works with the real model,
  recent selections appear after commit.

**Risk.** Low-medium. The risk is integration drift: a missing prop, a typo in a config field. Tests catch most; manual
smoke catches the rest.

### M8: New "Select" top-level macOS menu

**Why.** David asked for it. Anchors the new feature in the system menu bar so users discover it without reading docs.

**What.**

- Create the new `Select` submenu between `Edit` and `View` in BOTH platform menu builders:
  - `apps/desktop/src-tauri/src/menu/macos.rs::build_menu_macos`: add the new submenu between the existing Edit and View
    submenu blocks, registering Select all (⌘A), Deselect all (⌘⇧A), Select files… (no accelerator on macOS; see R9),
    Deselect files… (no accelerator).
  - `apps/desktop/src-tauri/src/menu/linux.rs::build_menu_linux`: same structural change.
- Remove the Select all / Deselect all items from the Edit menu in both files.
- Register the new menu item IDs in `apps/desktop/src-tauri/src/menu/menu_items.rs` (constants for the IDs).
- Update `apps/desktop/src-tauri/src/menu/mod.rs::menu_id_to_command` and `command_id_to_menu_id` with the four IDs
  (`select_all`, `deselect_all`, `select_files`, `deselect_files` map to `selection.selectAll`, `selection.deselectAll`,
  `selection.selectFiles`, `selection.deselectFiles`).
- Update `apps/desktop/src/lib/shortcuts/shortcuts-store.ts::menuCommands` to include all four command IDs.
- Verify macOS shows the menu correctly via the running app + MCP screenshot. Verify Linux via the e2e-linux suite and a
  manual Docker run if needed.

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
- Shortcuts work: `⌘A` from a focused pane selects all; `=` opens Select files dialog; `-` opens Deselect files dialog.
  (Bare `=` and `-` bind via the FilePane keydown handler from M7, not via menu accelerators, because macOS menu
  accelerators always carry the ⌘ modifier.)
- Linux fallback: the menu structure also works on Linux (Cmdr ships there). The implementing agent verifies via the
  e2e-linux suite.

**Risk.** Low.

### M9: Settings panel for `selection.recentSelections.maxCount`

**Why.** The setting exists in the registry from M5. It needs a UI surface so users can tune it. Mirror Search's "Recent
searches" row.

**What.**

- Add a row to `lib/settings/sections/SearchSection.svelte` (or a new SelectionSection if the section structure changes)
  for the new setting. Mirror the existing search.recentSearches.maxCount row.
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
- Run
  `cargo mutants --file src-tauri/src/selection/history.rs --file src-tauri/src/selection/ai/parser.rs --file src-tauri/src/selection/ai/query_builder.rs`
  and triage survivors.
- Run `pnpm exec stryker run` on the new TS files (`selection-matching.ts`, `folder-sampler.ts`).
- Review `EmptyState` examples on Selection with a 10-second usability check via the running app.
- Verify the title bar visual on both Search and Selection in light and dark mode. Check `a11y-contrast`.
- Verify keyboard contract: ⌘N inside Selection resets; ⌘H opens the recent-selections popover and fuzzy filter works.
- Verify that the search-results-pane Selection path works (focus a snapshot pane, hit `=`, match against full paths,
  apply, confirm the right rows became selected).
- Verify that the AI mode in Selection works end-to-end against the configured cloud model and that the AI transparency
  strip renders the prompt and caveat.

**Tests.**

- Any tests added during polish stay in this milestone.

**Docs.**

- Sweep all CLAUDE.mds touched in M1–M9 for accuracy. Verify the `lib/search/CLAUDE.md` and `lib/query-ui/CLAUDE.md`
  split is clean (no duplicate documentation; cross-links work).
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

## CLAUDE.md split sheet (used by M3)

`lib/search/CLAUDE.md` is 703 lines of decisions and gotchas. M3 splits the load-bearing content between
`lib/query-ui/CLAUDE.md` (new) and `lib/search/CLAUDE.md` (slimmed). To prevent duplication or accidental loss, here is
the tag for every load-bearing item currently in `lib/search/CLAUDE.md`. The implementing agent moves verbatim with the
tag; M10 verifies, not reconstructs.

| Section / decision in current `lib/search/CLAUDE.md`                                                                                                                                                                                                                                                                                                   | Tag                                                                                                                                                                                                                                                                                                                                                                                                                                                                  |
| ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| § "Files" (the big component table)                                                                                                                                                                                                                                                                                                                    | SPLIT: move shared rows to query-ui, keep search-only rows in search                                                                                                                                                                                                                                                                                                                                                                                                 |
| § "State shape (post-M4)"                                                                                                                                                                                                                                                                                                                              | both: query-ui owns the core fields, search owns the extras (B7)                                                                                                                                                                                                                                                                                                                                                                                                     |
| § "Round 3 polish (R3)" B1–B6, U1–U8, T1                                                                                                                                                                                                                                                                                                               | tag per item: B1/B5/U1/U2/U3/U4/U5/U7 → query-ui; B2/B3/B4/B6/U6/U8/T1 → search                                                                                                                                                                                                                                                                                                                                                                                      |
| § "Round 2 grid-style filter popovers"                                                                                                                                                                                                                                                                                                                 | query-ui (filter chips are shared)                                                                                                                                                                                                                                                                                                                                                                                                                                   |
| § "Round 2 D12: Use current folder smart fallback"                                                                                                                                                                                                                                                                                                     | search (scope is search-only)                                                                                                                                                                                                                                                                                                                                                                                                                                        |
| § "Round 2 R2: PathPills measurement"                                                                                                                                                                                                                                                                                                                  | query-ui (PathPills is shared)                                                                                                                                                                                                                                                                                                                                                                                                                                       |
| § "Keyboard shortcuts (in-dialog, hard-coded)" table                                                                                                                                                                                                                                                                                                   | query-ui (all consumers inherit)                                                                                                                                                                                                                                                                                                                                                                                                                                     |
| § "Round 2 D8: ⏎ ownership swap"                                                                                                                                                                                                                                                                                                                       | query-ui                                                                                                                                                                                                                                                                                                                                                                                                                                                             |
| § "Round 2 D9: scope shortcuts"                                                                                                                                                                                                                                                                                                                        | search                                                                                                                                                                                                                                                                                                                                                                                                                                                               |
| § "Round 2 D6: footer buttons always visible"                                                                                                                                                                                                                                                                                                          | query-ui (the policy); search (the specific Search footer buttons)                                                                                                                                                                                                                                                                                                                                                                                                   |
| § "Data flow" diagram                                                                                                                                                                                                                                                                                                                                  | search (index lifecycle is search-only); also add a query-ui diagram for the shared flow                                                                                                                                                                                                                                                                                                                                                                             |
| § "Key patterns" / Command palette pattern, two-cursor hover, live search debounce, auto-apply gates, ⏎ run button, "Press Enter" hint, scope row, index not available, AI single-pass flow, IME composition, deferred loading, state preservation, ⌘N, MCP open path, runOnMount, path pills with overflow, per-row … menu, footer right-edge actions | tag per pattern: command-palette / two-cursor / debounce / auto-apply / run-button / Press-Enter / IME / deferred-loading / state-preservation / ⌘N / runOnMount / path-pills / row-menu → query-ui; scope-row / AI-single-pass / index-not-available / MCP-open-path / footer-right-edge (because the SPECIFIC buttons are search-specific) → search                                                                                                                |
| § "Snapshot store M8a", "Closed-tab lifecycle and refs", "{#key activeTabId} recreation", "Capability flags M8c", "Cross-snapshot delete sync M8c", "Source-side ops from the snapshot pane M8d"                                                                                                                                                       | search (all about the snapshot machinery)                                                                                                                                                                                                                                                                                                                                                                                                                            |
| § "Key decisions" (all M10 load-bearing decisions)                                                                                                                                                                                                                                                                                                     | tag per decision: Unified bar+chips, Filter chips with popovers, MAX_HISTORY_PER_TAB → query-ui; Open-in-pane promotes to virtual volume, Recent-search history added on Open-in-pane only, AI mode never auto-applies → search; AI mode chips re-run on click → query-ui (general); RecentSearchesPopover reuses FilterChipPopover → query-ui; Pattern chip always rendered → query-ui (both consumers use it); Path pills mouse-only / not in Tab order → query-ui |
| § "Gotchas" (stopPropagation, prepareSearchIndex failure, clearSearchState in onDestroy, status bar empty, ⌘⏎ no-op, AI translation overwrite, nested-interactive a11y disable)                                                                                                                                                                        | tag per gotcha: stopPropagation, ⌘⏎ no-op, status-bar-empty, clearSearchState-in-onDestroy, AI-translation-overwrite, nested-interactive → query-ui; prepareSearchIndex failure, "Open in pane" M8b flow → search                                                                                                                                                                                                                                                    |
| § "References" (ai-search-eval-history.md)                                                                                                                                                                                                                                                                                                             | search                                                                                                                                                                                                                                                                                                                                                                                                                                                               |
| § "Dependencies"                                                                                                                                                                                                                                                                                                                                       | SPLIT: query-ui inherits the shared deps; search keeps its specific commands                                                                                                                                                                                                                                                                                                                                                                                         |

Process for M3: the implementing agent prints the current `lib/search/CLAUDE.md` in full, reads each section, and moves
chunks into the new file per the tags. Cross-links between the two files where a topic touches both (for example, the
Pattern chip's general design vs. Search's specific use). If the agent finds a section not on this sheet, they add a tag
in this plan and proceed.

**M3 also produces a content-loss check artifact.** Before committing M3, the agent runs:

```bash
# Capture pre-M3 baseline (from the worktree's parent branch / main)
git show main:apps/desktop/src/lib/search/CLAUDE.md > /tmp/search-claude-before.md

# Concatenate post-M3 result
cat apps/desktop/src/lib/search/CLAUDE.md apps/desktop/src/lib/query-ui/CLAUDE.md > /tmp/search-claude-after.md

# Word-count both
wc -w /tmp/search-claude-before.md /tmp/search-claude-after.md
```

The post-M3 combined word count must be ≥95% of the pre-M3 baseline (small reductions are acceptable for genuinely
redundant prose; substantial drops mean something got dropped, not moved). M10 verifies this artifact, not reconstructs
the split.

## Risk register

- **R1: M2 factory refactor breaks Search.** 15 files, ~471 identifier usages; one missed rewrite is a runtime error.
  Mitigation: TypeScript catches all missed imports; full Vitest run + manual MCP smoke before the commit.
- **R2: M4 orchestration extraction subtly changes Search behavior.** Risky because the orchestrator has the most state.
  Mitigation: Search's existing test suite (tier-3 a11y + dialog tests + Playwright e2e). Add new integration tests for
  the `primaryAction` / `secondaryAction` callback paths so the config-driven contract is pinned.
- **R3: OpenAI API key handling for testing.** David has the key in Keychain. We need to configure Cmdr in dev mode to
  use OpenAI as the cloud AI provider. The implementing agent does this once via the Settings UI through MCP: open
  Settings, select AI → Cloud → OpenAI, paste the key retrieved via
  `security find-generic-password -s OPENAI_API_KEY -a veszelovszki -w`, set the model to whichever OpenAI model David
  has access to. (David's curl example used `gpt-5.5`; the agent uses whichever model is shown in his cloud provider's
  `/models` listing.) The key is persisted in macOS Keychain via the existing `saveAiApiKey` IPC. No keys in any
  committed file.
- **R4: Selection's `QueryResults` view has an empty path column for current-folder entries.** It's fine but visually
  it's a wasted column. Mitigation: a `showPathColumn: boolean` prop on `QueryResults`; Selection passes `false`. Search
  keeps it `true`. (Already in the design.)
- **R5: `event.key === '+'` and `event.key === '-'` clashes with text fields.** If focus is on a text input (rename
  editor, search bar inside the pane, settings), the keystroke should NOT open the dialog. Mitigation: the dispatch
  guard in FilePane checks that focus is on the pane (`event.target === paneRoot` or the cursor row), not on any input.
- **R6: AI prompt for Selection produces invalid glob/regex.** The model might emit a malformed regex. Mitigation: the
  parser validates the response; on parse failure, surface a caveat in the AI strip ("Couldn't translate; try again or
  use Filename mode") and don't apply a broken pattern. The frontend matcher already handles the empty-pattern case
  gracefully (returns `[]`).
- **R7: Selection in `search-results://` panes matches against the displayed friendly path.** Per B3 resolution, the
  accessor returns `entry.name` (which IS the friendly full path in snapshot panes, with `~` for home) so "what the user
  sees is what they match". Risk is users expecting basename matching. Mitigation: the dialog shows a small hint near
  the bar when the focused pane is a snapshot: "Matching what's shown in the list (the full path)". Place in
  `QueryDialog` as a conditional banner driven by the consumer's config (a new field `noticeBanner?: string` on
  `QueryDialogConfig`).
- **R8: `selection-history.json` corruption on crash.** Mirrors `search-history.json`'s atomic-write story. Same
  schema-version quarantine on parse failure.
- **R9: The new `Select` menu accelerators `=` and `-` show up in the macOS menu bar with the wrong glyphs.** On macOS,
  menu accelerators always carry the ⌘ key as a modifier. Bare `=` and `-` aren't valid menu accelerators. Mitigation:
  register the menu items WITHOUT accelerators; the shortcut binding lives in the FilePane keydown handler. The menu
  shows the items as "Select files…" with no accelerator badge. This is the same approach as Space (for Quick Look) and
  Insert (for toggleAndDown), neither of which carry a menu accelerator.

## Feedback loops and tooling

Per David's instructions:

- **Don't fly blind.** Run the app via `pnpm dev` between milestones. Use MCP CLI calls (the agents will need to curl
  the running MCP server at `localhost:19225` since auto-connect doesn't work). Inspect the dev console logs via the
  `tauri MCP`'s `read_logs` tool. Take screenshots between milestones to confirm the title bar, the mode chips via the
  new ToggleGroup, and the empty state for both consumers.
- **TDD wherever reasonable.** For pure helpers (`folder-sampler`, `selection-matching`, the matcher's edge cases),
  write tests first. For UI components, write the a11y test alongside the component (tier 3).
- **`./scripts/check.sh` after each milestone.** No exceptions. `--include-slow` only at M11.
- **E2E specs run in <1 s each.** The single Selection spec in M7 must hit that bar. Use `dispatchMenuCommand`,
  `waitForSelector`, and `pollUntil`; never `sleep()`.

## Parallelism notes

Almost everything here is sequential. The factory refactor (M2) gates M3–M10. The Rust backend (M5) gates the frontend
dialog (M7). M6 (pane API) is independent of M5 (backend IPC) and could run in parallel, but the agents work
sequentially per the execute workflow; this isn't a critical-path savings worth chasing.

Two safe-to-parallelize pieces inside individual milestones (single-file edits, no shared state):

- M5: the `history.rs` mirror and the `ai/` module are independent; one agent can build both in parallel bash commands,
  but it's not worth a worktree.
- M7: the pure helpers (`folder-sampler.ts`, `selection-matching.ts`) and the wrapper component
  (`SelectionDialog.svelte`) can be built side by side; tests for the helpers don't depend on the component existing.

These are notes; the executing agents work sequentially by default. Parallelize only when you can run two commands that
genuinely don't touch each other's files.

## Open questions deferred to execution

These don't block the plan; the executing agents resolve them as they go:

- The exact wording of the AI prompt in `selection/ai/prompt.rs`. Seed in M5; refine via the eval test.
- Final copy on the EmptyState examples. Seed in § "Empty state copy" above; refine in M10 with the real running app.
- Whether the title bar in Search should say "Search" or "Search files…" for parity with Selection's "Select files…". I
  (the planner) lean "Search" since it's a verb and reads cleanly; the executing agent confirms when building M4.

## Definition of done (whole plan)

- [ ] All 11 milestones committed to `worktree-selection-dialog` and ready to FF-merge.
- [ ] `./scripts/check.sh --include-slow` is green.
- [ ] `bindings-fresh` is green.
- [ ] Manual smoke via MCP: both Search and Selection work end-to-end, including AI mode against the configured cloud
      model, in both light and dark mode.
- [ ] No CLAUDE.md is stale.
- [ ] `docs/architecture.md` reflects the new `lib/query-ui/` directory and the new `selection/` Rust module.
- [ ] The `Select` menu shows correctly on macOS.
