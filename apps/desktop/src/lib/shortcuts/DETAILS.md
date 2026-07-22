# Keyboard shortcuts details

Pull-tier docs for `apps/desktop/src/lib/shortcuts/`: architecture, flows, and decision rationale. Must-know invariants
and gotchas live in `CLAUDE.md`.

## Purpose

Customizable keyboard shortcuts for all Cmdr commands. Users can edit, add, remove, and reset shortcuts through the
Settings window. MCP tools can also modify shortcuts programmatically.

## Architecture

### Command registry (`../commands/command-registry.ts`)

Lives in the sibling `src/lib/commands/` directory (which has its own CLAUDE.md). Defines all commands with default
shortcuts:

```typescript
{
    id: 'nav.parent',
    name: 'Go to parent folder',
    scope: 'Main window/File list',
    shortcuts: ['Backspace', '⌘↑'],
    showInPalette: true
}
```

### Shortcuts store (`shortcuts-store.ts`)

- Persists to `shortcuts.json` in app data directory
- Delta-only storage: only customizations, not defaults
- Empty array means "all shortcuts removed"
- Missing command means "use defaults from registry"
- Saves are serialized: every mutator fires `void saveToStore()`, and the save chains onto the previous one
  (`saveChain`) so two rapid mutations can't interleave their reconcile/delete/set/save loops over the shared store.
- `saveToStore` reconciles disk against the in-memory map on every write: it deletes any `shortcut:*` key whose command
  no longer has a map entry, then writes the current entries. So when `resetShortcut` / `cleanupIfMatchesDefaults` drops
  an entry (e.g. a custom that's been edited back to defaults, or `app.showAll`'s `[]` default after removing an added
  shortcut), the stale disk key goes too. Without this the old value resurrects at next load. `resetAllShortcuts` relies
  on the same step — it just clears the map and saves.
- `initializeShortcuts` loads any array, including `[]`, and skips only non-array garbage (`Array.isArray` check). The
  empty array is the persisted "removed all shortcuts" state, so it must survive a reload, not be treated as absent.
- `initializeShortcuts` notifies `onShortcutChange` listeners for every loaded customization, so components that mounted
  before the async init finished (reactive shortcut reads, the dispatch map) catch up instead of showing registry
  defaults. The notification path also syncs menu accelerators (`updateMenuAccelerator` no-ops for commands without a
  menu item).

### macOS-native commands are not customizable

The four `nativeShortcut` commands (`app.quit`/`hide`/`hideOthers`/`showAll`, exported as `NATIVE_SHORTCUT_COMMAND_IDS`
from `$lib/commands/command-registry`) are macOS `PredefinedMenuItem`s: AppKit owns BOTH the behavior and the
accelerator, so any persisted customization is a pure illusion (it can't disable the OS accelerator and dispatches into
a void). The store enforces this at its boundary, the real seam for MCP events and any future caller:

- **Load drops them.** `initializeShortcuts` skips any persisted `shortcut:<native id>` entry (David's dev
  `shortcuts.json` carries `app.hide: []` from testing). Not loading it means the map has no entry, the registry default
  applies, and the next `saveToStore` reconcile deletes the stale disk key.
- **Mutators no-op.** `setShortcut` / `addShortcut` / `removeShortcut` early-return with a `log.warn` for native
  commands (no write, no `notifyListeners`, no cross-window emit). `resetShortcut` stays permissive — it only ever
  DELETES a custom entry, never writes the illusion, so it can usefully clear a leaked native customization.
- `isNativeShortcutCommand(commandId)` is the exported predicate; the editor uses it to render native rows read-only.
- MCP shortcut edits route through these same mutators (`mcp-shortcuts-listener.ts`), so they inherit the guard for
  free.

### Fixed-key commands are not customizable either

The Family-2/3 dispatch-exempt commands (exported as `FIXED_KEY_COMMAND_IDS` from `$lib/commands/command-registry`,
flagged `fixedKey: true` in the registry) have their keys hardcoded in the owning component's keydown handler (FilePane
arrows, palette navigation, modal Enter/Escape) — they never consult this store, so a customization would be a no-op
illusion: the new key wouldn't fire and the built-in key wouldn't release. Same boundary rules as the native commands:
load drops persisted entries, the mutators no-op with a `log.warn`, `resetShortcut` stays permissive, and
`isFixedKeyCommand(commandId)` is the exported predicate the editor uses to render these rows read-only ("Fixed" badge).
If a fixed command's handler is ever rewired to read effective shortcuts, remove it from `FIXED_KEY_COMMAND_IDS` and it
becomes rebindable everywhere at once.

