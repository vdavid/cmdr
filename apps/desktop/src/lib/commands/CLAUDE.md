# Commands

Centralized command registry and fuzzy search engine for the command palette.

## Module map

- **`command-ids.ts`**: `COMMAND_IDS` (the `as const` id tuple), the derived `CommandId` union, the `isCommandId()`
  boundary guard.
- **`types.ts`**: `Command`, `CommandMatch`, `CommandScope`, plus `CommandArgs` / `CommandDispatchArgs` (the dispatch
  arg-tuple shape).
- **`command-registry.ts`**: the `commands` array, `getPaletteCommands()`, `updateLicenseCommandName()`, and the
  `NATIVE_SHORTCUT_COMMAND_IDS` / `FIXED_KEY_COMMAND_IDS` lists. Registry DATA is in **`sources/*.ts`** (add commands
  there, not inline).
- **`fuzzy-search.ts`**: `searchCommands()` (palette set) + `searchAllCommands()` (full registry), via the shared
  `#lib/utils/unicode-fuzzy.ts` engine (any script, accents optional).
- Tests: `fuzzy-search.test.ts`, `command-registry.test.ts` (set-equality + palette pin), `command-types.test.ts`
  (arg-shape guards), `rust-command-id-drift.test.ts` (Rust ids ∈ `COMMAND_IDS`), `command-reachability.test.ts` (a key,
  a palette row, or a reasoned exemption).

## Must-knows (invariants and guardrails)

- **Entries hold i18n message KEYS, not English** (`CommandSource.nameKey` / `descriptionKey`); copy lives in
  `messages/en/commands.json`, resolved via getter-backed `name` / `description`. Don't hardcode a label
  (`cmdr/no-raw-user-facing-string`); IDS stay untouched. `DETAILS.md` § i18n.
- **`name` is every listing surface's label** (Settings, help window, conflict toast, MCP); **`displayName` is the
  PALETTE's alone**, falling back to `name`. A live-state label takes a generic hook — a `nameKey` thunk or a
  `displayName` resolver — ❌ never a per-id branch in `resolveCommand`. Rust's native-menu labels are a SEPARATE
  mechanism. `DETAILS.md` § "Two labels".
- **Two set-equality guards keep tuple and registry in sync.** `Command.id: CommandId` enforces tuple ⊇ registry at
  compile time; `command-registry.test.ts` enforces registry ⊇ tuple. Adding to one without the other fails the build or
  the test.
- **Never `as CommandId`-cast at an untyped edge; use `isCommandId()`.** Ids enter the frontend untyped from the Rust
  `execute-command` payload, the cross-window emit from `LicenseSection.svelte`, and the selection-dialog `onCommand`
  prop. A stale id cast through would miss the handler record and silently no-op. The IPC boundary is untyped (Rust
  emits a bare `json!`), so `rust-command-id-drift.test.ts` is the backstop.
- **`handleCommandExecute(commandId, ctx, ...args)` is the only dispatch entry point** (in
  `routes/(main)/command-dispatch.ts`). It looks the id up in the flat `commandHandlers` record (in
  `routes/(main)/command-handlers/`), keyed by `Exclude<CommandId, DispatchExemptId>` so every dispatchable id has a
  handler at compile time; handlerless ids go in `DISPATCH_EXEMPT_IDS` and silently no-op.
- **Native macOS commands (quit, hide, hide others, show all) carry `nativeShortcut: true` and `showInPalette: false`.**
  AppKit owns behavior AND accelerator via `PredefinedMenuItems`, so JS dispatch would double-execute. The flag sits on
  exactly `NATIVE_SHORTCUT_COMMAND_IDS`, the one source for the read-only editor rows, the store mutators' refusal, and
  `DISPATCH_EXEMPT_IDS`'s native family.
- **`scope` is documentation-only, not runtime-enforced** (keyboard routing is each UI component's job; scope drives
  conflict detection and Settings display). A NEW scope is three places: `DETAILS.md` § "Adding a command".
- **The haystack is `foldForSearch`ed** (typed `FoldedText`); `info.ranges` is a flat `[start, end, …]` array (`end`
  exclusive), unpacked into per-char `matchedIndices`. Read `DETAILS.md` before changing highlighting.

## Gotchas

- **`handleCommandExecute` intercepts `edit.copy` and `selection.selectAll` BEFORE logging when the selection is in an
  opt-in text region** (`.error-pane` or `[data-text-region]`). The native Edit menu's ⌘C / ⌘A fire through this
  dispatcher even for plain text in the ErrorPane; without the early bail, every text copy would trigger file-scope side
  effects and pollute the rollback log. See `handleTextRegionShortcut` in `command-dispatch.ts`.
- **Adding a command with a menu item touches four places**, and missing any one fails silently (shortcut works but menu
  doesn't, or vice versa): (1) `command-registry.ts`, (2) the handler in `routes/(main)/command-handlers/`, (3)
  `src-tauri/src/menu/command_map.rs` id mappings (`menu_id_to_command` + `command_id_to_menu_id`) plus its row in
  `menu/menu_bar.rs`, (4) the `menuCommands` array in `shortcuts-store.ts`.

## Adding a command

Full step list in `DETAILS.md` § "Adding a command", with everything else (types, labels, uFuzzy ranking, rationale).
The guards above catch most omissions; the four-places gotcha covers the menu-item case.
