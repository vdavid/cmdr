# Command palette

VS Code/Spotlight-style modal for searching and executing app commands via fuzzy matching.

## Files

| File                     | Purpose                                                                           |
| ------------------------ | --------------------------------------------------------------------------------- |
| `CommandPalette.svelte`  | Modal UI: keyboard nav, mouse hover, fuzzy-highlighted results, query persistence |
| `CommandPalette.test.ts` | Vitest tests with mocked `$lib/commands` and `$lib/app-status-store`              |

## Data flow

```
User presses ⌘⇧P
  → +page.svelte sets showCommandPalette = true
  → CommandPalette mounts, loads persisted query from app-status-store, focuses input
  → searchCommands(query) returns CommandMatch[] (reactive via $derived)
  → User navigates with ↑/↓ (keyboard cursor) or mouse (hover cursor)
  → Enter / click → onExecute(commandId) → handleCommandExecute() in command-dispatch.ts
  → Escape / overlay click → query saved, onClose() called
```

## Key patterns

**Two-cursor hover model**: `cursorIndex` (keyboard) and `hoveredIndex` (mouse) are independent. Arrow keys clear
`hoveredIndex`; mouse enter/leave updates `hoveredIndex` without touching `cursorIndex`. The CSS classes
`is-under-cursor` and `is-hovered` apply different highlight intensities (`--color-accent-subtle` vs. a subtle rgba
overlay).

**Event propagation**: `stopPropagation()` is called on every `keydown` in the overlay `div`'s handler. This prevents
the file list from scrolling or handling shortcuts behind the modal.

**Query persistence**: `loadPaletteQuery` / `savePaletteQuery` from `$lib/app-status-store` (Tauri store). Query is
loaded on mount; saved on Escape, Enter, and overlay-click close.

**Own overlay, no shared ModalDialog**: `CommandPalette` manages its own `position: fixed` overlay and `role="dialog"`
ARIA attributes. It does not use the shared `ModalDialog` component.

**Cursor reset**: `cursorIndex` resets to `0` and `hoveredIndex` to `null` on every query change (via `$effect` tracking
`query`).

**Fuzzy highlight rendering**: `highlightMatches()` converts the flat `matchedIndices` set into `{ text, highlighted }`
segments and renders `<mark class="match-highlight">` for matched chars.

**Shortcut display**: `formatShortcuts()` calls `.slice(0, 2)` — at most 2 shortcuts are shown per command.

**jsdom limitation**: `scrollIntoView` is not implemented in jsdom; tests mock it via
`Element.prototype.scrollIntoView = vi.fn()`.

## Key decisions

**Decision**: Own overlay and modal, not using the shared `ModalDialog` component. **Why**: The command palette has
unique interaction requirements (live fuzzy filtering, keyboard-first navigation, two-cursor hover model) that don't fit
`ModalDialog`'s confirm/cancel pattern. Sharing would mean adding palette- specific escape hatches to a generic
component. The palette's overlay also needs `stopPropagation` on all keydown events — a concern that shouldn't leak into
a shared component.

**Decision**: Two independent cursor models (`cursorIndex` for keyboard, `hoveredIndex` for mouse). **Why**: VS Code and
Spotlight both have this behavior: arrow keys move a "hard" cursor, while mouse hover shows a "soft" highlight that
doesn't interfere with keyboard navigation. Arrow keys clear `hoveredIndex` so there's never two items highlighted at
the same intensity. Without this separation, moving the mouse would fight keyboard navigation.

**Decision**: Query persisted across open/close via `app-status-store`. **Why**: Users often open the palette, run a
command, then reopen to run a related command. Preserving the query saves retyping. Saved on every close path (Escape,
Enter, overlay click) to ensure nothing is lost.

**Decision**: `$derived` for search results instead of debounced input. **Why**: `searchCommands()` via uFuzzy is fast
enough for ~60 commands that debouncing would only add latency. The `$derived` reactive binding reruns the search
synchronously on every keystroke, keeping results perfectly in sync with the input. Debouncing would be needed if the
command list grew to thousands.

**Decision**: `formatShortcuts()` caps display at 2 shortcuts via `.slice(0, 2)`. **Why**: Some commands have many
shortcuts (e.g., `nav.parent` has `Backspace` and `Cmd+Up`). Showing all of them would crowd the result row. Two is
enough for discoverability; the full list lives in the shortcut settings.

## Gotchas

**Gotcha**: `stopPropagation()` is called on every `keydown`, not just handled keys. **Why**: Without this, unhandled
keys (letters, numbers) would propagate to the file explorer behind the modal and trigger quick-search or other
handlers. The palette's overlay div catches all keyboard events first.

**Gotcha**: `cursorIndex` and `hoveredIndex` both reset on every query change. **Why**: After typing, the old cursor
position may point beyond the new results array. Resetting to 0 / null avoids an out-of-bounds index. This happens in a
`$effect` tracking `query`.

**Gotcha**: `scrollIntoView` is mocked in tests (`Element.prototype.scrollIntoView = vi.fn()`). **Why**: jsdom doesn't
implement `scrollIntoView`. The palette calls it after arrow key navigation to keep the cursor visible. Tests would
crash without the mock.

**Gotcha**: Overlay click detection uses `e.target === e.currentTarget`. **Why**: Clicks on the modal content (input,
results) bubble up to the overlay. Only clicks on the semi-transparent backdrop itself should close the palette.
Checking `target === currentTarget` ensures the click originated on the overlay div, not a child.

## Adding a new command

Add the command to `$lib/commands/command-registry.ts` and handle the ID in the `handleCommandExecute` switch in
`routes/(main)/command-dispatch.ts`. The palette itself needs no changes.

## Dependencies

- `$lib/commands` — `searchCommands`, `CommandMatch`
- `$lib/app-status-store` — `loadPaletteQuery`, `savePaletteQuery`
- CSS variables from `app.css` (`--z-modal`, `--color-accent-subtle`, `--color-bg-secondary`, etc.)