### Cross-window propagation (`shortcuts:changed`)

The store is per-webview module state, so a rebind in the Settings window must reach the main window's `customShortcuts`
map, its `onShortcutChange` consumers (reactive chips, F-key bar, palette, sort tooltips), AND its dispatch map —
otherwise they stay stale until restart and the new key doesn't actually work. This mirrors settings-store's
`settings:changed` pattern.

- **Every mutation** (`setShortcut` / `addShortcut` / `removeShortcut` / `resetShortcut` / `resetAllShortcuts`) emits a
  `shortcuts:changed` Tauri event AFTER updating local state and saving. A single-command change carries
  `{ senderId, commandId, shortcuts }` where `shortcuts` is the new custom list, or `null` when the command reverted to
  its registry default (cleanup/reset dropped the map entry). `resetAllShortcuts` emits `{ senderId, resetAll: true }`.
- **`initializeShortcuts` installs a listener** (`setupCrossWindowListener`) that, on a remote change, updates the local
  map directly and calls `notifyListeners(commandId)` so reactive consumers + the dispatch map rebuild. It does NOT save
  to disk (the writer already saved) and does NOT re-emit (that would loop). A reset-all clears the whole map and
  notifies each previously-customized id (computed from the map before clearing). The listener is installed once per
  window — guarded by both `initialized` and a non-null `crossWindowUnlisten`, so a re-init can't double-subscribe.
- **Loop guard:** each window stamps every emit with a per-window `SENDER_ID` (a `crypto.randomUUID()` generated once at
  module load); the listener drops any event whose `senderId` matches its own. Settings-store dedupes instead via a
  strict-equality idempotency guard on the cached value, but shortcut payloads are arrays that arrive as fresh
  references (nothing to compare by identity), so the explicit sender id is the clean guard here.
- **The viewer never subscribes.** It's capability-restricted and never calls `initializeShortcuts`, so no
  `shortcuts:changed` listener is installed there. Importing `shortcuts-store` at module eval (the viewer pulls it in
  transitively via the literal-mode `ShortcutChip`) only runs `crypto.randomUUID()` and declares functions — no
  `listen()` call — so the capability boundary holds.

### Reactive reads (`reactive-shortcuts.svelte.ts`)

Two readers over one module-level `$state` version (bumped on every `onShortcutChange`):

