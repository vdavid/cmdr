# Commands

Centralized command registry and fuzzy search engine for the command palette.

## Files

| File                            | Purpose                                                                                                                    |
| ------------------------------- | -------------------------------------------------------------------------------------------------------------------------- |
| `command-ids.ts`                | `COMMAND_IDS` (the `as const` id tuple), the derived `CommandId` union, and the `isCommandId()` boundary guard             |
| `types.ts`                      | `Command`, `CommandMatch`, `CommandScope`, plus `CommandArgs` / `CommandDispatchArgs` (the dispatch arg-tuple shape)       |
| `command-registry.ts`           | The `commands` array (single source of truth). `getPaletteCommands()` filter. `updateLicenseCommandName()` in-place write. |
| `fuzzy-search.ts`               | `searchCommands(query, recentCommandIds?)` using `@leeoniya/ufuzzy`                                                        |
| `index.ts`                      | Barrel re-export                                                                                                           |
| `fuzzy-search.test.ts`          | Vitest tests: empty query, exact/fuzzy matches, ranking, index bounds, palette filtering                                   |
| `command-registry.test.ts`      | Set-equality guard (tuple ↔ registry), `isCommandId`, `updateLicenseCommandName`, palette-visible-set pin                  |
| `command-types.test.ts`         | Compile-time `@ts-expect-error` guards for the `CommandId` union and arg-tuple shapes                                      |
| `rust-command-id-drift.test.ts` | Parses `menu/mod.rs` + `LicenseSection.svelte`; asserts every Rust-emitted command id ∈ `COMMAND_IDS`                      |

## Types

```ts
type CommandId = (typeof COMMAND_IDS)[number] // closed union of every id (command-ids.ts)

interface Command {
  id: CommandId // dot-namespaced: 'app.quit', 'file.rename', 'nav.parent'
  name: string // shown in palette
  scope: CommandScope // hierarchical, display-only (does not enforce routing)
  showInPalette: boolean
  shortcuts: string[] // e.g. ['⌘Q'], ['Backspace', '⌘↑']
  nativeShortcut?: true // macOS owns behavior AND accelerator (PredefinedMenuItem); read-only in the editor
  description?: string
}

interface CommandMatch {
  command: Command
  matchedIndices: number[] // flat char indices in command.name for highlight rendering
}

// Dispatch arg foundation. Most commands are arg-less (→ `NoCommandArgs`, an
// `undefined` marker), so `dispatch('file.rename')` needs no second arg.
// REQUIRED-payload commands declare their shape in `CommandArgsOverrides`
// (mostly the per-pane MCP commands). OPTIONAL-payload commands declare it in
// `CommandArgsOptionalOverrides` — these dispatch arg-less from one path and with
// a payload from another (`file.copy`/`move`/`delete`: arg-less from the F-bar /
// palette, payload-carrying from the MCP tools). `CommandDispatchArgs` distributes
// over `K` so a broad `CommandId` resolves to `[] | [args] | [args?]`.
interface CommandArgsOverrides {
  'view.setMode': { pane: PaneId; mode: ViewMode; fromMenu: boolean } // fromMenu: skip vs push menu state
  'sort.set': { pane: PaneId; column: SortColumn; order: 'asc' | 'desc' }
  'selection.mcpSelect': { pane: PaneId; start: number; count: number | 'all'; mode: McpSelectMode }
  'cursor.moveTo': { pane: PaneId; to: number | string }
  'cursor.scrollTo': { pane: PaneId; index: number }
  'volume.selectByName': { pane: PaneId; name: string }
  'tab.mcpAction': { pane: PaneId; action: McpTabAction; tabId?: string; pinned?: boolean }
  'dialog.confirm': { type: ConfirmDialogType; onConflict?: string }
}
interface CommandArgsOptionalOverrides {
  'file.copy': { autoConfirm?: boolean; onConflict?: string }
  'file.move': { autoConfirm?: boolean; onConflict?: string }
  'file.delete': { autoConfirm?: boolean }
}
type CommandArgs = {
  [K in CommandId]: K extends keyof CommandArgsOverrides
    ? CommandArgsOverrides[K]
    : K extends keyof CommandArgsOptionalOverrides
      ? CommandArgsOptionalOverrides[K]
      : NoCommandArgs
}
type CommandDispatchArgs<K extends CommandId> = K extends CommandId
  ? K extends keyof CommandArgsOptionalOverrides
    ? [args?: CommandArgs[K]]
    : CommandArgs[K] extends NoCommandArgs
      ? []
      : [args: CommandArgs[K]]
  : never
```

