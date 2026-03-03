# Commands

Centralized command registry and fuzzy search engine for the command palette.

## Files

| File                   | Purpose                                                                                  |
| ---------------------- | ---------------------------------------------------------------------------------------- |
| `types.ts`             | `Command`, `CommandMatch`, `CommandScope` types                                          |
| `command-registry.ts`  | The `commands` array — single source of truth. `getPaletteCommands()` filter.            |
| `fuzzy-search.ts`      | `searchCommands(query)` using `@leeoniya/ufuzzy`                                         |
| `index.ts`             | Barrel re-export                                                                         |
| `fuzzy-search.test.ts` | Vitest tests: empty query, exact/fuzzy matches, ranking, index bounds, palette filtering |

## Types

```ts
interface Command {
    id: string // dot-namespaced: 'app.quit', 'file.rename', 'nav.parent'
    name: string // shown in palette
    scope: CommandScope // hierarchical, display-only (does not enforce routing)
    showInPalette: boolean
    shortcuts: string[] // e.g. ['⌘Q'], ['Backspace', '⌘↑']
    description?: string
}

interface CommandMatch {
    command: Command
    matchedIndices: number[] // flat char indices in command.name for highlight rendering
}
```

`CommandScope` is a union of string literals: `'App'`, `'Main window'`, `'Main window/File list'`,
`'Main window/Brief mode'`, `'Main window/Full mode'`, `'Main window/Network'`, `'Main window/Share browser'`,
`'Main window/Volume chooser'`, `'About window'`, `'Onboarding'`, `'Command palette'`. Scope is documentation-only;
keyboard routing is handled by each UI component.

## Command registry

`command-registry.ts` holds ~70 commands grouped by scope. Key rules:

- `showInPalette: false` for low-level navigation (↑/↓, ←/→, volume/palette modal internals).
- `app.commandPalette` has `showInPalette: false` — opening the palette from inside itself makes no sense.
- `getPaletteCommands()` is the only filter exported; `commands` (the full array) is also exported for use by shortcut
  documentation and future settings panes.

## Fuzzy search

`searchCommands(query)` wraps `@leeoniya/ufuzzy`:

```
query empty → return all getPaletteCommands() with matchedIndices: []
query non-empty →
  haystack = paletteCommands.map(c => c.name)
  [idxs, info, order] = fuzzy.search(haystack, query)
  order.map(...) → CommandMatch[] ranked by relevance
```

uFuzzy configuration:

- `intraMode: 1` — typo-tolerant within-word fuzzy matching (e.g. "tyoe" → "type")
- `interIns: 3` — max 3 inserted characters between matched chars

`info.ranges` is a flat `[start, end, start, end, ...]` array where `end` is exclusive. The code unpacks ranges into
individual character indices for `matchedIndices`.

The uFuzzy instance is a module-level singleton (created once at import time).

## Unified dispatch

Native menu clicks and keyboard shortcuts both route through `handleCommandExecute(commandId)` in `+page.svelte`. The
Rust `on_menu_event` handler maps menu item IDs to command registry IDs and emits a single `"execute-command"` Tauri
event. The frontend listens for this event and calls `handleCommandExecute`. This eliminates the old per-command
individual events (`show-command-palette`, `switch-pane`, etc.).

Exception: `CheckMenuItem`s (show hidden files, view modes) keep separate handling to avoid double-toggle. Close tab
(⌘W) has special logic to close focused non-main windows.

## Adding a command

1. Add an entry to the `commands` array in `command-registry.ts`.
2. Add a `case` for its `id` in the `handleCommandExecute` switch in `routes/(main)/+page.svelte`.
3. No changes needed to the palette, fuzzy search, types, or keyboard dispatch. Commands with `showInPalette: true` are
   automatically dispatched from keyboard shortcuts via centralized dispatch (`../shortcuts/shortcut-dispatch.ts`).
4. If the command has a native menu item, add a mapping in `menu.rs` (`menu_id_to_command` and `command_id_to_menu_id`)
   and add its ID to the `menuCommands` array in `shortcuts-store.ts`.

