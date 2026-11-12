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
  shortcuts: PlatformShortcuts // { macos, windows, linux }, each in that OS's spelling (platform-shortcuts.ts)
  nativeShortcut?: true // the app menu owns behavior AND accelerator where there is one (`ownedByAppMenu`)
  fixedKey?: true // key hardcoded in the owning component, never reads the store; read-only in the editor
  status?: 'alpha' | 'beta' // stability badge in the palette row; derive via getBadgeStatus() from #lib/feature-status
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
  'tab.mcpAction':
    | { pane: PaneId; action: McpTabAction; tabId?: string; pinned?: boolean; mcpRequestId?: undefined }
    | McpTabMoveArgs
    | McpTabOpenArgs // `move` carries toPane / toIndex; both of those carry the reply's round-trip id
  'dialog.confirm': { type: ConfirmDialogType; onConflict?: string }
}
interface CommandArgsOptionalOverrides {
  'file.copy': { pane?: PaneId; onConflict?: string; mcpRequestId?: string; initiator: Initiator }
  'file.move': { pane?: PaneId; onConflict?: string; mcpRequestId?: string; initiator: Initiator }
  'file.delete': { pane?: PaneId; permanent?: boolean; mcpRequestId?: string; initiator: Initiator }
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

`ConfirmDialogType` covers `transfer-confirmation`, `delete-confirmation`, `rename`, `mkdir-confirmation`, and
`new-file-confirmation` from the MCP `dialog_confirm` tool, plus `archive-password` from `unlock`, which stores the
password on the backend first and then confirms the prompt to trigger the frontend's follow-up. Reusing `dialog.confirm`
for that last one is deliberate: it's the existing "programmatically answer an open dialog" seam, so the tool needs no
command id of its own.

`CommandScope` is a union of string literals: `'App'`, `'Main window'`, `'Main window/File list'`,
`'Main window/Brief mode'`, `'Main window/Full mode'`, `'Main window/Servers'`, `'Main window/Places'`,
`'Main window/Volume chooser'`, `'Main window/Favorites menu'`, `'Main window/Error screen'`,
`'Main window/Multi-rename'`, `'Main window/Operation log'`, `'Main window/Ask Cmdr'`, `'Main window/Search dialog'`
(the Search and Select files dialogs' fixed keys, `sources/search-dialog.ts`; its chain skips `Main window`, like Ask
Cmdr's), `'About window'`, `'Operation queue window'`, `'Viewer window'`, `'Onboarding'`, `'Command palette'`. Scope is
documentation-only.

❗ **A NEW scope is three places**: the `CommandScope` union here, its ancestry chain in `shortcuts/scope-hierarchy.ts`
(without one, `getActiveScopes` returns `[]` and its commands can never conflict with anything), and a row in
`settings/sections/keyboard-shortcuts-grouping.ts`'s `scopeOrder` — miss that last one and every command on the scope
silently vanishes from the rebinding UI, which its exhaustiveness test catches.

## Command registry

The registry data is split by top-level scope into `sources/*.ts` (`app`, `main-window`, `file-list`, `multi-rename`,
`browsers`, `mcp`, `about-window`, `queue-window`, `viewer-window`, `command-palette`, `search-dialog`), each exporting
a `CommandSource[]`. `command-registry.ts` concatenates them in authoring order into `commandSources` (order is
load-bearing: it drives palette listing and shortcut conflict resolution) and holds all the logic. Most commands are
palette-visible; the rest are `showInPalette: false`: low-level navigation and MCP-only per-pane commands.
`app.commandPalette` is `showInPalette: false` (opening the palette from inside itself makes no sense).
`getPaletteCommands(holds?)` is the only filter exported; `commands` (the full array) is exported too, for shortcut
documentation and Settings panes.

**Conditional palette rows.** A source may carry a `paletteCondition` (a `PaletteCondition` member:
`focusedPaneSharesLinks`, or `trashUndoAvailable`, which hides `edit.undoTrash` exactly while its Edit menu item is
greyed out), and the palette shows it only while that holds. `CommandPalette.svelte` answers each one through its
`PALETTE_CONDITIONS` record when it searches, so a new member doesn't compile until it's answered there. Without
`holds`, `getPaletteCommands` counts every conditional row as shown, which is what pruning stale recents wants. Every
other surface (Settings, help, MCP) lists the command regardless, and its handler still checks: the palette knows the
pane, not the cursor row.

`command-ids.ts::COMMAND_IDS` is also the backend's command-breadcrumb vocabulary. `src-tauri/build.rs` parses the tuple
into an `OUT_DIR` Rust slice, and `error_reporter/breadcrumbs.rs` retains a command event only when its id is in that
generated slice. The generator fails on unrecognized tuple syntax instead of producing a partial allowlist, and the Rust
drift test compares the compiled slice back to the authored tuple. This keeps one registry while closing the untrusted
IPC boundary; the generated Specta payload remains `commandId: string`, and the frontend wrapper narrows it to
`CommandId`.

No source reads the platform or a capability at module load: a name that differs per OS is a catalog platform block read
per access (§ i18n), a command an OS lacks is gated by `needsCapability` at call time, and default keys are a three-way
table (§ "Default keys per platform").

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
- **Three commands are named after an OS feature** (`file.showInFinder` / `file.getInfo` / `file.quickLook`). Each reads
  ONE key (`commands.fileShowInFileManager.label`, `commands.fileGetInfo.label`, `commands.fileQuickLook.label`) whose
  value is a platform block (`../intl/DETAILS.md` § Platform variants), so the name is resolved per read and no source
  asks `isMacOS()`. The command id `file.showInFinder` stays: ids are persisted in shortcut customizations.
- **A `nameKey` thunk** (`() => MessageKey`) picks between catalog keys on live state, resolved on every read.
  `app.licenseKey` is the one user: it reads `hasExistingLicense` (in `sources/app.ts`, beside the entry) and resolves
  `commands.appLicenseKey.seeDetails.label` or `…enterKey.label`. `updateLicenseCommandName(hasLicense)` flips the flag
  — it doesn't rewrite English in place — keeping the palette label in step with the native menu.
- **A `displayName` resolver** composes the palette label outright, when no fixed set of catalog keys covers it.
  `edit.undoTrash` names the trash it would put back (`trashUndoCommandLabel`, `file-operations/delete/trash-undo.ts`,
  the same state that relabels the Edit menu item). `selection.selectSameKind` renders whatever the focused pane
  published into `file-explorer/pane/same-kind-target.svelte.ts` ("Select all with extension *.pdf"), falling back to
  its static name when the cursor row has no kind. ❗ The command itself never reads that store — it re-reads the cursor
  row when it runs — so a stale label can't change what gets selected (`file-explorer/pane/DETAILS.md` § Select all of
  the same kind).

**Rust's menu labels are a SEPARATE mechanism, and stay that way.** Native menus never read `Command.name`: they resolve
through Rust's own `menu_t` catalog, and `Label::License` (`crates/cmdr-menu/src/spec.rs`) is a second, independently
maintained dynamic-label path that happens to track the same license state. Generalizing the frontend's naming, as
`nameKey` thunks and `displayName` just did, fixed nothing on that side. Keeping them separate is deliberate
(`docs/specs/select-same-kind.md` § Not in scope); the duality is documented rather than merged.

### OS-feature commands (`needsCapability`)

A command that drives an OS feature (Get info, Quick Look, Open terminal here, the cloud-file actions, color tags, the
`cmdr` terminal command) declares `needsCapability: '<capability>'` and is authored as if every OS had it
(`showInPalette: true`, its real default keys). Where the capability is off:

- No surface lists it: `listedCommands()` is the registry minus those, and the palette (`getPaletteCommands`), the
  Keyboard shortcuts section and its search (`searchAllCommands`), the help window (`ShortcutsList.svelte`), and the MCP
  settings resource's shortcuts all read it. Dispatch, conflict detection, and the id guards keep the whole `commands`
  array.
- It gets no default key (`defaultShortcuts`, asked on every read), so ⌘I or ⇧Space stays free for a user binding.

Why one declaration over `showInPalette: hasCapability(…)` plus a ternary on `shortcuts`: those hid the command from the
palette only, so Linux's Keyboard shortcuts section listed "File properties" and "Preview" rows that did nothing. The
menu bar's matching rows are `macos_only` in `crates/cmdr-menu/src/bar.rs`, kept in step per OS by
`apps/desktop/src-tauri/src/menu/capability_rows_test.rs`.

**Decision/Why: a gated command's label keeps its per-OS block.** With `needsCapability`, nothing shows "File
properties" or "Preview" off macOS today, so `commands.fileGetInfo.label` and `commands.fileQuickLook.label` could be
plain macOS values. They stay blocks: Windows has a native Properties dialog, so `getInfo` may turn on there, and the
branch is ready when it does.

### Default keys per platform

A command's `shortcuts` is three lists (`PlatformShortcuts`: `macos`, `windows`, `linux`), each in that platform's own
canonical spelling, the one `formatKeyCombo` writes there. Nothing converts a combo at read time: the store reads
`defaultShortcuts(commandId, platform = getPlatform())` (`command-registry.ts`), a lookup in a per-platform table built
on first read. That function also owns the `needsCapability` rule, asked on every call outside the cached table, so a
test that switches capabilities never reads a stale answer.

Sources write the lists with the constructors in `platform-shortcuts.ts`, each naming its rule, in macOS glyph spelling
so the macOS arm is the literal in the source:

- `sameOnAll('F5', '⇧F6')`: the same physical chord everywhere (Alt and Shift words off macOS). Refuses ⌘ and ⌃.
  `sameOnAll()` is "no key".
- `cmdOrCtrl('⌘⇧P')`: ⌘ on macOS, Ctrl on Windows and Linux. Needs ⌘, refuses ⌃.
- `macOnly('⌘H')`: keys on macOS, none elsewhere.
- `concatShortcuts(sameOnAll('Backspace'), cmdOrCtrl('⌘↑'))`: joins parts arm by arm, in order; throws on a repeat.
- The literal `{ macos, windows, linux }`, for a list holding ⌃ or keys that truly differ per platform.

Decision: refuse at the type level (template-literal types) and by throwing at load. Why: the old one-way converter put
⌘ and ⌃ both on Ctrl, so `⌘D` and `⌃D` landed on one key with nobody deciding; with the refusals that mistake can't be
written. The helpers write the off-macOS arms in canonical `Ctrl+Alt+Shift+Super` order whatever the authored order.

### The default keymap snapshot

`default-keymap.test.ts` renders the default keys a user gets on each OS into `__snapshots__/default-keymap.macos.txt`,
`.windows.txt`, and `.linux.txt`: one `<command id>\t<combo>, <combo>` line per command, in registry order, a keyless
command as an empty list, a command the OS can't offer left out (under `withCapabilitiesPossibleOn(os)`,
`../platform/DETAILS.md` § "Test helpers"). It reads through `getDefaultShortcuts`, so the registry's internals can
change under it. Every key change, and every added or removed command, is a reviewed line diff there; ❌ never refresh a
snapshot without reading which lines moved and why.