The per-pane MCP commands route through `mcp-listeners.ts` (a transport adapter — see `routes/(main)/CLAUDE.md`); the
adapter validate-parses each raw payload into these arg shapes before dispatching. They're all `showInPalette: false`.

### Typed ids and the dispatch boundary

`COMMAND_IDS` (in `command-ids.ts`) is an `as const` tuple; `CommandId` is its element union. The registry array stays a
**mutable** `Command[]` — not `as const satisfies readonly Command[]` — because `updateLicenseCommandName` rewrites an
entry's `.name` in place, and `getPaletteCommands()` plus the shortcuts conflict detector consume a mutable `Command[]`.
`Command.id: CommandId` enforces tuple ⊇ registry at compile time; the set-equality test enforces registry ⊇ tuple.

`isCommandId(value: string): value is CommandId` narrows the un-typed string edges where ids enter the frontend — the
Rust `execute-command` event payload (`+page.svelte`), the cross-window emit from `LicenseSection.svelte`, and the
selection-dialog `onCommand` prop. Never `as CommandId`-cast at these edges; a stale id would miss the handler record
and silently no-op. The IPC boundary is un-typed (Rust emits a bare `json!`), so `rust-command-id-drift.test.ts` is the
backstop that keeps Rust ids and `COMMAND_IDS` in sync.

`handleCommandExecute<K extends CommandId>(commandId: K, ctx, ...args: CommandDispatchArgs<K>)` is the typed dispatch
entry point. Arg-less ids take no third argument; arg-carrying ids (like `view.setMode`) require their payload. Inside,
`commandId` widens to the `CommandId` union, the core runs its preamble, then looks the id up in the flat
`commandHandlers` record (in `routes/(main)/command-handlers/`). The record is keyed by
`Exclude<CommandId, DispatchExemptId>`, so every dispatchable id has a handler at compile time (a missing one is a
compile error); the 20 `DispatchExemptId`s deliberately have none and silently no-op. See `routes/(main)/CLAUDE.md` and
`routes/(main)/command-handlers/CLAUDE.md` for the handler record and the exempt families.

`CommandScope` is a union of string literals: `'App'`, `'Main window'`, `'Main window/File list'`,
`'Main window/Brief mode'`, `'Main window/Full mode'`, `'Main window/Network'`, `'Main window/Share browser'`,
`'Main window/Volume chooser'`, `'About window'`, `'Onboarding'`, `'Command palette'`. Scope is documentation-only;
keyboard routing is handled by each UI component.

## Command registry

`command-registry.ts` holds 109 commands grouped by scope (about 77 palette-visible; the rest are `showInPalette: false`
— low-level navigation and the MCP-only per-pane commands). Key rules:

- `showInPalette: false` for low-level navigation (↑/↓, ←/→, volume/palette modal internals).
- `app.commandPalette` has `showInPalette: false`; opening the palette from inside itself makes no sense.
- `getPaletteCommands()` is the only filter exported; `commands` (the full array) is also exported for use by shortcut
  documentation and future settings panes.

## Fuzzy search

`searchCommands(query, recentCommandIds?)` wraps `@leeoniya/ufuzzy`:

```
query empty →
  recents (filtered through getPaletteCommands to drop stale IDs) first,
  then the rest of getPaletteCommands() in registry order
query non-empty →
  haystack = paletteCommands.map(c => c.name)
  [idxs, info, order] = fuzzy.search(haystack, query)
  order.map(...) → CommandMatch[] ranked by relevance (recents argument ignored)
```

uFuzzy configuration:

- `intraMode: 1`: typo-tolerant within-word fuzzy matching (for example "tyoe" → "type")
- `interIns: 3`: max 3 inserted characters between matched chars

`info.ranges` is a flat `[start, end, start, end, ...]` array where `end` is exclusive. The code unpacks ranges into
individual character indices for `matchedIndices`.

