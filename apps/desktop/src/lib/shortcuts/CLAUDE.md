# Keyboard shortcuts system

Customizable keyboard shortcuts for all Cmdr commands (edit/add/remove/reset via Settings or MCP). Defaults live in the
sibling `../commands/command-registry.ts`; only customizations persist to `shortcuts.json`.

Background on default sort-order shortcuts: `docs/notes/sort-order-shortcut-research.md`.

## Module map

- `shortcuts-store.ts` (delta-only persistence, cross-window emit, the native/fixed boundary),
  `reactive-shortcuts.svelte.ts` (reactive reads), `scope-hierarchy.ts` + `conflict-detector.ts` (overlap → conflict),
  `key-capture.ts`, `shortcut-dispatch.ts` (Tier 1 reverse lookup), `mcp-shortcuts-listener.ts`.
- Read-only help window (Help > Keyboard shortcuts): `shortcuts-window.ts` (opener), `ShortcutsList.svelte` (grouped
  list), `shortcut-diff.ts` (pure default-vs-effective diff). Route at `routes/shortcuts/`. See DETAILS.md § "Keyboard
  shortcuts help window".

## Must-knows

- **Delta-only persistence; empty array vs missing key are semantically different.** `"nav.parent": []` means "user
  removed all shortcuts, don't use defaults"; a missing key means "use registry defaults". `initializeShortcuts` loads
  `[]` (and skips only non-array garbage), so the empty array survives a reload.
- **`saveToStore` reconciles disk against the in-memory map on every write** (deletes any `shortcut:*` key with no map
  entry). Without it, a value dropped by reset/cleanup resurrects at next load. Saves are serialized via `saveChain` so
  two rapid mutations can't interleave.
- **macOS-native (`app.quit`/`hide`/`hideOthers`/`showAll`) and fixed-key (`FIXED_KEY_COMMAND_IDS`) commands are not
  customizable, enforced at the store boundary.** Load drops persisted entries, mutators (`setShortcut` / `addShortcut`
  / `removeShortcut`) no-op with `log.warn`, `resetShortcut` stays permissive (delete-only). MCP edits route through
  these same mutators, so they inherit the guard. `isNativeShortcutCommand` / `isFixedKeyCommand` are the predicates.
- **Every mutation emits `shortcuts:changed` after saving; the per-window `SENDER_ID` is the loop guard** (the listener
  drops events with its own id). The listener updates the local map and calls `notifyListeners` but does NOT save or
  re-emit. The viewer never subscribes (capability-restricted; importing the store at eval only runs `randomUUID()`, no
  `listen()`). Without cross-window propagation a rebind stays stale in other windows until restart.
- **`initializeShortcuts` heals leaked `''` entries on load:** `[]` kept; `['']`/`['','']` dropped entirely (registry
  default applies, do NOT collapse to `[]`, that would suppress a default-bound command); `['⌘X','']` → `['⌘X']`. An
  empty string is never a real shortcut.
- **A captured combo conflicts only when scopes overlap** (one scope's ancestry chain contains the other), via the
  static `scopeHierarchy`. Adding a new scope means hand-editing that object. The dispatch map keeps one winner per
  combo: most-specific scope wins, registry order is the tiebreaker (pinned by `shortcut-dispatch.test.ts`).
- **`menuCommands` (in `shortcuts-store.ts`) must stay in sync with the Rust menu items.** The
  `menuCommands ↔ command_id_to_menu_id` set-equality test in `commands/rust-command-id-drift.test.ts` fails when a menu
  item is missing (stale accelerator after rebind) or excused without a documented reason.
- **`downloads.goToLatest` binds `⌘J` deliberately, not by oversight.** Finder's `⌘J` is "View Options", so Finder
  migrants may expect per-pane appearance controls — accepted (user-confirmed): Cmdr's view-mode and appearance controls
  have their own bindings, and `⌘J` ("Jump" to the latest download) displaces no existing Cmdr action. Don't "fix" it to
  match Finder.
- **`handleGlobalKeyDown` bails when focus is in a text input and the combo `isTypingKeyCombo`** (central typing guard),
  so a bare-key Tier 1 binding (Tab → switch pane) doesn't fire mid-typing. No chorded shortcuts; modifier-only combos
  are rejected.

Architecture, flows, and decision detail: `DETAILS.md`. Read it before any non-trivial work here: editing, planning,
reorganizing, or advising.