The same test writes the menu bar's side of the parity check, `crates/cmdr-menu/src/registry_default_keys.tsv`: each
`menuCommands` entry's first default key per OS, under the same fixture. `cmdr-menu`'s `bar_parity_test.rs` reads it and
fails a bar row whose accelerator isn't what that key registers (`crates/cmdr-menu/DETAILS.md` § "Parity with the
registry"). So a moved key that has a menu row moves three things in one change: the source, this snapshot (and the TSV,
`vitest -u`), and the row in `bar.rs`.

The other per-platform keymap tests take one of two inputs. What a user on that OS gets (conflicts in
`../shortcuts/registry-conflicts.test.ts`, `command-reachability.test.ts`) runs under the per-OS capability fixture.
What the data says (`../shortcuts/shortcut-vocabulary.test.ts`, and `../shortcuts/keymap-invariants.test.ts`: no Windows
or Linux default is `Ctrl+Alt+<letter>`, which AltGr layouts type with, or holds `Super`, no two commands in overlapping
scopes share a raw key on any OS, and a Linux command whose first key a desktop takes has a second one no desktop takes)
walks every command's raw arm, capability ignored, so a gated command's keys are checked before its capability flips on.
The keys themselves are `docs/notes/windows-port/keymap-decisions.md`'s; a change to one starts there.

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
7. For a native menu item: add the id const and both map directions in `crates/cmdr-menu/src/ids.rs`, add its row to
   `crates/cmdr-menu/src/bar.rs` for the menu bar (rows are tracked by default, which is what lets accelerator rebinding
   reach them; update the pinned bar in `crates/cmdr-menu/src/bar_test.rs` and its enabled states in
   `crates/cmdr-menu/src/bar_states_test.rs` alongside) or add its row to `crates/cmdr-menu/src/context/` for the
   right-click menu, and add the id to `menuCommands` in `shortcuts-store.ts`. `rust-command-id-drift.test.ts` fails if
   `menu_id_to_command` emits an unknown id, or if `menuCommands` and the reverse map disagree. Then refresh the parity
   TSV (`vitest -u default-keymap`): `bar_parity_test.rs` fails until the row's accelerator on each OS is what the
   command's first default key registers there.