The uFuzzy instance is a module-level singleton (created once at import time).

## Unified dispatch

Native menu clicks and keyboard shortcuts both route through `handleCommandExecute(commandId)` in
`routes/(main)/command-dispatch.ts`. The Rust `on_menu_event` handler maps menu item IDs to command registry IDs and
emits a single `"execute-command"` Tauri event. The frontend listens for this event and calls `handleCommandExecute`.
This eliminates the old per-command individual events (`show-command-palette`, `switch-pane`, etc.).

Exception: `CheckMenuItem`s (show hidden files, view modes) keep separate handling to avoid double-toggle. Close tab
(⌘W) has special logic to close focused non-main windows.

`view.showHidden` specifically uses a **local-first** path inside `handleCommandExecute`: it flips the explorer's
`showHiddenFiles` state synchronously via `explorerRef.toggleHiddenFiles()`, then fire-and-forgets
`syncMenuShowHidden(newState)` to update the macOS/Linux `CheckMenuItem` checked state. Previously this case awaited the
`toggle_hidden_files` Rust IPC, which toggled the menu item and emitted `settings-changed` for the FE to react to. That
added an IPC + Tauri-event + Svelte-effect hop between the keystroke and the DOM, and the
`toggles hidden file visibility` e2e spec flaked ~1/25 runs under slow-lane CPU contention. The native menu accelerator
/ palette-click paths are unaffected — they still travel through `on_menu_event` → `settings-changed` → FE listener.

## Adding a command

1. Add the id to the `COMMAND_IDS` tuple in `command-ids.ts`. (Skipping this makes the registry entry a compile error,
   since `Command.id` is `CommandId`.)
2. Add an entry to the `commands` array in `command-registry.ts`. (Skipping this fails the set-equality test in
   `command-registry.test.ts`.)
3. If the command carries a REQUIRED dispatch payload, declare its shape in `CommandArgsOverrides` in `types.ts`; if the
   payload is OPTIONAL (dispatched both arg-less and with a payload, like the MCP `file.copy`/`move`/`delete` tools),
   use `CommandArgsOptionalOverrides` so `CommandDispatchArgs` resolves to `[args?]`. Arg-less commands skip both (they
   default to `NoCommandArgs`). The handler then reads the payload from `hctx.dispatchArgs`. MCP-only commands are
   dispatched from the `mcp-listeners.ts` adapter after a validate-parse, and are `showInPalette: false`.
4. Add the handler to the right family module in `routes/(main)/command-handlers/`. The `commandHandlers` record is
   keyed by `Exclude<CommandId, DispatchExemptId>`, so a missing handler is a COMPILE error; an intentionally
   handlerless command goes in `DISPATCH_EXEMPT_IDS` (in `command-handlers/types.ts`) with a documented reason (the
   `command-handler-record.test.ts` set-equality test fails if the id is in neither).
5. No changes needed to fuzzy search or keyboard dispatch. Commands with `showInPalette: true` are automatically
   dispatched from keyboard shortcuts via centralized dispatch (`../shortcuts/shortcut-dispatch.ts`). If the command is
   palette-visible, also add its id to `EXPECTED_PALETTE_IDS` in `command-registry.test.ts` (the palette-visible-set pin
   fails otherwise); an MCP-only / low-level command stays `showInPalette: false` and out of that list.
6. If the command has a native menu item, add a mapping in `menu.rs` (`menu_id_to_command` and `command_id_to_menu_id`)
   and add its ID to the `menuCommands` array in `shortcuts-store.ts`. The `rust-command-id-drift.test.ts` test will
   fail if `menu_id_to_command` emits an id that isn't in `COMMAND_IDS`.

## Key decisions

**Decision**: `scope` is a documentation-only string literal, not enforced at runtime. **Why**: Keyboard routing is
handled by each UI component (FilePane, NetworkBrowser, etc.) based on what's focused, not by the command system. Making
scope enforcement centralized would require the command module to know about all UI state. Instead, scope exists for
conflict detection in the shortcuts system and for display in settings; the actual dispatch is the responsibility of
each component's keydown handler or the centralized dispatch map.

