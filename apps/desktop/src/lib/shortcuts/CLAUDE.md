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

- **ONE canonical combo vocabulary; macOS glyphs are display only.** `formatKeyCombo` is the single writer: word key
  names (`Enter`, `Backspace`, `Escape`, `PageUp`) in ⌘⌃⌥⇧ order. Storage, dispatch, conflict detection, and Rust
  accelerators all speak it. Render via `toDisplayShortcut` (`⌘Backspace` → `⌘⌫`); never store or compare that form. A
  default spelled `↩`, or in Apple's `⌥⌘A` order, is dead on the keyboard — `shortcut-vocabulary.test.ts` fails on it.
  Writes canonicalize at the store boundary; load heals older files.
- **Delta-only persistence; empty array vs missing key are semantically different.** `"nav.parent": []` means "user
  removed all shortcuts, don't use defaults"; a missing key means "use registry defaults". `initializeShortcuts` loads
  `[]` (and skips only non-array garbage), so the empty array survives a reload.
- **`saveToStore` reconciles disk against the in-memory map on every write** (deletes any `shortcut:*` key with no map
  entry), else a value dropped by reset/cleanup resurrects at next load. `saveChain` serializes saves so two rapid
  mutations can't interleave.
- **macOS-native (`app.quit`/`hide`/`hideOthers`/`showAll`) and fixed-key (`FIXED_KEY_COMMAND_IDS`) commands are not
  customizable, enforced at the store boundary.** Load drops persisted entries, mutators (`setShortcut` / `addShortcut`
  / `removeShortcut`) no-op with `log.warn`, `resetShortcut` stays permissive (delete-only). MCP edits route through
  these same mutators, so they inherit the guard. `isNativeShortcutCommand` / `isFixedKeyCommand` are the predicates.
- **Every mutation emits `shortcuts:changed` after saving; the per-window `SENDER_ID` is the loop guard.** The listener
  updates the local map and calls `notifyListeners`, never saving or re-emitting. The viewer never subscribes
  (capability-restricted). Without this a rebind stays stale in other windows until restart.
- **`initializeShortcuts` heals leaked `''` entries on load:** `[]` kept; `['']`/`['','']` dropped entirely (registry
  default applies — do NOT collapse to `[]`, that suppresses a default-bound command); `['⌘X','']` → `['⌘X']`.
- **A captured combo conflicts only when scopes overlap** (one ancestry chain contains the other), via the static
  `scopeHierarchy` — hand-edit it to add a scope. The dispatch map keeps one winner per combo: most-specific scope wins,
  registry order breaks ties (pinned by `shortcut-dispatch.test.ts`).
- **`menuCommands` (in `shortcuts-store.ts`) must stay in sync with the Rust menu items.** The
  `menuCommands ↔ command_id_to_menu_id` set-equality test in `commands/rust-command-id-drift.test.ts` fails when a menu
  item is missing (stale accelerator after rebind) or excused without a documented reason.
- **`downloads.goToLatest` binds `⌘J` deliberately, not by oversight**, deviating from Finder's "View Options".
  User-confirmed; don't "fix" it to match Finder. Rationale: DETAILS.md.
- **`handleGlobalKeyDown` bails when focus is in a text input and the combo `isTypingKeyCombo`** (central typing guard),
  so a bare-key Tier 1 binding (Tab → switch pane) doesn't fire mid-typing. No chords; modifier-only combos are
  rejected.
- **❌ Never hand-roll a key predicate (`e.key === 'a' && e.metaKey`) in a keydown handler.** That's a modifier
  SUPERSET: `⌥⌘A` matched it, so opening Ask Cmdr also selected every file. A local handler calls
  `eventMatchesCommand(e, 'some.command')` (exact, follows a rebind, scope-correct); the document handler uses
  `lookupCommand`. `allowShift` is only for the file list's Shift-extends-selection gesture. Class-of-key matchers
  (type-to-jump, `+`/`-`) stay hand-rolled but still reject ⌘/⌃/⌥.

Architecture, flows, and decision detail: `DETAILS.md`. Read it before any non-trivial work here: editing, planning,
reorganizing, or advising.
