# Command palette

VS Code/Spotlight-style modal for searching and executing app commands via fuzzy matching.

## Module map

- **`CommandPalette.svelte`**: modal UI (keyboard nav, mouse hover, fuzzy-highlighted results, recents on empty query).
- **`CommandPalette.test.ts`** / **`CommandPalette.a11y.test.ts`**: Vitest tests with mocked `$lib/commands` and
  `$lib/app-status-store`.

`+page.svelte` sets `showCommandPalette = true` on ⌘⇧P; the palette mounts, prunes recents, searches reactively, and
calls `onExecute(commandId)` → `handleCommandExecute()` on Enter / click.

## Must-knows (invariants and guardrails)

- **`stopPropagation()` runs on every `keydown`, not just handled keys.** Without it, unhandled keys (letters, numbers)
  propagate to the file explorer behind the modal and trigger quick-search or other handlers.
- **Focus containment has two layers; don't remove either.** (1) `handleKeyDown` swallows Tab (`preventDefault`): the
  palette is a combobox, so DOM focus must stay on the input. Without the swallow, two Tab presses walk focus into the
  blurred background where the suppressed global dispatch leaves Esc / ⌘⇧P / Tab all dead (a full keyboard lockout,
  mouse-only recovery). (2) The overlay carries `use:trapFocus={{ onEscape: onClose }}` (`$lib/ui/focus-trap`), pulling
  back programmatic focus leaks and keeping Escape working. Regression coverage: `e2e-playwright/focus-trap.spec.ts`.
- **Two independent cursors.** `cursorIndex` (keyboard) and `hoveredIndex` (mouse) are separate. Arrow keys clear
  `hoveredIndex`; mouse enter/leave updates `hoveredIndex` without touching `cursorIndex`. Both reset on every query
  change (a `$effect` tracking `query`), because the old cursor can point beyond the new results array.
- **Own overlay, not the shared `ModalDialog`.** `CommandPalette` manages its own `position: fixed` overlay and
  `role="dialog"` / combobox ARIA. Overlay-click-to-close uses `e.target === e.currentTarget` so only backdrop clicks
  close it (content clicks bubble up).
- **Combobox + `aria-activedescendant`, never moving DOM focus to the cursor row.** The active option is announced via
  `aria-activedescendant`; the cursor option also gets `tabindex="0"` (roving tabindex of one) to satisfy axe's
  `scrollable-region-focusable`. Moving real focus to a row would steal it from the search input and break
  search-as-you-type.
- **`scrollIntoView` is mocked in tests** (`Element.prototype.scrollIntoView = vi.fn()`): jsdom doesn't implement it,
  and the palette calls it after arrow navigation. Tests crash without the mock.
- **Shortcut chips read live effective bindings, not registry defaults.** Each row reads
  `getEffectiveShortcutsReactive(command.id)` (capped at `MAX_SHORTCUTS_SHOWN = 3`), so a Settings/MCP rebind updates an
  open palette without reopening. Reading `command.shortcuts` instead would show stale, no-longer-working combos.
- **Recents are pruned on mount.** `pruneRecentCommands(validIds)` loads persisted recents, drops ids no longer valid
  palette commands, saves the cleaned list, and feeds it to `searchCommands`. `pushRecentCommand(id)` on every Enter /
  click moves the id to front, dedups, caps at 10. The query is never persisted: the palette opens empty so the
  last-executed command sits at index 0 and Enter re-runs it.

## Adding a new command

Add it to `$lib/commands/command-registry.ts` and handle the id in `routes/(main)/command-dispatch.ts` (see
`commands/CLAUDE.md`). The palette itself needs no changes.

## Dependencies

- `$lib/commands`: `searchCommands`, `getPaletteCommands`, `CommandMatch`.
- `$lib/app-status-store`: `pruneRecentCommands`, `pushRecentCommand`.
- `$lib/shortcuts/reactive-shortcuts.svelte`: `getEffectiveShortcutsReactive`.
- `$lib/ui/ShortcutChip.svelte` and `$lib/ui/StatusBadge.svelte`.

Full details (data flow, the stability-badge derivation, fuzzy-highlight rendering, and decision rationale):
[DETAILS.md](DETAILS.md).