## Key decisions

**Decision**: `scope` is a documentation-only string literal, not enforced at runtime. **Why**: Keyboard routing is
handled by each UI component (FilePane, NetworkBrowser, etc.) based on what's focused, not by the command system. Making
scope enforcement centralized would require the command module to know about all UI state. Instead, scope exists for
conflict detection in the shortcuts system and for display in settings — the actual dispatch is the responsibility of
each component's keydown handler or the centralized dispatch map.

**Decision**: `showInPalette: false` for native macOS commands (quit, hide, hide others, show all). **Why**: These
commands are handled by macOS via `PredefinedMenuItems` with native selectors (`terminate:`, `hide:`, etc.). The native
menu accelerators handle the keyboard shortcuts directly. Including them in the JS shortcut dispatch map would cause
double-execution — the native handler fires AND the JS handler fires.

**Decision**: `isMacOS()` called at module load time for platform-specific command names and visibility. **Why**: Some
commands only make sense on macOS (`Get info`, `Quick look`, `Show in Finder`). Rather than filtering at render time,
the registry itself contains platform-correct names and `showInPalette` values. This keeps the palette and shortcut
systems platform-aware without platform checks scattered through the UI.

**Decision**: uFuzzy as the fuzzy search library (over alternatives like Fuse.js, fzf-for-js). **Why**: uFuzzy is
optimized for the exact use case — short search phrases against short-to-medium phrases. It's pure TypeScript with no
dependencies, handles typos (`intraMode: 1`), and returns match ranges for highlighting. Fuse.js is heavier and slower
for this scale. The `interIns: 3` setting limits inserted characters between matches to avoid nonsensical matches on a
~60-item list.

**Decision**: uFuzzy instance is a module-level singleton. **Why**: Creating a uFuzzy instance involves compiling regex
patterns from the configuration. Doing this once at import time avoids repeated setup on every search call. The
configuration never changes at runtime.

**Decision**: Unified dispatch through `handleCommandExecute` for both menu clicks and keyboard shortcuts. **Why**:
Before this, each menu item emitted a separate Tauri event (`show-command-palette`, `switch-pane`, etc.) and each had
its own listener. Unifying through a single `execute-command` event with the command ID eliminates per-command wiring.
Exception: `CheckMenuItem`s (hidden files, view modes) keep separate handling because their toggle state needs special
sync to avoid double-toggling.

## Gotchas

**Gotcha**: `commands` is a plain array, not a `Map` or indexed structure. **Why**: The array is ~60 items.
`getPaletteCommands()` filters it on each call, and uFuzzy needs an array of strings anyway. Indexing by ID would help
lookup but add complexity for no measurable gain at this scale.

**Gotcha**: `getPaletteCommands()` is the only filter; there's no `getCommandById()`. **Why**: Command lookup by ID
happens in the shortcuts system (which builds its own reverse map) and in `handleCommandExecute` (which uses a switch
statement). The commands module intentionally stays minimal — it's a registry and a search engine, not a full command
bus.

**Gotcha**: `info.ranges` from uFuzzy is a flat `[start, end, start, end, ...]` array, not an array of tuples. **Why**:
uFuzzy uses this flat format for performance. The code unpacks ranges into individual character indices for
`matchedIndices`. If you change the highlighting approach, you need to understand this flat layout — `end` is exclusive.

**Gotcha**: Adding a command with a menu item requires changes in four places. **Why**: The menu system (Rust) and
command system (TypeScript) are separate codebases connected by string IDs. The four places are: (1)
`command-registry.ts`, (2) `handleCommandExecute` switch, (3) `menu.rs` ID mappings, (4) `menuCommands` array in
`shortcuts-store.ts`. Missing any one causes silent failures (shortcut works but menu doesn't, or vice versa).

## Dependencies

- `@leeoniya/ufuzzy` (npm) — pure TypeScript, no Tauri or Svelte deps in this module.