**Decision**: `showInPalette: false` AND `nativeShortcut: true` for native macOS commands (quit, hide, hide others, show
all). **Why**: These commands are handled by macOS via `PredefinedMenuItems` with native selectors (`terminate:`,
`hide:`, etc.). The native menu accelerators handle the keyboard shortcuts directly. Including them in the JS shortcut
dispatch map would cause double-execution: the native handler fires AND the JS handler fires. AppKit owns BOTH the
behavior and the accelerator, so Cmdr can neither rebind nor intercept them — editing them in Settings would be a double
illusion (removal doesn't disable the OS accelerator; a new binding dispatches into a void). `nativeShortcut: true` (set
on exactly the `NATIVE_SHORTCUT_COMMAND_IDS` exported from `command-registry.ts`) is the single source of truth that
makes the shortcuts editor render these rows read-only and makes the store mutators refuse to write them.
`command-handlers/types.ts` sources its Family-1 dispatch-exempt list from the same `NATIVE_SHORTCUT_COMMAND_IDS`, so
the "AppKit owns this" fact lives in one place; `command-registry.test.ts` pins the flag set-equal to that list.

**Decision**: `isMacOS()` called at module load time for platform-specific command names and visibility. **Why**: Some
commands only make sense on macOS (`Get info`, `Quick look`, `Show in Finder`). Rather than filtering at render time,
the registry itself contains platform-correct names and `showInPalette` values. This keeps the palette and shortcut
systems platform-aware without platform checks scattered through the UI.

**Decision**: uFuzzy as the fuzzy search library (over alternatives like Fuse.js, fzf-for-js). **Why**: uFuzzy is
optimized for the exact use case: short search phrases against short-to-medium phrases. It's pure TypeScript with no
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

**Gotcha**: `commands` is a plain array, not a `Map` or indexed structure. **Why**: The array is ~109 items.
`getPaletteCommands()` filters it on each call, and uFuzzy needs an array of strings anyway. Indexing by ID would help
lookup but add complexity for no measurable gain at this scale.

**Gotcha**: `getPaletteCommands()` is the only filter; there's no `getCommandById()`. **Why**: Command lookup by ID
happens in the shortcuts system (which builds its own reverse map) and in `handleCommandExecute` (which looks the id up
in the flat handler record). The commands module intentionally stays minimal: it's a registry and a search engine, not a
full command bus.

**Gotcha**: `info.ranges` from uFuzzy is a flat `[start, end, start, end, ...]` array, not an array of tuples. **Why**:
uFuzzy uses this flat format for performance. The code unpacks ranges into individual character indices for
`matchedIndices`. If you change the highlighting approach, you need to understand this flat layout (`end` is exclusive).

**Gotcha**: `handleCommandExecute` intercepts `edit.copy` and `selection.selectAll` BEFORE logging when the user's
selection is inside an opt-in text region (`.error-pane` or `[data-text-region]`). **Why**: The native macOS Edit menu's
accelerators (⌘C, ⌘A) fire through this dispatcher even when the user is selecting/copying plain text in the ErrorPane.
Without the early bail, every text copy would log `FE:user-action edit.copy` and trigger file-scope side effects (file
copy, file select-all), polluting the user-action log used for rollback and breaking the user's expectation that ⌘C/⌘A
do text things in selectable regions. See `handleTextRegionShortcut` in `command-dispatch.ts`.

**Gotcha**: Adding a command with a menu item requires changes in four places. **Why**: The menu system (Rust) and
command system (TypeScript) are separate codebases connected by string IDs. The four places are: (1)
`command-registry.ts`, (2) the handler in `routes/(main)/command-handlers/`, (3) `src-tauri/src/menu/mod.rs` ID mappings
(`menu_id_to_command` + `command_id_to_menu_id`) plus the matching `Menu/SubmenuItem::with_id` registration in the right
platform builder (`macos.rs` / `linux.rs` — the top-level menus include `Select` between Edit and View), (4)
`menuCommands` array in `shortcuts-store.ts`. Missing any one causes silent failures (shortcut works but menu doesn't,
or vice versa).

## Dependencies

- `@leeoniya/ufuzzy` (npm): pure TypeScript, no Tauri or Svelte deps in this module.
