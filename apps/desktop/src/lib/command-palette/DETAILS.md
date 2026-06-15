# Command palette: details

Depth and rationale. `CLAUDE.md` holds the must-knows that prevent silent breakage; this file holds the data flow,
secondary patterns, and decision rationale.

## Data flow

```
User presses ⌘⇧P
  → +page.svelte sets showCommandPalette = true
  → CommandPalette mounts, calls pruneRecentCommands(validIds) (load + drop stale + save), focuses input
  → searchCommands(query, recentCommandIds) returns CommandMatch[] (reactive via $derived)
  → User navigates with ↑/↓ (keyboard cursor) or mouse (hover cursor)
  → Enter / click → pushRecentCommand(id), onExecute(commandId) → handleCommandExecute()
  → Escape / overlay click → onClose() called
```

## Patterns

- **Highlight intensities**: the CSS classes `is-under-cursor` and `is-hovered` apply different intensities
  (`--color-accent-subtle` vs. a subtle rgba overlay). Arrow keys clear `hoveredIndex` so two items are never
  highlighted at the same intensity.
- **Recents on empty query**: when the query is empty, recents lead the result (most-recent first) under a `Recent`
  subheader, with an `All commands` subheader before the rest. The pruned list comes from `pruneRecentCommands` /
  `pushRecentCommand` in `$lib/app-status-store` (a Tauri store).
- **Combobox semantics**: the input is a WAI-ARIA combobox (`role="combobox"`, `aria-controls="palette-listbox"`,
  `aria-expanded`, `aria-autocomplete="list"`, `aria-activedescendant`). Each option has a stable id
  (`palette-option-{command.id}`). The listbox isn't rendered when a search yields zero results (a "No commands found"
  message replaces it), so the `scrollable-region-focusable` rule has nothing to flag in the empty state and
  `aria-expanded` flips to `false`.
- **Fuzzy highlight rendering**: `highlightMatches()` converts the flat `matchedIndices` set into
  `{ text, highlighted }` segments and renders `<mark class="match-highlight">` for matched chars.
- **Shortcut chips**: each combo renders as a non-clickable, dense (`size="sm"`) `ShortcutChip`, capped at three. Chips
  are non-clickable because the row already owns the click. On the accent-subtle cursor row, the chip background lifts
  to `--color-bg-secondary` so it stays legible against the tint (both themes).
- **Stability badge**: rows whose command carries `status: 'alpha' | 'beta'` render a `StatusBadge` pill between the
  name and the shortcut chips. The registry derives the field from `getBadgeStatus()` in `$lib/feature-status`
  (repo-root `feature-status.json` is the single source of truth), so graduating a feature removes the badge with a JSON
  edit.

## Decisions and rationale

- **Own overlay, not the shared `ModalDialog`.** The palette's live fuzzy filtering, keyboard-first navigation, and
  two-cursor hover model don't fit `ModalDialog`'s confirm/cancel pattern, and its `stopPropagation`-on-all-keydown
  concern shouldn't leak into a shared component.
- **Two independent cursor models.** VS Code and Spotlight both move a "hard" keyboard cursor while mouse hover shows a
  "soft" highlight that doesn't interfere with keyboard navigation. Without the separation, moving the mouse would fight
  keyboard nav.
- **Combobox + `aria-activedescendant` instead of moving DOM focus.** This is the WAI-ARIA combobox-with-listbox
  pattern: typing must stay routed to the input while browsing options, so moving real focus to a row would break
  search-as-you-type. The cursor option's `tabindex="0"` (roving tabindex of one) satisfies axe's
  `scrollable-region-focusable`. GitHub's command bar and several production palettes ship the same combination.
- **Empty-query view shows recents (last 10) instead of persisting the last query.** Users reach for the same handful of
  commands. Persisting the query only helped the "run a related command" reopen case; recents covers that AND every
  other reopen (the last-executed command is at index 0, so Enter re-runs it), replacing two mechanisms with one.
- **`$derived` for search results, no debounce.** uFuzzy is fast enough for ~60 commands that debouncing would only add
  latency; the reactive binding reruns synchronously per keystroke, keeping results in sync with the input. Debouncing
  would matter only if the list grew to thousands.
- **Shortcut display caps at three and reads live effective bindings.** Some commands have many shortcuts (`nav.parent`
  has `Backspace` and `⌘↑`); showing all crowds the row. Three is the product-decided cap (power users use the palette
  to discover alternates). Reading effective bindings (not `command.shortcuts`) keeps an open palette in sync with a
  mid-session rebind and avoids showing a rebound combo that no longer works. The full list lives in shortcut settings.
  See decision (a) in `docs/specs/shortcut-display-unification-plan.md`.