- `getEffectiveShortcutsReactive(commandId)` returns the full effective list (the command palette shows up to three).
  Param is typed `CommandId`, not loose `string`. Returns a fresh array on every call (the underlying
  `getEffectiveShortcuts` copies the store's data), so consumers can't mutate the store — don't cache the reference.
- `getFirstShortcutReactive(commandId)` is `[0]` of that list (the one menus and inline `ShortcutChip`s show).

Both subscribe `$derived`/`$effect` consumers to shortcut changes, so a rebind updates the UI live. Use them for
long-lived UI that displays a shortcut (tooltips, hints, the palette, `ShortcutChip`). One-off reads at event time
(toasts, context menus) keep calling `getEffectiveShortcuts` directly — a toast deliberately snapshots the binding it
was created with.

### Deep link to a command's row (`../settings/settings-window.ts`)

A clickable `ShortcutChip` (and any other "customize this shortcut" affordance) deep-links to the command's row in
Settings > Keyboard shortcuts via `openShortcutCustomization(commandId)`. That helper calls
`openSettingsWindow(['Keyboard shortcuts'], shortcutAnchorId(commandId))`. The anchor-id convention
`shortcut-<commandId>` lives as the paired `shortcutAnchorId` / `commandIdFromShortcutAnchor` functions in
`settings-window.ts` so the writer (the helper) and the readers (the section's row id, the settings page's arrival
handler) can't drift. The settings side scrolls the row into its nested list and flashes it — see
`../settings/sections/DETAILS.md` § "Deep-link arrival".

### Scope hierarchy (`scope-hierarchy.ts`)

`CommandScope` is the single scope vocabulary — `scope-hierarchy.ts` re-exports it from `../commands/types.ts`, so the
registry's `scope` strings and the hierarchy keys are the same type. `scopeHierarchy` holds each scope's ancestry chain
(most specific first); two scopes "overlap" (and so can conflict) when one chain contains the other.

The chains mirror what renders together in the app:

- `App` → global, always active.
- `Main window` → inherits `App`.
- `Main window/File list` → inherits `Main window` → `App`.
- `Main window/Brief mode` and `Main window/Full mode` → sit UNDER `Main window/File list` (→ `Main window` → `App`).
  The file list renders in both view modes, so a mode-scoped key genuinely collides with a File-list key. Brief and Full
  stay siblings (neither chain contains the other), so they don't conflict with each other — the registry binds `←`/`→`
  in both on purpose, and the modes never coexist.
- `Main window/Network`, `Main window/Share browser`, `Main window/Volume chooser` → siblings of `Main window/File list`
  (under `Main window` → `App`, but not under the file list). A pane shows one of them INSTEAD of the file list, so
  their keys don't collide with File-list keys.
- `Command palette` → inherits `Main window` → `App` (it overlays the main window).
- `About window` and `Onboarding` → inherit `App` only (standalone/modal contexts).

`getActiveScopes(unknown)` returns `[]`, and `scopesOverlap` treats an empty chain as non-overlapping, so a typo'd scope
silently can't conflict rather than throwing.

### Conflict detection (`conflict-detector.ts`)

Two commands conflict if:

1. They share the same key combo, AND
2. Their scopes overlap (via hierarchy)

Example: `⌘N` in `Main window/File list` and `⌘N` in `Main window` conflict because the File-list chain contains
`Main window`. Two `Main window/File list` commands sharing a combo also conflict (same scope). `←` in
`Main window/Brief mode` and `←` in `Main window/Full mode` do NOT, because Brief and Full are siblings.

### Key capture (`key-capture.ts`)

Platform-specific formatting:

- macOS: `⌘⇧P` (symbols)
- Windows/Linux: `Ctrl+Shift+P` (names)

No normalization: shortcuts are stored exactly as displayed.

### MCP integration (`mcp-shortcuts-listener.ts`)

Main window listens for MCP events to modify shortcuts even when settings window is closed. This allows AI agents to
customize shortcuts on the fly.

### Centralized dispatch (`shortcut-dispatch.ts`)

Builds a reverse lookup `Map<shortcutString, commandId>` for Tier 1 commands (those with `showInPalette: true` plus
`app.commandPalette`). On every keypress, `handleGlobalKeyDown()` in `+page.svelte` calls `formatKeyCombo(e)` and
`lookupCommand()` to find a matching command, then routes through `handleCommandExecute()` -- the same path used by the
command palette and MCP events. Rebuilds automatically when custom shortcuts change via `onShortcutChange`.

Tier 2 commands (arrows, Space, Enter, Backspace, etc.) are not in the dispatch map. Unmatched keypresses propagate
normally to component-level handlers in DualPaneExplorer and FilePane.

Typing wins in text inputs: before the lookup, `handleGlobalKeyDown` bails when focus is in a text-editing element and
the combo `isTypingKeyCombo` (no ⌘/⌃/⌥, not an F-key or Escape — shift-only counts as typing). Without this, a bare-key
Tier 1 binding (Tab → switch pane) fires mid-typing in any in-pane text input that forgets to `stopPropagation`. The
guard is central so new inputs are protected by default; `NetworkLoginForm`'s own Tab shielding remains as before.

### Keyboard shortcuts help window (read-only)

A separate window (Help > Keyboard shortcuts, command `help.openShortcuts`) lists every command's shortcuts as a
read-only reference. Editing stays in Settings; this window only links there ("Edit shortcuts").

The "Edit shortcuts" links do NOT call `openSettingsWindow` directly. Tauri capabilities are checked against the calling
window, and `openSettingsWindow` needs `get-all-windows` + `create-webview-window` + `available-monitors` +
`set-effects`. Granting all that to a read-only help window is the privilege creep the per-window capability split
exists to prevent. Instead the links emit the shared `open-settings` event (`requestOpenSettings('Keyboard shortcuts')`)
that the main window already handles via `onOpenSettings` (the same channel the MCP `dialog open settings` path uses).
The main window owns the window-creation perms; the help window needs only `core:event:default`.

- **Opener** (`shortcuts-window.ts`): a singleton `WebviewWindow` on the `/shortcuts` route (focuses if already open,
  via the `focus-self` event like Settings). Narrow and tall (~1:3), both dimensions scaled by the effective text size,
  the height capped to the target monitor so a tall window never spawns off-screen. Resizable; the list scrolls.
- **Route** (`routes/shortcuts/+page.svelte`): the window shell. Inits settings + shortcuts stores, accent color, and
  text size (so it tracks the app-wide font size). Escape closes (deferred past the event tick, like Settings/Viewer).
  Holds the "Hide features with no shortcut" checkbox state (off by default; hides commands whose effective list is
  empty).
- **List** (`ShortcutsList.svelte`): one `SectionCard` per `CommandScope`, reusing the Settings editor's
  `groupCommandsByScope` (same grouping + order). It lists ALL registry commands, including the non-customizable
  native/fixed ones (they just render plain chips). Live-syncs via `onShortcutChange` (cross-window Settings edits ride
  `shortcuts:changed`): a counter bump re-derives the groups and re-keys the rows so each row's diff recomputes.
- **Diff** (`shortcut-diff.ts`, pure + unit-tested): `diffShortcuts(defaults, effective)` returns one chip per key,
  status `active` (in both), `added` (effective-only: user-added/replaced, rendered bold green with an "Added" tooltip),
  or `disabled` (default-only: turned off, rendered dimmed + struck with a "Disabled" tooltip). One set-diff covers
  extra / replaced / removed. The "added" green is the themed `--color-allow` token (AA in both modes).

## Key decisions

### Why platform-specific storage?

Cross-platform normalization is complex and error-prone. Storing shortcuts as display strings keeps the code simple.
Each platform captures and matches shortcuts in its native format.

### Why delta-only persistence?

Default shortcuts live in `command-registry.ts` and are baked into the app. Storing them in `shortcuts.json` would
duplicate data and make defaults harder to change. Only customizations are stored, keeping the file small and clear.

### Why scope hierarchy?

Allows the same key combo to have different meanings in different contexts without warnings. `⌘N` can be "New file" in
one window and "New folder" in another if their scopes don't overlap.

### Why 500ms confirmation delay?

Users often mis-press keys or change their mind mid-capture. The delay lets them press multiple combos rapidly and only
the final one (after 500ms of silence) is saved. Prevents accidental captures.

### ⌘J binds to "Go to latest download" (not Finder's "View Options")

**Decision.** The in-app `⌘J` shortcut triggers `downloads.goToLatest`, jumping the focused pane to the most recent file
in `~/Downloads`. We accept the deviation from Finder, which uses `⌘J` for the "View Options" inspector.

**Why.** Cmdr's view-mode toggles already live on dedicated single-key shortcuts (`⌘1` Full, `⌘2` Brief, plus the inline
view-mode toggle and the appearance controls under `⌘,`), so we're not displacing an existing Cmdr action — we're
choosing not to mirror Finder for this one binding. `⌘J` is short, easy to reach, and intuitive as "Jump." Chrome's
Downloads-tab shortcut is `⌘⇧J`, so there's no collision with the most common downloads workflow either. Finder migrants
pick up the new muscle memory within a few uses because the view-mode controls they expect are still keyboard-accessible
on dedicated keys.

