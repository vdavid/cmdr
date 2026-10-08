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
  names (`Enter`, `Backspace`, `Escape`, `PageUp`) in ⌘⌃⌥⇧ order, and a Shift-typed symbol by its CHARACTER (`*`, ❌
  never `⇧8`), so `*` is "the `*` key" on every layout. Render via `toDisplayShortcut` (`⌘Backspace` → `⌘⌫`); never
  store or compare that form. A default spelled `↩`, `⌥⌘A`, or `⇧8` is dead on some keyboard, and
  `shortcut-vocabulary.test.ts` fails on it. DETAILS § Key capture.
- **Delta-only persistence; empty array vs missing key differ.** `"nav.parent": []` means "user removed all shortcuts";
  a missing key means "use registry defaults". `initializeShortcuts` loads `[]`, so it survives a reload.
- **`saveToStore` reconciles disk against the in-memory map on every write** (deletes any `shortcut:*` key with no map
  entry), else a value dropped by reset/cleanup resurrects at next load. `saveChain` serializes saves so two rapid
  mutations can't interleave.
- **macOS-native (`app.quit`/`hide`/`hideOthers`/`showAll`) and fixed-key (`FIXED_KEY_COMMAND_IDS`) commands are not
  customizable, enforced at the store boundary.** Load drops persisted entries, the mutators no-op with `log.warn`,
  `resetShortcut` stays permissive. MCP edits route through the same mutators, so they inherit the guard.
- **Every mutation emits `shortcuts:changed` after saving; the per-window `SENDER_ID` is the loop guard.** The listener
  updates the local map and calls `notifyListeners`, never saving or re-emitting. Without this a rebind stays stale in
  other windows until restart.
- **`initializeShortcuts` heals leaked `''` entries on load:** `[]` kept; `['']` dropped entirely (❌ don't collapse it
  to `[]`, that suppresses a default-bound command); `['⌘X','']` → `['⌘X']`.
- **A captured combo conflicts only when scopes overlap** (one ancestry chain contains the other), via the static
  `scopeHierarchy` — hand-edit it to add a scope. The dispatch map keeps one winner per combo: most-specific scope wins,
  registry order breaks ties (pinned by `shortcut-dispatch.test.ts`).
- **`menuCommands` (in `shortcuts-store.ts`) must stay in sync with the Rust menu items.** The set-equality test in
  `commands/rust-command-id-drift.test.ts` fails on a missing item (stale accelerator after a rebind) or an undocumented
  excuse.
- **`downloads.goToLatest` binds `⌘J` deliberately** (not Finder's "View Options"); don't "fix" it.
- **`handleGlobalKeyDown` bails when focus is in a text input and the combo `isTypingKeyCombo`** (central typing guard),
  so a bare-key Tier 1 binding (Tab → switch pane) doesn't fire mid-typing. No chords or modifier-only combos.
- **❌ A local handler that ACTS on a key calls `claimKey(e)`** (`claim-key.ts`, imported from the leaf so a barrel mock
  can't fake it). `preventDefault` alone is NOT a claim, so the command runs twice — invisibly, four times so far. Bare
  keys are Tier 1 too (`Enter`, `Tab`, Space, `PageUp`/`Down`, `Home`/`End`, F5, Insert). DETAILS § Local handlers.
- **❌ Never hand-roll a key predicate (`e.key === 'a' && e.metaKey`) in a keydown handler.** That's a modifier
  SUPERSET: `⌥⌘A` matched it, so opening Ask Cmdr also selected every file. A local handler calls
  `eventMatchesCommand(e, 'some.command')`, or `comboMatchesCommand(resolveKeyCombo(e), …)`, ❌ never a bare
  `formatKeyCombo(e)`: that skips the layout fallbacks the dispatcher uses. Enforced by `cmdr/no-raw-key-match`; DETAILS
  § Local handlers.

Architecture, flows, and decision detail: `DETAILS.md`. Read it before any non-trivial work here: editing, planning,
reorganizing, or advising.
