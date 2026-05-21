# Search window redesign: plan

Status: **drafting**. This is a temp spec; once shipped, the durable knowledge lands in `apps/desktop/src/lib/search/CLAUDE.md` and adjacent docs, and this file gets wiped on the next docs sweep.

---

## 1. Why we're doing this

The current `apps/desktop/src/lib/search/` is a working MVP and looks the part: three rows of form-shaped inputs, every input outlined, the AI prompt and the filename pattern living in separate rows fighting for the same job, filter controls competing for the eye with the results below. It's functional but it doesn't feel like the rest of Cmdr, and it doesn't feel like a Mac app. We have time, and search is one of the screens users will look at most. We're rebuilding the visual shell from scratch.

We're also adding capability that doesn't exist yet:

- **Recent searches** (persisted, last 1,000 entries, the last ~6 shown in-dialog, all reachable via an in-dialog popover behind `⌘H`).
- **Open in pane**: promote a result set into a real (virtual-volume) pane view so the user can act on it with the full file-explorer toolkit and come back to it via the navigation history.
- **Auto-apply** on a 1 s debounce for filename and regex modes only, behind a setting (default on).
- **State preservation** across dialog close/reopen within a session.
- **MCP tool** for opening the dialog pre-filled.

What we are **not** doing:

- Content-indexed search. The backend index is filename and metadata only. The "Content" mode chip ships as **disabled with a Coming soon tooltip** to anchor the eventual feature in the UI without overpromising.
- AI auto-apply on keystroke. AI calls cost money and the user must explicitly opt in by pressing Enter or `⌘Enter`. Auto-apply applies to filename and regex only.
- Cross-restart persistence of dialog state (recent-searches history is persisted; transient dialog state is not).
- Sidebar entries for the search-results virtual volume. It's reached programmatically (via "Open in pane") and via pane history, not via a clickable sidebar item.
- Windows-specific path handling in the path-pill splitter (macOS and Linux are the only ship targets right now).

## 2. Design principles that govern every decision below

Read these alongside `docs/design-principles.md` and `docs/design-system.md`. These resolutions reconcile the LLM-suggested mockup with Cmdr's existing system. Every milestone enforces them; reviewers will reject work that violates them.

1. **Accent is dynamic, not green.** The mockup is green because the renderer picked one accent. Cmdr's accent comes from `NSColor.controlAccentColor()` and lives in `--color-accent`. Use it. No hard-coded greens. Hover and subtle variants via `color-mix()`, never `brightness()`.
2. **Native, not web.** Small radii (4 to 8 px max). System font. Native scrollbars. No 1.05 button-scale hover gimmicks. macOS-style frosted glass via `backdrop-filter` is welcome where it earns its place (search bar, chip popovers).
3. **Borders only where they earn it.** Replace roughly 60% of the current dialog's borders with tonal-surface separation and spacing. The dialog should hold together without 1 px lines except where two semantically different regions actually need a divider.
4. **Keyboard-first, mouse-second.** Every chip, popover, footer chip, and action is reachable by keyboard. New shortcuts in this dialog are **hard-coded** (not in Settings > Shortcuts), documented in the dialog's `CLAUDE.md`, and surfaced in tooltips.
5. **Platform-native wording.** macOS: "Finder", Spotlight-style metaphors are fine. Linux: "file manager". Use `isMacOS()` to branch user-facing strings. No "Open in your OS file manager"-style watered-down phrasing.
6. **Radical transparency.** When the AI runs, the user sees the original prompt, the caveat, and which fields the AI populated. The mockup's "magical" abstractness is wrong for Cmdr; we earn trust by exposing what's happening.
7. **Don't block, don't lose.** Auto-apply is debounced; every long operation is cancelable; closing the dialog is non-destructive (state preserved).
8. **Friendly, informal copy.** Sentence case in every label, button, tooltip. No "Please" or "Sorry". Active voice. No "just", "simple", or "easy". Oxford commas. en-dashes for ranges, no em-dashes anywhere.

## 3. Architecture decisions

Each decision below has an **Intent** (why) and a **Mechanism** (how). Implementation agents must keep the why intact while adapting the how.

### 3.1. Unified search bar with mode chips

**Intent.** Today, the AI prompt and the filename pattern occupy separate rows. That makes them feel like different features competing for the same job. They're not: they're two ways to ask the same question. Collapse to **one input** with a chip row underneath showing the active mode. The chip row sits below the bar (not inside it), close enough to read as part of the same control.

**Mechanism.**

- One `<input>` for the typed query.
- Mode chips (left to right): `AI` badge plus the label **"Ask anything"** (default when `ai.provider !== 'off'`), **"Filename"**, **"Content"** (disabled, see §3.1.1 for shortcut handling), **"Regex"**. The active chip shows the dynamic accent at low saturation; inactive chips are neutral.
- Switching mode preserves the typed query.
- Keyboard:
  - `Tab` from the input moves focus to the active chip; arrow keys move between chips; `Enter` or `Space` activates the focused chip.
  - `⌘1` switches to AI (Ask anything). `⌘2` switches to Filename. `⌘3` switches to Regex. **`⌘4` is reserved; do not wire it now.** Content shipping later will claim `⌘3` and Regex moves to `⌘4`.
  - `Enter` in the input runs the search in the active mode.
  - `⌘Enter` runs an AI search regardless of active mode (only if AI is enabled).
- When AI is off, the AI chip is hidden (not just disabled). Filename becomes default, `⌘1` becomes Filename, `⌘2` becomes Regex.
- Exact chip strings: `Ask anything` (with an `AI` badge before it inside the chip), `Filename`, `Content` (with a small "Coming soon" tooltip), `Regex`. No mixed casing inside the chip text.

#### 3.1.1. Why we're not wiring a shortcut for the disabled chip

Assigning `⌘3` to a disabled control is hostile: either it silently no-ops or it pops a "Coming soon" toast on every press. Reserve the slot. Document the renumber-on-ship intent in the dialog's `CLAUDE.md`.

### 3.2. Filter chips with popovers

**Intent.** The current filter row is form-shaped (label plus select plus inline value). It works but it's noisy. The mockup's chip-with-popover model reads as calmer and is easier to extend. Users only care about a filter when they care; a filled chip ("Size > 100 MB ×") communicates "this is on" with the close affordance baked in.

**Mechanism.**