## Fuzzy search

`searchCommands(query, recentCommandIds?)` wraps `@leeoniya/ufuzzy` through the shared engine in
`#lib/utils/unicode-fuzzy.ts` (`foldForSearch` + `fuzzySearch`, also used by Settings search and the recent-items
dropdown):

- Empty query: recents first (filtered through `getPaletteCommands` to drop stale ids), then the rest of
  `getPaletteCommands()` in registry order.
- Non-empty query: the haystack is each command's folded `displayName` plus its keywords; `fuzzySearch(haystack, query)`
  yields ranked `CommandMatch[]` (the `recents` argument is ignored).
- Keywords are English code literals, the same on every OS, so they carry the words another OS (or an older name) uses:
  `nav.goHome` keeps `home` because Windows names it "Go to user folder", and the trash commands carry `recycle bin`.
  Labels change per OS through their catalog value (`docs/style-guide.md` § "Platform vocabulary");
  `fuzzy-search.platform.test.ts` pins both, in the palette and the shortcuts editor.

The engine's config: every script counts as letters (a Russian or Chinese command name is findable), accents fold on
both sides (`tema` finds `Téma`), `intraMode: 1` (typo-tolerant within-word fuzzy, "tyoe" → "type") except for a CJK
query, which must match exactly, and `interIns: 3` (max 3 inserted chars between matched chars, to avoid nonsensical
matches on a short list). Folding is one code unit in, one out, so `info.ranges` index `displayName` directly: a flat
`[start, end, start, end, …]` array with exclusive `end`, unpacked into per-char indices for `matchedIndices`. Pinned by
`fuzzy-search.unicode.test.ts`.