### Why two tiers (action vs navigation commands)?

Tier 1 commands (~20 "action" commands like F-keys and Cmd+ combos) go through centralized dispatch. Tier 2 commands
(~40 navigation keys like arrows, Space, Enter, Backspace) stay in component-level handlers. Centralizing Tier 2 would
require a `when`-clause system (like VS Code's `fileListFocused && !renameActive`) because these keys mean different
things depending on context (file list vs volume chooser vs command palette). That's a significant architecture
investment with low payoff for Cmdr's current scope. Tier 1 commands are the ones where the "two sources of truth" bug
hurt (adding F8 to the registry but forgetting to add it to the keydown handler).

### Why separate MCP listener for main window?

`mcp-shortcuts-listener.ts` listens for the backend's `mcp-shortcuts-set` / `-remove` / `-reset` events (a distinct
backend→main channel) so MCP tools can rebind even when the Settings window is closed. It calls the same store mutators
(`setShortcut` / `removeShortcut` / `resetShortcut`), so an MCP-driven change in the main window now also rides the
`shortcuts:changed` cross-window event to the Settings window for free — no special handling needed. The MCP listener
stays separate because its trigger (a backend event with a different payload shape) is unrelated to the window-to-window
`shortcuts:changed` channel; folding them together would conflate two transports. (If a future change makes the two
channels share a payload, revisit — but today it's not a trivial merge, so leave it.)

## Gotchas

### Modifier-key accelerators may fire twice (menu + JS)

For commands that have BOTH a native-menu accelerator (`menu/macos.rs` `Some("Shift+Space")` etc.) AND a registry
shortcut (`shortcuts: ['⇧Space']`), AppKit can leak the modifier keydown to the webview even after the menu accelerator
has fired. So `on_menu_event` emits `execute-command file.quickLook` AND `handleGlobalKeyDown` in `+page.svelte` also
sees the keydown and calls `handleCommandExecute('file.quickLook')`. **Both paths run, both reach the dispatcher.**

The race is not theoretical — observed empirically as `FE:user-action file.quickLook (×2, deduplicated)` log lines in
the Quick Look feature.

The dispatch core now swallows this class centrally: both double-fire callers tag their dispatches
(`markDispatchSource('keyboard')` in the centralized keydown path, `'menu'` in the `execute-command` listener), and
`routes/(main)/dispatch-dedup.ts` drops the same command arriving from the OTHER source within 300ms. Keying on the
source pair (instead of a bare time window) means real rapid input — double-presses, key auto-repeat — is same-source
and always passes. New toggle commands need NO per-command guard. Quick Look's older local guard
(`quickLookDispatchGuardJustFired` in `file-explorer/quick-look/quick-look-state.svelte.ts`) predates the central one
and remains as a harmless second line of defense.

### Scope hierarchy is hardcoded

`scopeHierarchy` in `scope-hierarchy.ts` is a static object. Adding a new scope requires updating the object manually.
There's no dynamic registration.

### Menu accelerator sync

When shortcuts change, `updateMenuAccelerator()` calls `invoke('update_menu_accelerator')` to update the native menu
label. The `menuCommands` array in `shortcuts-store.ts` lists all ~40 commands that have menu items. At startup,
`initializeShortcuts` pushes any persisted customizations into the menu via the `notifyListeners` path. On the Rust
side, `MenuState.items` is a `HashMap<String, MenuItemEntry>` that tracks regular `MenuItem`s by ID;
`update_menu_item_accelerator()` handles the remove/recreate/reinsert cycle. View mode CheckMenuItems still use the
separate `update_view_mode_accelerator()` path to preserve checked state.

The list can't silently drift from the Rust side anymore: the `menuCommands ↔ command_id_to_menu_id` set-equality test
in `commands/rust-command-id-drift.test.ts` parses `menu/mod.rs` and fails when a menu item is missing from
`menuCommands` (stale accelerator after rebinding) or excused without a documented reason. Five reverse-map items are
deliberately excused there because they're not registered in `MenuState.items` (an accelerator update would error);
register the item first, then move it from the exception list into `menuCommands`.

### Conflict warnings are not errors

Users can keep conflicting shortcuts. The UI shows a warning and offers to resolve, but "Keep both" is a valid choice.
At runtime the dispatch map keeps one winner per combo: the most specific scope (longest `getActiveScopes` chain) wins,
with registry declaration order as the stable tiebreaker for equal specificity. Pinned by the scope-winner tests in
`shortcut-dispatch.test.ts`; without the scope rule, an unrelated registry reorder could silently flip a kept conflict's
winner.

### Modifier-only combos are rejected

Pressing just `⌘` or `Shift` doesn't capture anything. The `isModifierKey()` check prevents this. Users must combine a
modifier with a non-modifier key.

### Key capture is client-side only

The 500ms confirmation delay happens in the UI (`KeyboardShortcutsSection.svelte`). The store layer
(`shortcuts-store.ts`) has no delay. If you call `setShortcut()` directly, it saves immediately.

### Empty array vs missing key

In `shortcuts.json`:

- `"nav.parent": []` means "user removed all shortcuts, don't use defaults"
- Missing `nav.parent` means "use defaults from registry"

These are semantically different.

An empty string is never a real shortcut. `initializeShortcuts` heals leaked `''` entries on load (an earlier settings
add flow could persist them; see `lib/settings/sections/DETAILS.md` § "The add slot is UI-only"). The healing matrix,
applied per command key:

- `[]` (length 0) → kept as-is: a genuine "removed all" state.
- `['']` / `['', '']` (non-empty, all `''`) → dropped entirely (entry not loaded), so the registry default applies. We
  must NOT collapse this to `[]` — that would wrongly suppress a default-bound command's defaults.
- `['⌘X', '']` → loaded as `['⌘X']` (the `''` filtered out).
- non-array garbage → skipped (unchanged).

Covered by the "heals leaked empty-string entries" tests in `shortcuts-store.test.ts`.

### Default shortcuts are immutable

`command-registry.ts` is compiled into the app. Changing defaults requires a new build. This is intentional: defaults
are part of the app's behavior, not user data.

### Scope overlap is transitive

If `Main window/File list` inherits `Main window` and `Main window` inherits `App`, then `Main window/File list` also
inherits `App`. `getActiveScopes()` returns the full ancestry chain, not just the immediate parent.

### No chorded shortcuts

Shortcuts are single key combos. `Ctrl+K Ctrl+C` (press K, then C) is not supported. Only `Ctrl+K` or `Ctrl+C`
individually. This simplifies capture and matching logic.