- Filters: **Size**, **Modified**, **Search in** (the existing scope row, folded in here). The scope chip's popover holds the path list textarea, the system-dirs filter toggle, and a footer with "Use current folder (⌥F)" and "All folders (⌥D)". `⌥F` and `⌥D` shortcuts still work globally inside the dialog regardless of popover state.
- A trailing **+ Add filter** chip opens a small menu listing available filters. We only have three (Size, Modified, Search in) right now. The chip is there because it's the affordance the user reads as "I can add filters" and gives the system a place to grow. Filters already shown are absent from the menu.
- A chip in its default ("any") state shows the filter name only, no value: `Size`, `Modified`, `Search in`. A configured chip shows the value: `Size > 100 MB ×`. Clicking ×, or pressing Backspace on a focused configured chip, returns it to default.
- Keyboard: Tab cycles through chips; Enter or Space opens the popover; Esc closes the popover without changes; Enter inside the popover confirms.
- Popovers use `--shadow-md`, `--radius-md`, the existing tooltip's frosted-glass material, and `--spacing-sm` (8 px) padding. They auto-flip away from viewport edges.

### 3.3. AI transparency strip

**Intent.** When an AI search has just run, the user needs to see (a) what they asked, (b) the AI's caveat if any, and (c) a clear place where they can eventually "talk back" to the agent ("don't include `node_modules`"). The current caveat is a one-line strip that disappears as soon as the user changes anything. That's not enough.

**Mechanism.**

- A small strip sits between the search bar and the filter chips, visible only after an AI search has run this session.
- Shows: the original prompt in `--color-text-secondary`, the caveat below it in `--color-text-tertiary` if present, and a disabled "Refine…" button at the right end with a "Coming soon: chat back to the agent" tooltip. Consistent with the Content mode chip: both are visible-but-disabled controls with a tooltip; neither has a keyboard shortcut wired. Hostile-disabled controls are the ones with shortcuts that activate silently; visible disabled controls with explanatory tooltips are fine and tell the user "this exists, just not yet."
- The applied filters are also briefly highlighted on the relevant chips (we already do this; keep the `highlightedFields` mechanism, just plumb it through the new chips).
- The strip persists until the user clears the search (`⌘N`, see §3.10) or runs a non-AI search. Surviving plain auto-applies in AI mode is irrelevant because AI mode doesn't auto-apply (§3.6).

### 3.4. Empty state with example queries

**Intent.** On open with no query, the results area is dead space. Use it. Show three real, working queries that anchor what's possible and where the system is good. Discoverability without a tutorial.

**Mechanism.**

- When `!hasSearched && query === ''` and the index is ready, the results area shows a centered block. When the index is NOT ready, the existing "Drive index not ready" message takes precedence; the "Try…" block is hidden until the index emits the `search-index-ready` event.
  - A small "Try…" line in `--color-text-tertiary`.
  - Three chips. Clicking a chip fills the bar with the query in the matching mode and auto-runs it (AI chips still respect the rule that AI requires explicit user action: the click counts as one).
  - When AI is on: three AI prompts validated against the eval catalog. Provisional set: **"large files modified this week"**, **"screenshots"**, **"PDFs from the last 7 days"**. The third uses "last 7 days" instead of "last week" because "last week" is itself ambiguous (rolling 7-day vs previous calendar week). Both rejected formulations point at the same lesson: example queries must be unambiguous in natural English so the AI's interpretation is reproducible. The set is locked in by adding it to `docs/notes/ai-search-eval-history.md` as canonical inputs, so any future change has to update both places.
  - When AI is off: three filename patterns: **`*.pdf`**, **`*.dmg`**, **`screenshot*`**.
- Below the chips, two muted lines:
  - "Index ready. 10.1M entries" (number formatted via `formatNumber()`).
  - "Tip: `⌘K` focuses search, `⌘N` starts fresh, `⌘H` shows recent searches."

### 3.5. Recent searches footer

**Intent.** Searches done are searches reusable. Two access patterns: the half-dozen most recent (always visible chip strip), and "look up something from a while ago" (a searchable popover behind `⌘H`).

**Mechanism.**

Storage:

- New backend module `apps/desktop/src-tauri/src/search/history.rs`. Pattern mirrors `known_shares.rs` and `manual_servers.rs`: in-memory `Mutex<HistoryStore>` plus atomic JSON write via the same temp-and-rename helper. **No `.await` while holding the `MutexGuard`** (standard rule for `std::sync::Mutex`; spawn-blocking the disk write or release the guard before awaiting).
- File: `{app_data_dir}/search-history.json`. Schema versioned (`_schemaVersion: 1`). camelCase fields.
- Entry shape:
  ```
  {
    id: string (uuid),
    timestamp: number (unix ms),
    mode: 'ai' | 'filename' | 'regex',
    query: string,
    filters: {
      sizeMin?: number, sizeMax?: number,
      modifiedAfter?: string, modifiedBefore?: string
    },
    scope: string,                  // raw chip text
    caseSensitive: boolean,
    excludeSystemDirs: boolean,
    resultCount: number
  }
  ```
- **Canonical dedupe key.** Build a deterministic string: lowercase the mode, normalize the query (trim, collapse internal whitespace), stringify filters with keys in alphabetical order (omit undefined fields entirely), stringify booleans as `t`/`f`, join with `|`. Example: `ai|large files modified this week||t|f`. Two entries with identical keys collapse to most-recent-wins (move-to-top). The key never appears in the JSON; it's purely a runtime comparison.
- Default cap **1,000 entries**, configurable via `search.recentSearches.maxCount` in Advanced (registry: `showInAdvanced: true`, type `number`, range 0 to 10,000, `component: 'number-input'`, suffix "entries"). Value `0` disables history entirely and clears the in-memory list at next call; the row's hint text reads "0 disables history".
- **A search is added to history only when the user clicks "Open in pane".** Auto-applies and Enter-runs do not add. History is the curated set of things the user cared about enough to act on. This is David's explicit design choice: it keeps the 1,000-entry budget signal-rich rather than noisy. Implementing agents must not "fix" this unilaterally, even if a narrower view would suggest "also add on Enter."
- A separate **in-memory "last attempt"** slot keeps the last query, filters, and result snapshot regardless of whether it was opened in pane. This is the source of state-preservation across dialog close/reopen (see §3.10).

Frontend:

- New IPC: `get_recent_searches(limit?)`, `add_recent_search(entry)`, `remove_recent_search(id)`, `clear_recent_searches()`. Typed via specta. Bindings regenerated via `pnpm bindings:regen`.
- Footer in the dialog: a row of up to 6 chips with the most recent entries' query text (mode-iconified via a small leading badge: `AI`, `Aa`, `.*`). Tooltip shows mode, filters summary, and age ("3h ago").
- Clicking a chip loads the entry into the bar and runs it. Right-click context menu: "Remove from history".
- A trailing "All searches…" chip opens an in-dialog popover (`⌘H` also opens it) with fuzzy search over the full 1,000. Same pattern as the command palette (`@leeoniya/ufuzzy`, already a project dependency via `lib/commands/fuzzy-search.ts`). Selecting an item loads it.
- **Positioning**: the popover is a **sub-overlay** on top of the search-dialog overlay (both are fixed-positioned). It uses the same anchoring primitive as the filter-chip popovers (`FilterChipPopover.svelte`), inherits the auto-flip behavior, and has its own focus trap (Tab cycles within the popover; Esc closes it without affecting the parent dialog). The search dialog's existing capture-phase Escape handler is checked: when the popover is open, Esc closes only the popover; the dialog stays open. Document this in the dialog's CLAUDE.md.