`searchAllCommands(query)` runs the same matcher over every command this OS offers (`listedCommands()`, including
`showInPalette: false` entries), with no recents handling (empty query → everything in registry order). It exists for
surfaces whose result set is the whole registry: the shortcuts editor renders and rebinds every command, so its name
search (and the `getMatchingSections` sidebar highlight in settings-search) must cover the same set. The command palette
itself stays on `searchCommands`.

## Unified dispatch

Native menu clicks, keyboard shortcuts, palette rows, the explorer's own controls, the mouse's side buttons, and MCP all
route through `handleCommandExecute(commandId, ctx)` in `routes/(main)/command-dispatch.ts`, each down its own road
(`ctx.source`), and all pass the same dialog gate (`routes/(main)/DETAILS.md` § The dialog gate). The Rust
`on_menu_event` handler maps menu item ids to command registry ids and emits a single `"execute-command"` Tauri event;
the frontend listens and dispatches it down the menu's road.

An open dialog can claim a menu command whose accelerator is one of its own keys (`menu-claims.ts`: the Multi-rename
sheet's F2 is File > Rename's, the Operation log's ⌘A / ⌘⇧A are Edit > Select all / Deselect all's; a claim answering
`false` passes that fire on). The core runs the claim for that command's menu road, ahead of the dialog gate;
`routes/(main)/DETAILS.md` § The dialog gate.

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
- **`nativeShortcut: true` for the app menu's four, and "native" is a per-OS fact.** Where the OS has an app menu
  (`appMenu`, macOS), they're handled by `PredefinedMenuItems` with native selectors (`terminate:`, `hide:`, …), and the
  native accelerators handle their keys; including them in the JS dispatch map would double-execute, and Cmdr can
  neither rebind nor intercept them. `ownedByAppMenu(command)` (`command-registry.ts`) is the one place that decides it:
  `resolveCommand` makes a native command's `showInPalette` a getter on it (the dispatch map takes the palette's
  commands), and the store's `isNativeShortcutCommand` (the read-only editor rows, the mutators' refusal, the load-time
  drop, the conflict banner) asks it. Without an app menu, `app.hide` / `app.hideOthers` / `app.showAll` don't exist
  (`needsCapability: 'appMenu'`, `APP_MENU_ONLY_COMMAND_IDS`, which `DISPATCH_EXEMPT_IDS` sources its Family-1 list
  from), and `app.quit` is an ordinary, rebindable command with no default key whose handler calls the backend's
  `quit_app`: the same path closing the main window takes, through the quit gate (`src-tauri/src/quit/CLAUDE.md`). Why a
  capability, not `isMacOS()`: it names the surface (an app menu that owns these), and the per-OS fixture pins it to the
  Rust arms.
- **`view.toggleFullScreen` exists on every OS that offers full screen (`fullScreen`)**, with `F11` on Windows and Linux
  and no key on macOS, where AppKit's own View menu item keeps its key and its place (so the bar has no Cmdr row there).
  It calls the window's own `setFullscreen`, the same native full screen `escape-key.ts` leaves.
- **uFuzzy over Fuse.js / fzf-for-js.** uFuzzy is optimized for short query phrases against short-to-medium phrases,
  pure TypeScript with no dependencies, handles typos, and returns match ranges for highlighting. Fuse.js is heavier and
  slower at this scale.
- **uFuzzy instances are module-level singletons** (in `unicode-fuzzy.ts`). Creating an instance compiles regex from
  config; doing it once at import avoids repeated setup per search. The config never changes at runtime.
- **`commands` is a plain array, not a `Map`.** It's ~110 items; `getPaletteCommands()` filters on each call and uFuzzy
  needs an array of strings anyway. Indexing by id would add complexity for no measurable gain at this scale. There's no
  `getCommandById()`: lookup by id happens in the shortcuts system (its own reverse map) and in `handleCommandExecute`
  (the flat handler record). The module stays a registry and a search engine, not a command bus.
