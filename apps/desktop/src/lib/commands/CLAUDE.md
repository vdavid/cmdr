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

`command-registry.ts` holds ~60 commands grouped by scope. Key rules:

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

## Adding a command

1. Add an entry to the `commands` array in `command-registry.ts`.
2. Add a `case` for its `id` in the `handleCommandExecute` switch in `routes/(main)/+page.svelte`.
3. No changes needed to the palette, fuzzy search, or types.

## Dependencies

- `@leeoniya/ufuzzy` (npm) — pure TypeScript, no Tauri or Svelte deps in this module.