### 3.6. Auto-apply

**Intent.** The unified search bar should feel like Spotlight: results land as you think. AI calls cost money and on a 10M-entry index even a fast filename scan deserves not to fire on every keystroke. We split the rule by mode.

**Mechanism.**

- One setting: `search.autoApply` (boolean, default **true**), registered in a **new Settings > Search section**. Live-applied via `settings-applier.ts`.
- Applies to **filename and regex modes only**. AI mode always requires explicit Enter, `⌘Enter`, or click on the inline `⏎` run button.
- Debounce 1,000 ms (was 200 ms). Constant `SEARCH_AUTO_APPLY_DEBOUNCE_MS` exported from `search-state.svelte.ts` and used by all auto-apply callsites.
- **IME composition guard.** Auto-apply does not fire while an `input` event is happening inside an active IME composition (`compositionstart` without a matching `compositionend`). The debounce timer is reset on `compositionend` so the user gets one fire after composition completes, not multiple fires mid-composition. **This is net-new code** (no existing composition handling in `lib/search/`); implementing agent wires `oncompositionstart` and `oncompositionend` on the search bar input. Non-negotiable for Chinese, Japanese, and Korean input.
- When auto-apply is off (any mode), the user runs searches with Enter or the small ⏎ button on the right of the bar. The bar shows a subtle "Press Enter to search" hint in the right gutter when the query has changed since the last run.
- A small `⏎` run button is always visible on the right end of the bar; clicking it is equivalent to Enter.

### 3.7. Open in pane (search-results virtual volume)

**Intent.** A search-results pane needs to behave like a real pane (selection, keyboard nav, copy/move source, history, Quick Look) without lying about being a filesystem. The cleanest model in our codebase today is the `network` virtual volume: a `volumeId` the FilePane special-cases to render a different view, no fake filesystem implementation underneath.

**Mechanism.**

Volume identity:

- New synthetic `volumeId = 'search-results'`. Like `'network'`, it's a string we special-case in FilePane and the navigation layer.
- Path encoding within the pane: `search-results://<snapshot-id>` where `<snapshot-id>` is a monotonic per-session counter (`'sr-1'`, `'sr-2'`, …). The "path" is opaque; only the dialog and the pane internals interpret it.
- The breadcrumb shows a friendly label derived from the snapshot's query: AI mode shows the original prompt truncated to ~40 chars; filename mode shows the pattern (`*.pdf`) with a "+filters" suffix if any; regex mode shows `/pattern/`.

Snapshot storage:

- Frontend-only in-memory map: `snapshotStore: Map<snapshotId, SearchSnapshot & { refCount: number }>`. Lives in a new `apps/desktop/src/lib/search/snapshot-store.svelte.ts`.
- A snapshot holds: `{ id, query, mode, filters, scope, entries (capped at 10,000), totalCount, createdAt, label }`. Capping rationale: a session full of "Open in pane" actions with 10K entries each shouldn't bloat memory. If the search has more than 10,000 matches, we keep 10,000 and the label reads `Search: … (first 10,000 of N)`. The truncated breadcrumb uses the standard mid-truncation (`useShortenMiddle` action with `preferBreakAt: '.'`). **Accepted limitation**: the user cannot reach matches beyond the first 10,000 from within the pane; they must refine the query in the dialog and re-run. Documented in the dialog's `CLAUDE.md` under "Gotchas".
- **Eviction by refcounting.** Each pane history entry whose `path` starts with `search-results://` increments the snapshot's `refCount` on push. Decrements happen in exactly these cases:
  1. `push()` truncates forward history (entries after the current index are discarded on a new push). The returned `droppedEntries` array drives decrements.
  2. Per-tab history is evicted by the new cap (see "Pane history cap" below). Same `droppedEntries` mechanism.
  3. A tab is closed: every history entry in that tab's stack decrements its target. **Caveat**: closed-tab reopen (`⌘⇧T`) restores the tab from a `ClosedTab` snapshot in `closedStack` (default cap 10). Snapshots referenced by closed-tab stack entries must stay alive too. Implementation: when a tab is closed, instead of immediately decrementing, transfer the refs from "live tab" to "closed-tab stack". When the closed-tab stack evicts an entry (cap overflow, manual clear), THEN decrement those refs.
  4. A pane is recreated (`{#key}` swap on tab switch): no-op for refs. History is owned by `TabState` (`tab-types.ts` line 19: `history: NavigationHistory`), not by the pane. Confirmed by spot-check, not a future TODO.

  In addition to history refs, the "last dialog attempt" slot (§3.10) holds a +1 strong ref that swaps to the new snapshot id on each new attempt and decrements the old one. The dialog's currently-displayed snapshot (while open) also holds a +1 ref.

  A snapshot with `refCount === 0` is deleted from the store. There is no "snapshot expired" pane: by construction, while any pane history entry points at a snapshot, the refcount is at least 1.
- **Pane history cap.** `lib/file-explorer/navigation/navigation-history.ts` is currently unbounded. M8a adds a per-tab cap (`MAX_HISTORY_PER_TAB = 100`) enforced in `push(history, entry)` (the entry-accepting primitive; `pushPath` is a thin delegate over it). When the stack grows past 100, the **oldest** entry is dropped. To keep `navigation-history.ts` pure (its existing contract: return new state, no side effects), `push()` returns `{ history, droppedEntries: HistoryEntry[] }`. Callers (the tab-state manager, where the history actually lives, confirmed: `history: NavigationHistory` is a field on `TabState`) inspect `droppedEntries` and, for each entry whose `path` starts with `search-results://`, call `snapshotStore.decrementRef(snapshotId)`. The cap applies to all volumes, not just search-results; this is a small, contained behavior change for the whole nav layer and must land with a Vitest covering it.
- **Backstop sanity check.** No hard cap on the snapshot store itself: refcount is the only authority. If a snapshot's refcount stays > 0 forever (a bug in some other code), it leaks; the store exposes a dev-only `getDebugStats()` returning `{ count, totalEntries, maxRefCount }` so we can spot leaks in tests and during real-app inspection.

Pane integration:

- `FilePane.svelte` gets a new `isSearchResultsView = volumeId === 'search-results'` derived, with the same shape as the existing `isNetworkView` branches.
- A new `SearchResultsView.svelte` is the equivalent of `NetworkMountView.svelte`: it renders the file list from the snapshot.
- We **reuse `FullList.svelte`**. Add a new boolean prop **`showPathColumn?: boolean`** (one consumer for now; cleaner than `extraColumns: ('path')[]`'s premature extensibility). When set, the column-widths computation and grid-template include a Path column between Name and Size. Same row template, same selection/keyboard/sort plumbing. This is the most invasive code change in M8; TDD it.
- **Capabilities are flags read by pane code, not scattered `if (isSearchResultsView)` blocks.** A new helper `searchResultsVolumeCapabilities()` returns `{ canPasteInto: false, canMkdir: false, canMkfile: false, canRename: false, isSourceOK: true }`. Pane code reads these flags. **Menu items and shortcuts that would invoke disabled actions are disabled at the source**, not silently swallowed at the action site. "Disabled is better than 'you did the wrong thing' toasts." (`docs/design-principles.md` says: extremely user-friendly; AGENTS.md says: the user is always in control.)
- **Tab creation while inside a search-results pane**: pressing `⌘T` opens a new tab whose volume defaults to the previously-active one (typically the user's home), NOT another search-results snapshot. The new-tab-on-pinned behavior (which opens a new tab automatically when navigating away from a pinned tab) is also unaffected by the capability flags: it operates at the navigation layer, not the pane capability layer, and the "new" tab simply receives the search-results URL like any other navigation target. Both paths are exercised in M8 tests.
- Source operations work: drag-out, copy/move source, delete (which deletes the underlying real files; confirmation always shown). After a successful delete, the deleted file is removed from this snapshot and from every other snapshot containing it.
- Navigation history: history entries are already `{ volumeId, path, networkHost? }`. Adding `'search-results'` as a volumeId works without schema changes. `isPathOnVolume(path, volumePath)` doesn't apply (search-results paths are opaque); we extend the explicit-virtual-volume branch already present in `DualPaneExplorer.svelte` (the `if (currentVolumeId === 'network')` block; line numbers drift, cite by branch).
- New-tab-on-pinned: rely on the existing logic. When the active tab is pinned, the navigation layer already opens a new tab. Verified in M8 via real-app test, not just code reading.
- Pressing Enter on a row in a search-results pane navigates into the real file or folder (the underlying path is in the snapshot entry). The pane pushes a new history entry for the real path; the search-results entry stays on the back stack. Pressing `⌘[` returns to the search-results entry and re-renders from the snapshot (which is still refcount-pinned). No re-query, no loading state. This is the user's "exit" from the search-results view back into the real filesystem with a way home.

Dialog behavior:

- "Open in pane" button at the bottom-right of the dialog. Clicking creates a snapshot, pushes the new "path" to the active pane's history, and closes the dialog. The dialog state remains preserved (§3.10) so reopening lands in the same place.

### 3.8. Path pills in the results

**Intent.** Reading a path as a flat string is tiring. The mockup's path pills feel calmer because they reveal structure. We go further: they're clickable.

**Mechanism.**

- Replace the current `parentPath` string in the results row with a sequence of small pill spans, separated by `/` glyphs (`color-text-tertiary`, no padding, sits between pills). Pills wrap on narrow widths; the `/` glyph also acts as the visual seam.
- Each pill is a button; clicking navigates the active pane to that ancestor folder and closes the dialog. **Pills are mouse-click only and are NOT in the keyboard Tab order**, because making them tabbable inside virtualized rows breaks the row's arrow-down keyboard flow. The row's primary cell is the keyboard target.
- For keyboard users: `⌥←` (jump to parent of cursor row's path) and `⌥→` (descend back) operate on the cursor row's path. Documented in tooltips and `CLAUDE.md`.
- Pill chrome: `--radius-sm` (4 px), `var(--spacing-xxs) var(--spacing-xs)` padding, `--font-size-xs`, no border by default. Hover: `--color-bg-tertiary` background. Mouse focus ring uses the standard 2-layer ring; keyboard focus ring is irrelevant because pills aren't in Tab order.
- **macOS and Linux only.** Paths split on `/`. No `\` handling (Windows is out of scope right now).
- In `search-results` panes (where the same data is the *content* of the pane), the path appears in the Path column of FullList using the same pill rendering and click behavior. The "click navigates and closes dialog" rule becomes "click navigates the same pane and replaces the current snapshot view" inside the search-results pane.

### 3.9. Per-row actions and footer chips

**Intent.** Mockup's `…` per row and "Open in Finder" footer chip are the small affordances that signal "this is an active surface, not just a list".

**Mechanism.**

- Right-click on a row opens the standard file-list context menu (reuses the existing context-menu factory). Entries: Open, Reveal in Finder (Linux: Open in file manager), Copy path, Copy name. Disabled appropriately for directories vs files.
- A `…` icon button on the cursor row (only the cursor row; revealed on hover for non-cursor rows) opens the same menu.
- Footer (left edge): a horizontal scroll of up to 6 recent-search chips (§3.5) plus "All searches… `⌘H`".
- Footer (right edge):
  - **Open in pane**: primary action button. Visible whenever `results.length > 0`.
  - **Open in Finder** (Linux: **Open in file manager**): opens the parent folder of the cursor row in the platform's file manager. Visible whenever `results.length > 0` (a cursor row always exists when there are results, since `cursorIndex` defaults to 0 on every search). "Visibility" here means rendered presence in the DOM; "cursor row" means the row at `cursorIndex`, not the DOM-`document.activeElement`.
- Both right-edge buttons are hidden (not just disabled) on empty/idle state because they have nothing to act on.

### 3.10. Dialog state preservation and `⌘N` to clear

**Intent.** Closing the dialog after picking a result and reopening should pick up exactly where the user left off. Losing the search to see one file's content is hostile UX. Recreating it manually is worse. The reset path is explicit (`⌘N`) so it's discoverable and unambiguous.

**Mechanism.**

- Today, `SearchDialog.svelte`'s `onDestroy()` calls `resetSearchState()`. We replace that with **explicit reset paths**:
  - `⌘N` inside the dialog calls a new `clearSearchState()` that wipes query, filters, scope, results, cursor, and the AI transparency strip.
  - Why `⌘N`: it reads as "new search" the same way `⌘N` reads as "new tab" or "new document" elsewhere in macOS. The global `⌘N` (which opens a new tab via `routes/(main)/command-dispatch.ts`) doesn't fire while focus is inside the dialog because `SearchDialog.svelte`'s `handleKeyDown` calls `e.stopPropagation()` on every keydown (existing gotcha in `lib/search/CLAUDE.md`). The new `⌘N` handler lives in the same `handleKeyDown` switch, ahead of the existing modifier-shortcut block. No collision.
  - Close-and-reopen does not reset.
- The "last attempt" snapshot (separate from the persisted history; in-memory; see §3.5) holds the result entries so reopening shows them instantly without a re-query. We re-render from the snapshot, then optionally trigger a debounced refresh (if `search.autoApply` is on and the mode allows it).
- Scroll position and `cursorIndex` are also preserved in the state module (`search-state.svelte.ts`).
- The state-svelte module already lives at module scope, so Svelte unmount/remount of the dialog component does not destroy the state. Verified in M1's tests.
- Lost on app restart: in-memory only by design. Persisting transient dialog state to disk would break the "fresh start" expectation on app launch.
- On open, focus goes to the search bar regardless of preserved state.

### 3.11. MCP tool: `open_search_dialog`

**Intent.** Agents should be able to put a query in front of the user, not just run a search behind the scenes. Useful for "I think you're looking for these, confirm?" workflows.

**Mechanism.**

- New tool: `open_search_dialog`. Lives in the dialogs tool group (consistent with the existing `dialog` tool). Schema:
  ```
  {
    query?: string,
    mode?: 'ai' | 'filename' | 'regex',     // defaults: 'ai' if AI on, else 'filename'
    sizeMin?: number,                        // bytes
    sizeMax?: number,
    modifiedAfter?: string,                  // ISO date
    modifiedBefore?: string,
    scope?: string,                          // same syntax as the scope chip
    caseSensitive?: boolean,
    excludeSystemDirs?: boolean,
    autoRun?: boolean                        // default true: open and run; false: open and prefill only
  }
  ```
- Backend emits a Tauri event (`mcp-open-search-dialog`) carrying the prefill payload. The main window's `+page.svelte` listens, opens the dialog, and sets up the state via `search-state.svelte.ts` setters before `SearchDialog` mounts.
- Ack contract: waits for the dialog to actually mount. The frontend already calls `notifyDialogOpened('search')` in `SearchDialog.svelte` on mount, so the `SoftDialogTracker` updates. `AckSignal::SoftDialogAppeared(&'static str)` accepts any static string against the tracker (no separate allowlist; verified by reading `mcp/executor/ack.rs::wait_for_ack`), so we just pass `"search"`. Default 1,500 ms budget.
- Result payload: result count and the snapshot id of the last attempt (so a future tool can act on it).

## 4. Milestones

Sequential by default. Each milestone's exit criteria are non-negotiable; an agent is not done until every box is checked.

**Per-milestone exit criteria (apply to all):**

- All new code TDD-style where reasonable: tests written or updated alongside or before implementation.
- `./scripts/check.sh` (default lane) green. Full output read, no `head`, `tail`, or `2>&1` truncation.
- Colocated `CLAUDE.md` updated if architecture, decisions, or gotchas changed. This is part of the milestone, not a final cleanup.
- Real-app verification via the running dev app and MCP screenshots/DOM snapshot. The leader runs an "Inspect" sub-agent between milestones to confirm look and behavior.
- Self-review: "Is what I've done solid AND elegant? Am I proud of it?" If no, iterate.
- Commit message follows `.claude/rules/git-conventions.md`. No co-author. Amending the in-progress commit on the same milestone is allowed only when the conditions in `~/.claude/rules/amend-unpushed-commits.md` are met; otherwise new commit.

### M1: Visual foundation plus state-preservation scaffolding (~1 day)

Goal: replace the dialog shell. Get the look in place: fewer borders, glass tones, tighter spacing rhythm, taller search input, softer column headers. Functionality stays current. Add `clearSearchState()` and remove `resetSearchState()` from `onDestroy`. Wire `⌘N` to `clearSearchState`.

Width: bump to 1,080 px. Rationale: the chip rows (§3.2) plus path-pill column (§3.8) crowd the current 900 px. 1,080 fits comfortably on a 1,366-wide laptop (the narrowest common modern MacBook screen) with sufficient window-chrome margin. Internal layout is fluid so we can resize programmatically later; no fixed inner widths.

Touches: `SearchDialog.svelte`, `SearchInputArea.svelte`, `SearchResults.svelte`, `AiSearchRow.svelte` (still present in this milestone), the dialog's `CLAUDE.md`.

Tests:

- Vitest: `SearchDialog.a11y.test.ts` and `SearchInputArea.a11y.test.ts` updated to assert no a11y regressions.
- Update existing snapshot tests where they pin border counts.
- Add a small test that `⌘N` clears state and that close-then-reopen preserves state (mount/unmount the dialog twice with assertions in between).

Verification: launch the app, open dialog, eyeball it against the mockup. Take screenshots for the next milestones to compare against.

### M2: Unified search bar with mode chips (~1 day)

Goal: merge `AiSearchRow` and the pattern input into a single bar with mode chips. Behavior per §3.1. The AI prompt and filename pattern are now one field in `search-state.svelte.ts` (call it `query`), with `mode: 'ai' | 'filename' | 'regex'` carrying the discriminator. Delete `AiSearchRow.svelte` and its tests. Migrate `aiPrompt` and `namePattern` getter call sites to read `query` plus `mode`; no back-compat shims (per AGENTS.md's "no backwards-compatibility hacks").

Touches: `SearchDialog.svelte`, `search-state.svelte.ts`, new `SearchBar.svelte` and `SearchModeChips.svelte`, deletion of `AiSearchRow.svelte`. Update IPC param shaping in `executeSearch()` and `executeAiSearch()`. **Backend IPC**: `translate_search_query(natural_query: String)` already takes a free-text string; no Rust param rename. The `search_files(query: SearchQuery)` payload is unchanged on Rust; the frontend just builds it from the unified state. Bindings regen (`pnpm bindings:regen`) is unnecessary for M2 because no Rust signatures change. (Bindings regen IS needed for M5 when new history IPCs land.)

Tests:

- Unit tests for mode switching preserving query and updating placeholder.
- `⌘1`, `⌘2`, `⌘3` shortcuts, Tab/Arrow chip nav, Enter runs in active mode, `⌘Enter` runs AI regardless.
- Disabled state of Content chip plus tooltip presence. No shortcut wired to Content.
- AI-off branch: AI chip hidden, `⌘1` becomes Filename, `⌘2` becomes Regex.

Verification: full keyboard flow walked manually with MCP screenshots.

### M3: Filter chips with popovers (incl. scope) (~1.5 days)

Goal: replace the filter row and the scope row with a chip strip. Each chip opens a popover. `⌥F` and `⌥D` shortcuts still work globally inside the dialog.

Touches: delete the scope row markup in `SearchInputArea.svelte`, refactor (rename) the rest to `SearchFilterChips.svelte`. Add new `FilterChip.svelte` and `FilterChipPopover.svelte`. The "+ Add filter" chip opens a small menu.

Popover behavior: native-feeling, frosted, ESC closes, click-outside closes, focus trapped inside while open.

Tests:

- Unit tests for chip-state derivation (default → configured → cleared).
- Popover open/close keyboard handling.
- Scope popover behavior (paste paths, `!`-prefix exclusions, ⌥F sets to current folder, ⌥D clears).
- a11y audit via Vitest tier 3 (`*.a11y.test.ts` files using `@axe-core`).

Verification: keyboard walk via MCP screenshots.

### M4: AI transparency strip (~0.5 day)

Goal: ship the prompt plus caveat plus disabled "Refine…" strip under the search bar. Visible only after an AI search has run this session; cleared on `⌘N` or running a non-AI search.

Touches: new `AiTransparencyStrip.svelte`, wire from `SearchDialog.svelte`. Reuse the existing `aiStatus`, `caveat`, and `aiPrompt` state.

Tests:

- Strip appears after AI run; hides on `⌘N`; hides on filename/regex run.
- "Refine…" is disabled and tooltip present.

Verification: trigger an AI search via the OpenAI provider (see §5.2), confirm strip content matches.

### M5: Empty state plus recent searches (~2 days)

Backend first (TDD):

- Rust: `src-tauri/src/search/history.rs` with `HistoryStore`, atomic JSON read/write. Cap, dedupe, move-to-top, schema version. Rust tests cover: load/save round-trip, dedupe semantics with canonical key, cap eviction, corrupted-file recovery (rename to `.broken`, start fresh), schema-version mismatch handling.
- Tauri commands: `get_recent_searches`, `add_recent_search`, `remove_recent_search`, `clear_recent_searches`. Specta-typed. Wired through `tauri-commands/`.
- Setting `search.recentSearches.maxCount` registered in Advanced (number input, range 0 to 10,000, "0 disables history").

Frontend:

- `RecentSearchesFooter.svelte` (chip strip).
- `RecentSearchesPopover.svelte` (fuzzy search via `@leeoniya/ufuzzy`, list, keyboard nav).
- `EmptyState.svelte` (Try… chips, tip line).
- Wire `⌘H` to open the popover.
- Wire AI example queries against the eval catalog: add `"large files modified this week"`, `"screenshots"`, `"PDFs from the last 7 days"` to `docs/notes/ai-search-eval-history.md` as canonical inputs. Verify in dev with the OpenAI provider (see §5.2). If any prompt produces empty results in repeated runs, swap before commit and update both the catalog and the empty state.

Tests:

- Vitest: footer renders the latest 6, popover filters fuzzy-style, `⌘H` opens, Esc closes, ↑↓ + Enter selects.
- Rust: history dedupe with canonical key, cap eviction.

Verification: end-to-end flow: do a search, open in pane, reopen dialog, see it in footer. Click footer chip: bar refills and auto-runs (filename) or waits for Enter (AI). Popover lists 1,000+ entries fluidly.

### M6: Auto-apply plus Settings > Search section (~0.5 day)

Goal: live-applied auto-apply toggle. Add the Search settings section right under "Behavior > Drive indexing" in the sidebar order. The Search section also exposes `search.recentSearches.maxCount` as a mirror (per the settings system's mirror pattern in `lib/settings/CLAUDE.md`), so users hunting for it under "search" find it there too.

Touches: `settings-registry.ts` (new entries `search.autoApply` and `search.recentSearches.maxCount`), new `apps/desktop/src/lib/settings/sections/SearchSection.svelte`, sidebar order update in `SettingsSidebar.svelte` and the E2E test `settings.spec.ts`, applier hook if needed.

Tests:

- Settings registry test: new entries present, defaults correct, search keywords cover the right terms.
- Live-apply: toggling the setting changes search-dialog behavior without restart.

### M7: Results table polish (path pills, row `…` menu, Open in Finder) (~1 day)

Goal: replace the row's `.result-path` plain text with `PathPills.svelte`; add the `…` action button on the cursor row; add the right-edge footer chips per §3.9.

Touches: `SearchResults.svelte` (path-pills and per-row menu wiring), new `PathPills.svelte`, new `SearchFooterActions.svelte`. Context menu reuses the existing factory in `apps/desktop/src/lib/file-explorer/` (verify exact import during impl).

Tests:

- `PathPills.svelte.test.ts`: segment splitting (`/` only), `⌥←` / `⌥→` keyboard nav, click navigates parent.
- a11y test verifying pills are NOT in tab order (the design's deliberate choice from §3.8).

Verification: MCP-driven walk: focus a row, press `⌥←`, dialog closes, pane navigates to ancestor.

### M8: Open in pane (search-results virtual volume) (~4 to 5 days, biggest)

This is the riskiest milestone. The same agent executes it in three sub-milestones in sequence; the leader inspects between them. The size estimate above reflects the actual surface: `FullList` plus `measure-column-widths.ts` is intricate canvas-based logic (prefix sums, date split, transitions) and adding a Path column with pill rendering is a day on its own.

#### M8a: Snapshot store plus nav history cap

- `apps/desktop/src/lib/search/snapshot-store.svelte.ts`: snapshot map with refcounting per §3.7. Public API: `getOrCreate(id, snapshot)`, `getRefCount(id)`, `incrementRef(id)`, `decrementRef(id)`, `getSnapshot(id)`, `getDebugStats()`. Internal: map plus a small WeakRef hook to the "last dialog attempt" slot.
- Add `MAX_HISTORY_PER_TAB = 100` cap to `navigation-history.ts::push(history, entry)`. Oldest-entry eviction. Change the return type to `{ history, droppedEntries: HistoryEntry[] }` to preserve the module's pure-function contract. `pushPath` delegates to `push` and forwards the result. Vitest tests in `navigation-history.test.ts` covering cap eviction across all volume types (not just search-results), plus the `pushPath` delegate test contract.
- The tab-state manager (the caller; lives in `tabs/`) inspects `droppedEntries` and decrements snapshot refs for any `search-results://` paths. **Do NOT inject a callback into `navigation-history.ts`**; keep it pure. Search-specific knowledge lives in the caller.
- Tab close: instead of immediate decrement, transfer refs from the closed tab's history to the closed-tab stack entry. Tab reopen restores them. Closed-tab-stack eviction (cap overflow, manual clear) is the actual decrement point. The closed-tab stack and the `closeTabRecording` / `reopenLastClosedTab` logic live in `apps/desktop/src/lib/file-explorer/tabs/tab-state-manager.svelte.ts` (not `pane/tab-operations.ts`, which is a different file in `pane/`). Add a small helper `transferSnapshotRefs(closedTab, action: 'transfer' | 'release')` next to the closed-stack logic.
- Dev-only `getDebugStats()` returns `{ count, totalEntries, maxRefCount, idsWithRefCount }`. Accessible via the existing debug page or directly via DevTools.

#### M8b: Virtual volume plus pane shell plus FullList path column

- FilePane: `isSearchResultsView` branches mirroring `isNetworkView`. New `SearchResultsView.svelte` rendering the snapshot's entries via `FullList`.
- Add `showPathColumn?: boolean` to `FullList.svelte`. Update `measure-column-widths.ts` to compute the Path column width using `@chenglou/pretext` (canvas measurement, already a project dependency at `@chenglou/pretext@^0.0.6`, accessed via `lib/utils/shorten-middle.ts::createPretextMeasure(font, pretext)`). Path-pill rendering inside the column cell.
- Update grid-template-columns formula; verify date-split column still works when Path is added.
- Document the new prop in the FullList `CLAUDE.md`. Add a regression test for the existing (path-column-off) layout to catch unintended regressions.
- Navigation: extend the explicit virtual-volume branch in `DualPaneExplorer.svelte` (cite by `if (currentVolumeId === 'network')` branch, not line number).
- Tab title and breadcrumb show the snapshot's friendly label.
- Dialog to pane handoff: the dialog directly invokes pane navigation by calling the explorer's `navigateToVolumePath('search-results', 'search-results://<id>')` after storing the snapshot. No new Tauri command needed (snapshots are frontend-only).
- New-tab-on-pinned: confirm via real-app test (pin a tab, "Open in pane" should open the snapshot in a NEW tab, not replace the pinned one). The existing navigation logic handles this; we verify, not implement.

#### M8c: Capability flags plus per-row actions in the pane

- `searchResultsVolumeCapabilities()` returns the flag set from §3.7. Refactor scattered checks to read it.
- **Disable the offending menu items and keyboard shortcuts at the source.** F-key bar entries (F7 New folder, etc.) read the capability flags and render disabled. Paste shortcut `⌘V` ignored at the pane level if `canPasteInto` is false. No "toast on action attempt"; user-friendliness via disabled affordances.
- Drag-out works (the underlying paths are real).
- Delete from search-results pane: confirms with the real path. On success, the row is removed from this snapshot AND from any other snapshot it appears in.
- Cross-pane copy/move: works as source.
- Test that `⌘[` and `⌘]` navigate in/out of the search-results entry correctly.

Tests:

- Vitest: snapshot store cap and eviction-by-reference invariants (referenced ids never dropped).
- Vitest: capability flags drive menu disabled state.
- Vitest: FullList rendering with `showPathColumn: true` width-computes correctly and the existing pane (with prop unset) is unaffected.
- Playwright E2E: open dialog → run search → "Open in pane" → cursor present, navigate back with `⌘[`, reach a real folder, `⌘]` returns to snapshot. Spec name: `search-open-in-pane.spec.ts`. Designed to run under 1 s.

Verification: real-app walk with multiple snapshots in history. Eviction inspected by reading `snapshotStore`'s size after deliberate over-fill via MCP.

### M9: MCP `open_search_dialog` (~0.5 day)

Touches: `src-tauri/src/mcp/tools.rs` (tool registration), `src-tauri/src/mcp/executor/search.rs` (new execute function), `src-tauri/src/mcp/CLAUDE.md` (tool catalog update). Ack signal: `SoftDialogAppeared("search")` (add `"search"` to the static dialog-id set if not already there).

Tests:

- MCP protocol test: tool is registered, schema is valid.
- Integration test in `mcp/tests/`: invoking the tool emits the right event and the FE consumes it.

Verification: real MCP call via `curl` against `localhost:19225`. Confirm dialog opens, query is pre-filled, results render.

### M10: Final polish, slow lane, docs audit, handoff (~1 day)

- Run the full `./scripts/check.sh --include-slow`. Address anything related; ignore unrelated (with a one-line note in the commit explaining each ignored failure).
- Read every `CLAUDE.md` touched by this redesign and confirm it accurately describes the new world. Sweep across the project for any doc that referenced the old `AiSearchRow`, scope row, 900 px width, etc., and update.
- Update `docs/architecture.md` if the search row in the §Frontend table needs new wording.
- Heavy updates to `apps/desktop/src/lib/search/CLAUDE.md`: new component map, the unified-bar pattern, snapshot store, virtual-volume integration, recent-searches storage shape, settings, MCP tool.
- Add a `Decision/Why` block in `apps/desktop/src/lib/search/CLAUDE.md` summarizing:
  - Why the unified bar instead of two rows.
  - Why filter chips with popovers instead of inline.
  - Why search-results is a virtual volume rather than a special FilePane mode (precedent: network).
  - Why history is added only on "Open in pane".
  - Why AI mode doesn't auto-apply.
- Self-review.

## 5. Cross-cutting

### 5.1. Testing strategy

- **Vitest unit and a11y tests** at every new component (tier 3 of the three-tier strategy). Colocated. Fast.
- **Rust unit tests** for `search/history.rs` (atomic write, dedupe, cap, recovery, schema version).
- **Playwright E2E**, search-scoped:
  - `search-dialog-open-close.spec.ts`: open, type, run, close. Under 1 s.
  - `search-modes.spec.ts`: switch modes with keyboard, query preserved. Under 1 s.
  - `search-filters.spec.ts`: chip plus popover plus Esc plus Enter. Under 1 s.
  - `search-recent.spec.ts`: Open in pane → footer chip → re-run. Under 1 s.
  - `search-open-in-pane.spec.ts`: M8 acceptance. Aim under 1 s; 1.5 s acceptable given pane-mount cost.
  - `search-ai-prompt.spec.ts`: AI mode visible state with provider mocked. Under 1 s.
- Reuse the existing Playwright fixtures for index readiness (`index-ready` event).
- During development, run only the relevant spec, never the full suite per cycle. Run the whole search-spec set at end of each milestone.
- The full check suite (`./scripts/check.sh`, default lane) runs after every milestone. Slow lane only at M10.

### 5.2. AI testing via OpenAI

David has $2,500 of OpenAI credits expiring soon; we use them generously.

- Provider: `cloud`, OpenAI default base URL. Model: David specified `gpt-5.5`. **Sanity-check the curl recipe below works before depending on it in M5's eval set.** Run it as a one-shot at the start of M5 to confirm the model accepts the call. **If the model name fails at runtime** (the cloud provider's `/models` endpoint will surface it), the implementing agent escalates rather than substituting a different model silently. The provider's connection-check flow already shows clear feedback for invalid models.
- API key: pulled from Keychain via the existing `getAiApiKey` flow. The setup curl recipe:
  ```
  curl -s https://api.openai.com/v1/chat/completions \
    -H "Authorization: Bearer $(security find-generic-password -s OPENAI_API_KEY -a veszelovszki -w)" \
    -H "Content-Type: application/json" \
    -d '{"model":"gpt-5.5","messages":[{"role":"user","content":"Say hi"}],"max_completion_tokens":50}'
  ```
  Use this directly in agent shell calls for sanity-checking the AI pipeline.
- Validate every AI-touching feature against a real call: the AI transparency strip, the example queries' result quality, the AI search regressions catalog.
- The eval catalog at `docs/notes/ai-search-eval-history.md` is the regression bed. Run it before merging M5 and M10. New eval entries for `"large files modified this week"`, `"screenshots"`, `"PDFs from the last 7 days"`.

### 5.3. Real-app verification (MCP)

The MCP servers don't auto-connect; agents call via CLI (`curl localhost:19225/mcp` per `docs/tooling/mcp.md`). Between every milestone, the leader spawns an "Inspect" sub-agent whose job is **only**: launch the dev app (`pnpm dev` if not running), drive it via MCP, take screenshots, verify the new behavior, return findings. The Inspect agent doesn't modify code. This gives us a clean feedback loop without polluting implementation agents' context.

### 5.4. Docs maintenance (per-milestone, not a final step)

Every milestone updates the colocated `CLAUDE.md` it touches. The implementation agent does this **as part of the milestone**, not at the end. The final M10 docs audit is a safety net, not the primary mechanism.

Watch these files specifically:

- `apps/desktop/src/lib/search/CLAUDE.md`: heavy updates across many milestones.
- `apps/desktop/src/lib/file-explorer/CLAUDE.md`: virtual-volume pattern note for search-results.
- `apps/desktop/src/lib/file-explorer/views/CLAUDE.md`: the `showPathColumn` prop.
- `apps/desktop/src-tauri/src/search/CLAUDE.md`: history module.
- `apps/desktop/src-tauri/src/mcp/CLAUDE.md`: new tool entry, including any new `AckSignal::SoftDialogAppeared("search")` plumbing.
- `apps/desktop/src/lib/settings/CLAUDE.md`: new Search section, plus the mirror entry.
- `docs/architecture.md`: only if the §Search summary or the frontend table needs touching.

### 5.5. Parallelism

No parallelism. Sequential agents. The leader's context window is the scarce resource; parallel worktrees buy hours at the cost of coordination errors. The exception: the Inspect sub-agent (read-only) can run between any two milestones.

### 5.6. Style and copy

- All strings sentence case. Audit candidate strings at implementation time: "Ask anything", "Add filter", "All searches…", "Open in pane".
- macOS branches: "Open in Finder", "Reveal in Finder". Linux branches: "Open in file manager", "Open file manager".
- "Open in pane", not "Show in pane" (active, user-initiated).
- "Search in" (the scope chip), not "Scope" (jargon).
- "All searches…" with the ellipsis, signals more-behind-it.
- No emoji in UI strings.
- Numbers: 1–9 spelled out, 10+ as numerals, thousands separators in displayed numbers via `formatNumber()` (locale-aware: `1,000` in `en-US`, `1 000` in `sv-SE`). Hard-coded numbers in code constants don't need formatting; only user-facing strings.
- No em-dashes anywhere (per `docs/style-guide.md`). Use colons, parentheses, or new sentences.

### 5.7. Risk register

- **`FullList` `showPathColumn` prop**: the most invasive change to a heavily-used file. Mitigate with TDD on widths and rendering; verify no regression in regular panes via screenshot comparison.
- **Snapshot store reference-tracking**: navigation history mutations are synchronous, snapshot reads from rendering can lag. The eviction runs *after* a history mutation. The invariant "referenced snapshots never evicted" is the load-bearing one; spec and test it explicitly.
- **`gpt-5.5` model name**: David provided the working curl. Trust it. If runtime rejects the model, the connection-check flow in `AiCloudSection` makes the failure visible; the agent escalates rather than guessing a substitute.
- **Recent-searches schema migration later**: ship `_schemaVersion: 1` and a `match` on schema version that triggers reset-with-rename to `.broken` on mismatch. Don't ship migrations now; plan for `migrate()` if v2 ever lands.
- **State preservation collides with Svelte unmounts**: `search-state.svelte.ts`'s `$state` lives in module scope already; closing/reopening the dialog component does not touch it. Verify in M1's tests.
- **Path-pill click in a search-results pane**: clicking a pill inside a row of a search-results pane navigates the pane to that real ancestor folder, leaving the snapshot view. This is correct per §3.8 but must be tested specifically because the rule changes context (dialog vs pane).
- **MCP `open_search_dialog` race with a closing dialog**: if the dialog is closing when the MCP call arrives, the new `notifyDialogOpened('search')` ack might never fire. Either queue the open until the close finishes, or have the executor return a clean "dialog busy, try again" error within the 1,500 ms budget. Prefer the latter (simpler) and document.
- **Snapshot reference iteration**: the refcounting model in §3.7 depends on every history-entry mutation site (push, truncate-forward, tab-close, tab-recreate) calling the snapshot-store hook. Missing a single call site leaks a snapshot (memory) or drops a referenced one (broken back-nav). Mitigation: M8a writes a single helper `decrementIfSearchResults(path)` and every mutation site calls it; a grep test in the check suite asserts no other code path mutates the history stack directly.
- **`{#key}` tab re-creation losing history refs**: `tabs/CLAUDE.md` says tab switch uses `{#key}` to recreate FilePane, but history lives in tab-state, not pane-state. M8a verifies this by reading `tab-operations.ts` carefully; if history actually does live on the pane, refcounting must move to tab-state lifecycle (a more invasive change).
- **Both panes showing the same snapshot, one tab closes**: tab close decrements once for each history entry in that closed tab. The snapshot stays alive while the other pane's history still references it. Refcounting handles this correctly by design; the risk is forgetting to test it explicitly. Add a Vitest case in M8.
- **Disabled `⌥←` / `⌥→` inside the AI transparency strip's "Refine…" disabled button**: when focus is inside the disabled button, what do `⌥←` / `⌥→` do? They should still route to the dialog's cursor-row pill navigation. Verify with a Vitest case.

### 5.8. What "done" looks like

- All 10 milestones merged into the worktree branch.
- `./scripts/check.sh --include-slow` green or only unrelated failures with documented reasoning.
- Search-spec Playwright suite green, under 1 s per spec average.
- Real-app walkthrough captured in screenshots: empty state, AI flow with transparency strip, filter chip with popover, recent-searches footer plus popover, search-results pane via "Open in pane", path-pill click navigates.
- Every touched `CLAUDE.md` reflects the new world.
- Leader's final self-review answers "solid AND elegant?" with yes.
- FF-merge worktree → main locally. Worktree deleted. No `git push` until David asks.

## 6. Out of scope (don't get nerd-sniped)

- Content-indexed search (the engine work).
- "Refine…" chat-back UX (the placeholder is the whole feature for now).
- Cross-restart persistence of dialog state.
- Sidebar entry for the search-results virtual volume.
- The Settings > Shortcuts integration for in-dialog shortcuts (they're hard-coded).
- Brief mode for search-results panes (Full mode only).
- Real-time updates of search results (the snapshot is a snapshot).
- Drag-and-drop reordering of recent searches.
- An "Open all results in pane" action distinct from "Open in pane" (they're the same thing).
- Windows-specific path handling.

End of plan. Review will follow.
