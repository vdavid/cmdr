# Commands

Centralized command registry and fuzzy search engine for the command palette.

## Module map

- **`command-ids.ts`**: `COMMAND_IDS` (the `as const` id tuple), the derived `CommandId` union, the `isCommandId()`
  boundary guard.
- **`types.ts`**: `Command`, `CommandMatch`, `CommandScope`, plus `CommandArgs` / `CommandDispatchArgs` (the dispatch
  arg-tuple shape).
- **`command-registry.ts`**: the `commands` array (single source of truth), `getPaletteCommands()`,
  `updateLicenseCommandName()`, and the `NATIVE_SHORTCUT_COMMAND_IDS` / `FIXED_KEY_COMMAND_IDS` lists.
- **`fuzzy-search.ts`**: `searchCommands()` (palette set) + `searchAllCommands()` (full registry), via
  `@leeoniya/ufuzzy`.
- Tests: `fuzzy-search.test.ts`, `command-registry.test.ts` (tuple ↔ registry set-equality, palette-visible-set pin),
  `command-types.test.ts` (compile-time arg-shape guards), `rust-command-id-drift.test.ts` (every Rust-emitted id ∈
  `COMMAND_IDS`).

## Must-knows (invariants and guardrails)

- **Entries hold i18n message KEYS, not English** (`CommandSource.nameKey` / `descriptionKey`); copy lives in
  `messages/en/commands.json`, resolved via getter-backed `name` / `description`. Don't hardcode a label
  (`cmdr/no-raw-user-facing-string` is enforced here); IDS stay untouched. `updateLicenseCommandName` flips a flag, not
  the text. The array stays a getter-backed mutable `Command[]`. Details: [DETAILS.md](DETAILS.md) § i18n.
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
  AppKit owns both the behavior and the accelerator via `PredefinedMenuItems`; including them in JS shortcut dispatch
  would double-execute. `nativeShortcut: true` (set on exactly `NATIVE_SHORTCUT_COMMAND_IDS`) is the single source of
  truth that makes the shortcuts editor render them read-only and the store mutators refuse to write them
  (`DISPATCH_EXEMPT_IDS` sources its native-menu family from the same list).
- **`scope` is documentation-only, not runtime-enforced** (keyboard routing is each UI component's job; scope drives
  conflict detection and Settings display).
- **The uFuzzy instance is a module-level singleton**; `info.ranges` is a flat `[start, end, …]` array (`end`
  exclusive), unpacked into per-char `matchedIndices`. Understand this before changing highlighting (see DETAILS.md).

## Gotchas

- **`handleCommandExecute` intercepts `edit.copy` and `selection.selectAll` BEFORE logging when the selection is in an
  opt-in text region** (`.error-pane` or `[data-text-region]`). The native Edit menu's ⌘C / ⌘A fire through this
  dispatcher even for plain text in the ErrorPane; without the early bail, every text copy would trigger file-scope side
  effects and pollute the rollback log. See `handleTextRegionShortcut` in `command-dispatch.ts`.
- **Adding a command with a menu item touches four places**, and missing any one fails silently (shortcut works but menu
  doesn't, or vice versa): (1) `command-registry.ts`, (2) the handler in `routes/(main)/command-handlers/`, (3)
  `src-tauri/src/menu/mod.rs` id mappings (`menu_id_to_command` + `command_id_to_menu_id`) plus the matching
  `Menu/SubmenuItem::with_id` registration in the right platform builder (`macos.rs` / `linux.rs`), (4) the
  `menuCommands` array in `shortcuts-store.ts`.

## Adding a command

The full step list (ids, registry entry, arg overrides, handler, palette pin, native-menu wiring) is in
[DETAILS.md](DETAILS.md) § "Adding a command". The compile-time and set-equality guards above catch most omissions; the
four-places gotcha covers the menu-item case.

Full details (the `Command` / `CommandArgs` / `CommandDispatchArgs` type definitions, the uFuzzy config and ranking
behavior, `searchAllCommands` rationale, the `view.showHidden` local-first path, and decision rationale):
[DETAILS.md](DETAILS.md).
