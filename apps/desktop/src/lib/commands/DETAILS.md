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
  'volume.selectByName': { pane: PaneId; name?: string; volumeId?: string; mcpRequestId?: string } // id wins; the MCP reply's round-trip id
  'tab.mcpAction': { pane: PaneId; action: McpTabAction; tabId?: string; pinned?: boolean } | McpTabMoveArgs // `move` carries toPane / toIndex + the reply's round-trip id
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

`ConfirmDialogType` covers `transfer-confirmation` and `delete-confirmation` from the MCP `dialog confirm` tool, plus
`archive-password` from `unlock_archive`, which stores the password on the backend first and then confirms the prompt to
trigger the frontend's follow-up. Reusing `dialog.confirm` for that third one is deliberate: it's the existing
"programmatically answer an open dialog" seam, so the tool needs no command id of its own.

`CommandScope` is a union of string literals: `'App'`, `'Main window'`, `'Main window/File list'`,
`'Main window/Brief mode'`, `'Main window/Full mode'`, `'Main window/Servers'`, `'Main window/Places'`,
`'Main window/Volume chooser'`, `'Main window/Favorites menu'`, `'About window'`, `'Onboarding'`, `'Command palette'`.
Scope is documentation-only.

❗ **A NEW scope is three places**: the `CommandScope` union here, its ancestry chain in `shortcuts/scope-hierarchy.ts`
(without one, `getActiveScopes` returns `[]` and its commands can never conflict with anything), and a row in
`settings/sections/keyboard-shortcuts-grouping.ts`'s `scopeOrder` — miss that last one and every command on the scope
silently vanishes from the rebinding UI, which its exhaustiveness test catches.

## Command registry

The registry data is split by top-level scope into `sources/*.ts` (`app`, `main-window`, `file-list`, `browsers`, `mcp`,
`about-window`, `command-palette`), each exporting a `CommandSource[]`. `command-registry.ts` concatenates them in
authoring order into `commandSources` (order is load-bearing: it drives palette listing and shortcut conflict
resolution) and holds all the logic. Most commands are palette-visible; the rest are `showInPalette: false`: low-level
navigation and MCP-only per-pane commands. `app.commandPalette` is `showInPalette: false` (opening the palette from
inside itself makes no sense). `getPaletteCommands(holds?)` is the only filter exported; `commands` (the full array) is
exported too, for shortcut documentation and Settings panes.

**Conditional palette rows.** A source may carry a `paletteCondition` (a `PaletteCondition` member, today only
`focusedPaneSharesLinks`), and the palette shows it only while that holds. `CommandPalette.svelte` answers each one
through its `PALETTE_CONDITIONS` record when it searches, so a new member doesn't compile until it's answered there.
Without `holds`, `getPaletteCommands` counts every conditional row as shown, which is what pruning stale recents wants.
Every other surface (Settings, help, MCP) lists the command regardless, and its handler still checks: the palette knows
the pane, not the cursor row.

`command-ids.ts::COMMAND_IDS` is also the backend's command-breadcrumb vocabulary. `src-tauri/build.rs` parses the tuple
into an `OUT_DIR` Rust slice, and `error_reporter/breadcrumbs.rs` retains a command event only when its id is in that
generated slice. The generator fails on unrecognized tuple syntax instead of producing a partial allowlist, and the Rust
drift test compares the compiled slice back to the authored tuple. This keeps one registry while closing the untrusted
IPC boundary; the generated Specta payload remains `commandId: string`, and the frontend wrapper narrows it to
`CommandId`.

`isMacOS()` is called at module load so the registry contains platform-correct names and `showInPalette` values
(`Get info`, `Quick look`, `Show in Finder` only make sense on macOS), keeping the palette and shortcut systems
platform-aware without platform checks scattered through the UI.

### i18n: keys, not English

Each entry is authored as a `CommandSource` (`Omit<Command, 'name' | 'displayName' | 'description'>` plus `nameKey`, an
optional `descriptionKey`, and an optional `displayName`). `resolveCommand` maps each source to a `Command` whose `name`
/ `displayName` / `description` are getters calling `tString()` against `messages/en/commands.json`, mirroring the
settings-registry pattern. Reading `command.name` resolves the current catalog string, so the palette, the fuzzy
haystack (rebuilt per search), the shortcuts list, and the menus all stay unchanged and reactive; the per-key base-en
parity net is `command-registry.parity.test.ts`. Key shape is `commands.<idish>.label` / `.description` (the command id
flattened to a lowerCamel leaf, e.g. `view.zoom.set75` → `commands.viewZoomSet75.label`); the command IDS themselves
never change.

### Two labels: `name` and `displayName`

`name` is the command's one static label, and it's what every listing surface reads: Settings > Shortcuts, the help
window, the conflict toast, and the MCP bridge. A row in those lists names a command; it doesn't offer to run it on
whatever's under the cursor, so a live label there would be noise at best.

`displayName` is the COMMAND PALETTE's label, and the palette is its only reader (the `fuzzy-search.ts` haystack and
`CommandPalette.svelte`, which highlights against the same string — so the fuzzy indices clamp to `displayName.length`,
never `name.length`). A source declares `displayName?: () => string` when its effect depends on where the cursor is and
saying so is worth it ("Select all with extension .pdf"); omit it and the getter falls back to `name`, which is why a
command's Settings row can read a plain static label while its palette row reads the live one. Nothing caches either
value — the getters resolve at read time, `fuzzy-search` rebuilds its haystack each keystroke, and
`CommandPalette.svelte`'s results are a `$derived` — so a label can change between two reads with nothing to invalidate.

