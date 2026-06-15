# Commands: details

Depth and rationale. `CLAUDE.md` holds the must-knows that prevent silent breakage; this file holds the type
definitions, the dispatch model, fuzzy-search behavior, and decision rationale.

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
  fixedKey?: true // key hardcoded in the owning component, never reads the store; read-only in the editor
  status?: 'alpha' | 'beta' // stability badge in the palette row; derive via getBadgeStatus() from $lib/feature-status
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
// `CommandArgsOptionalOverrides`: these dispatch arg-less from one path and with
// a payload from another (`file.copy`/`move`/`delete`: arg-less from the F-bar /
// palette, payload-carrying from the MCP tools). `CommandDispatchArgs` distributes
// over `K` so a broad `CommandId` resolves to `[] | [args] | [args?]`.
interface CommandArgsOverrides {
  'view.setMode': { pane: PaneId; mode: ViewMode; fromMenu: boolean } // fromMenu: skip vs push menu state
  'sort.set': { pane: PaneId; column: SortColumn; order: 'asc' | 'desc' }
  'selection.mcpSelect': { pane: PaneId; start: number; count: number | 'all'; mode: McpSelectMode }
  'selection.mcpSelectByNames': { pane: PaneId; names: string[]; mode: McpSelectMode }
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

The per-pane MCP commands route through `mcp-listeners.ts` (a transport adapter, see `routes/(main)/CLAUDE.md`); the
adapter validate-parses each raw payload into these arg shapes before dispatching. They're all `showInPalette: false`.

`CommandScope` is a union of string literals: `'App'`, `'Main window'`, `'Main window/File list'`,
`'Main window/Brief mode'`, `'Main window/Full mode'`, `'Main window/Network'`, `'Main window/Share browser'`,
`'Main window/Volume chooser'`, `'About window'`, `'Onboarding'`, `'Command palette'`. Scope is documentation-only.

## Command registry

`command-registry.ts` holds the commands grouped by scope (most are palette-visible; the rest are
`showInPalette: false`: low-level navigation and MCP-only per-pane commands). `app.commandPalette` is
`showInPalette: false` (opening the palette from inside itself makes no sense). `getPaletteCommands()` is the only
filter exported; `commands` (the full array) is exported too, for shortcut documentation and Settings panes.

`isMacOS()` is called at module load so the registry contains platform-correct names and `showInPalette` values
(`Get info`, `Quick look`, `Show in Finder` only make sense on macOS), keeping the palette and shortcut systems
platform-aware without platform checks scattered through the UI.

## Adding a command (full steps)

1. Add the id to `COMMAND_IDS` in `command-ids.ts`. Skipping this makes the registry entry a compile error (`Command.id`
   is `CommandId`).
2. Add an entry to the `commands` array in `command-registry.ts`. Skipping this fails the set-equality test in
   `command-registry.test.ts`.
3. If the command carries a REQUIRED dispatch payload, declare its shape in `CommandArgsOverrides` in `types.ts`; if the
   payload is OPTIONAL (dispatched both arg-less and with a payload, like the MCP `file.copy`/`move`/`delete` tools),
   use `CommandArgsOptionalOverrides` so `CommandDispatchArgs` resolves to `[args?]`. Arg-less commands skip both. The
   handler reads the payload from `hctx.dispatchArgs`. MCP-only commands dispatch from the `mcp-listeners.ts` adapter
   after a validate-parse, and are `showInPalette: false`.
4. Add the handler to the right family module in `routes/(main)/command-handlers/`. The `commandHandlers` record is
   keyed by `Exclude<CommandId, DispatchExemptId>`, so a missing handler is a compile error; an intentionally
   handlerless command goes in `DISPATCH_EXEMPT_IDS` (in `command-handlers/types.ts`) with a documented reason
   (`command-handler-record.test.ts` fails if the id is in neither).
5. No changes needed to fuzzy search or keyboard dispatch. Commands with `showInPalette: true` auto-dispatch from
   keyboard shortcuts via centralized dispatch (`../shortcuts/shortcut-dispatch.ts`). For a palette-visible command,
   also add its id to `EXPECTED_PALETTE_IDS` in `command-registry.test.ts` (the palette-visible-set pin fails
   otherwise).
6. For a native menu item, add a mapping in `menu.rs` (`menu_id_to_command` and `command_id_to_menu_id`) and add the id
   to `menuCommands` in `shortcuts-store.ts`. `rust-command-id-drift.test.ts` fails if `menu_id_to_command` emits an id
   not in `COMMAND_IDS`.

## Fuzzy search

`searchCommands(query, recentCommandIds?)` wraps `@leeoniya/ufuzzy`:

- Empty query: recents first (filtered through `getPaletteCommands` to drop stale ids), then the rest of
  `getPaletteCommands()` in registry order.
- Non-empty query: haystack is `paletteCommands.map(c => c.name)`; `fuzzy.search(haystack, query)` yields ranked
  `CommandMatch[]` (the `recents` argument is ignored).

uFuzzy config: `intraMode: 1` (typo-tolerant within-word fuzzy, "tyoe" → "type"), `interIns: 3` (max 3 inserted chars
between matched chars, to avoid nonsensical matches on a short list). `info.ranges` is a flat
`[start, end, start, end, …]` array with exclusive `end`, unpacked into per-char indices for `matchedIndices`.

`searchAllCommands(query)` runs the same matcher over the FULL registry (including `showInPalette: false` entries), with
no recents handling (empty query → everything in registry order). It exists for surfaces whose result set is the whole
registry: the shortcuts editor renders and rebinds every command, so its name search (and the `getMatchingSections`
sidebar highlight in settings-search) must cover the same set. The command palette itself stays on `searchCommands`.

## Unified dispatch

Native menu clicks and keyboard shortcuts both route through `handleCommandExecute(commandId)` in
`routes/(main)/command-dispatch.ts`. The Rust `on_menu_event` handler maps menu item ids to command registry ids and
emits a single `"execute-command"` Tauri event; the frontend listens and calls `handleCommandExecute`.

Exception: `CheckMenuItem`s (show hidden files, view modes) keep separate handling to avoid double-toggle. Close tab
(⌘W) has special logic to close focused non-main windows.

`view.showHidden` uses a local-first path inside `handleCommandExecute`: it flips the explorer's `showHiddenFiles` state
synchronously via `explorerRef.toggleHiddenFiles()`, then fire-and-forgets `syncMenuShowHidden(newState)` to update the
macOS/Linux `CheckMenuItem` checked state. This avoids an IPC + Tauri-event + Svelte-effect hop between keystroke and
DOM (the `toggles hidden file visibility` e2e spec flaked under slow-lane CPU contention on the awaited path). The
native menu accelerator / palette-click paths still travel through `on_menu_event` → `settings-changed` → FE listener.

## Decisions and rationale

- **`scope` is documentation-only.** Centralized scope enforcement would require the command module to know all UI
  state. Scope exists for conflict detection in the shortcuts system and for Settings display; dispatch is each
  component's keydown handler or the centralized dispatch map.
- **`showInPalette: false` AND `nativeShortcut: true` for native macOS commands.** They're handled by macOS via
  `PredefinedMenuItems` with native selectors (`terminate:`, `hide:`, …); the native accelerators handle the keyboard
  shortcuts. Including them in the JS dispatch map would double-execute. AppKit owns both behavior and accelerator, so
  Cmdr can neither rebind nor intercept them. `nativeShortcut: true` (on `NATIVE_SHORTCUT_COMMAND_IDS`) makes the editor
  render these rows read-only and the store refuse to write them; `DISPATCH_EXEMPT_IDS` sources its Family-1 list from
  the same constant.
- **uFuzzy over Fuse.js / fzf-for-js.** uFuzzy is optimized for short query phrases against short-to-medium phrases,
  pure TypeScript with no dependencies, handles typos, and returns match ranges for highlighting. Fuse.js is heavier and
  slower at this scale.
- **uFuzzy instance is a module-level singleton.** Creating an instance compiles regex from config; doing it once at
  import avoids repeated setup per search. The config never changes at runtime.
- **`commands` is a plain array, not a `Map`.** It's ~110 items; `getPaletteCommands()` filters on each call and uFuzzy
  needs an array of strings anyway. Indexing by id would add complexity for no measurable gain at this scale. There's no
  `getCommandById()`: lookup by id happens in the shortcuts system (its own reverse map) and in `handleCommandExecute`
  (the flat handler record). The module stays a registry and a search engine, not a command bus.
