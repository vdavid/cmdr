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
  → Enter / click → onExecute(commandId) → handleCommandExecute() switch in +page.svelte
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

## Adding a new command

Add the command to `$lib/commands/command-registry.ts` and handle the ID in the `handleCommandExecute` switch in
`routes/(main)/+page.svelte`. The palette itself needs no changes.

## Dependencies

- `$lib/commands` — `searchCommands`, `CommandMatch`
- `$lib/app-status-store` — `loadPaletteQuery`, `savePaletteQuery`
- CSS variables from `app.css` (`--z-modal`, `--color-accent-subtle`, `--color-bg-secondary`, etc.)