Three sources of a non-constant label, all generic — ❌ `resolveCommand` carries no per-id branch:

- **`favorites.addFromMenu` points at `fileExplorer.navigation.favoritesAddCurrent`**, the only `nameKey` outside
  `commands.*`. The entry is `showInPalette: false` + `fixedKey: true`: it exists so Settings > Keyboard shortcuts can
  say the open favorites menu answers `0`, and what it says is the menu's own `0`-row label. Giving it a `commands.*`
  twin meant one string in two places, which five locales had already collapsed into byte-identical values and one
  English edit would have split. A row that QUOTES a surface should read that surface's key.
- **The three `isMacOS()` commands** (`file.showInFinder` / `file.getInfo` / `file.quickLook`) pick one of two keys at
  module load (`.mac.label` vs `.other.label`), so each platform's wording is a distinct, separately-translatable key.
- **A `nameKey` thunk** (`() => MessageKey`) picks between catalog keys on live state, resolved on every read.
  `app.licenseKey` is the one user: it reads `hasExistingLicense` (in `sources/app.ts`, beside the entry) and resolves
  `commands.appLicenseKey.seeDetails.label` or `…enterKey.label`. `updateLicenseCommandName(hasLicense)` flips the flag
  — it doesn't rewrite English in place — keeping the palette label in step with the native menu.
- **A `displayName` resolver** composes the palette label outright, when no fixed set of catalog keys covers it.
  `selection.selectSameKind` is the one user: it renders whatever the focused pane published into
  `file-explorer/pane/same-kind-target.svelte.ts` ("Select all with extension *.pdf"), falling back to its static name
  when the cursor row has no kind. ❗ The command itself never reads that store — it re-reads the cursor row when it
  runs — so a stale label can't change what gets selected (`file-explorer/pane/DETAILS.md` § Select all of the same
  kind).

**Rust's menu labels are a SEPARATE mechanism, and stay that way.** Native menus never read `Command.name`: they resolve
through Rust's own `menu_t` catalog, and `Label::License` (`src-tauri/src/menu/menu_spec.rs`) is a second, independently
maintained dynamic-label path that happens to track the same license state. Generalizing the frontend's naming, as
`nameKey` thunks and `displayName` just did, fixed nothing on that side. Keeping them separate is deliberate
(`docs/specs/select-same-kind.md` § Not in scope); the duality is documented rather than merged.

## Adding a command (full steps)

1. Add the id to `COMMAND_IDS` in `command-ids.ts`. Skipping this makes the registry entry a compile error (`Command.id`
   is `CommandId`).
2. Add a `CommandSource` entry to the matching `sources/*.ts` scope file (with a `nameKey`, and a `descriptionKey` if it
   needs help text), and add the matching `commands.<idish>.label` / `.description` to `messages/en/commands.json` with
   a `@key` description, then run `pnpm intl:keys`. Skipping the entry fails the set-equality test in
   `command-registry.test.ts`; a `nameKey` with no catalog entry fails the key-gen build. The entry's `whileDialogOpen`
   is required: `BLOCKED_BY_DIALOGS` unless the command leaves the panes and the main window's UI alone. An opt-out
   (`runsOverDialogs(reason)`, or `IN_TEXT_INPUTS_ONLY`) also goes into the pinned lists in `command-registry.test.ts`.
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
6. Pin its English in `command-registry.parity.test.ts` (`EXPECTED_NAMES`, plus `EXPECTED_DESCRIPTIONS` when it has a
   description). The test asserts every registry command appears there, so a new command fails it until pinned.
7. For a native menu item: add the id const and both map directions in `src-tauri/src/menu/command_map.rs`, add its row
   to `menu/menu_bar.rs` for the menu bar (rows are tracked by default, which is what lets accelerator rebinding reach
   them; update the pinned bar in `menu/menu_bar_test.rs` alongside) or build it in `menu/menu_structure.rs` for the
   right-click menu, and add the id to `menuCommands` in `shortcuts-store.ts`. `rust-command-id-drift.test.ts` fails if
   `menu_id_to_command` emits an unknown id, or if `menuCommands` and the reverse map disagree.

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

Native menu clicks, keyboard shortcuts, palette rows, the explorer's own controls, the mouse's side buttons, and MCP all
route through `handleCommandExecute(commandId, ctx)` in `routes/(main)/command-dispatch.ts`, each down its own road
(`ctx.source`), and all pass the same dialog gate (`routes/(main)/DETAILS.md` § The dialog gate). The Rust
`on_menu_event` handler maps menu item ids to command registry ids and emits a single `"execute-command"` Tauri event;
the frontend listens and dispatches it down the menu's road.

Exception: `CheckMenuItem`s (show hidden files, view modes) keep separate handling to avoid double-toggle. Close tab
(⌘W) has special logic to close focused non-main windows.

`view.showHidden` flips the `listing.showHiddenFiles` setting and stops there. That's still the local-first path the
`toggles hidden file visibility` e2e spec needs: `setSetting` writes its in-memory cache synchronously, so both panes'
re-fetch effects land in the next Svelte tick, while the disk save is debounced and the native `CheckMenuItem` follows
from `settings-applier.ts`. Routing the toggle through Rust instead (an IPC + Tauri-event + Svelte-effect hop between
keystroke and DOM) is what flaked that spec under slow-lane CPU contention. The native menu accelerator path travels the
other way, `on_menu_event` → `settings-changed` → the `listener-setup.ts` listener, which writes the same setting.

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
