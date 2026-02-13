# Rename feature tasks

## Milestone 1: Backend foundation

- [x] Create `src-tauri/src/file_system/validation.rs` with `validate_filename()` and `validate_path_length()`
- [x] Extend Volume trait `rename()` with `force: bool` param, update all implementations (LocalPosix, InMemory, MTP)
- [x] Add `check_rename_permission` Tauri command (parent writable, immutable flag, SIP, file lock checks)
- [x] Add `check_rename_validity` Tauri command (validation + conflict detection with inode comparison)
- [x] Add `rename_file` Tauri command with `force` param
- [x] Add Rust tests for validation, permission check, and rename with/without force
- [x] Run `./scripts/check.sh --rust` and fix any issues

## Milestone 2: Frontend core

- [x] Create `filename-validation.ts` with client-side validation (disallowed chars, empty, byte length)
- [x] Create `rename-state.svelte.ts` with $state for rename mode (active, filename, validation status, etc.)
- [x] Create `InlineRenameEditor.svelte` — input component with green/red/yellow border states and animations
- [x] Integrate InlineRenameEditor into FullList.svelte and BriefList.svelte (replaces name cell when active)
- [x] Wire activation: Shift+F6 in command registry, Edit menu item, context menu item
- [x] Implement click-to-rename (800 ms timer, 10 px threshold, cancel on double-click)
- [x] Suppress app-level shortcuts during rename (reuse dialog-open flag in keyboard-handler.ts)
- [x] Handle all cancel triggers (Escape, click elsewhere, Tab, drag, scroll >200 px, sort/hidden toggle)
- [x] Add Vitest tests for filename-validation.ts and rename-state
- [x] Run `./scripts/check.sh --svelte` and fix any issues

## Milestone 3: Dialogs, notifications, and settings

- [x] Add `allowFileExtensionChanges` setting (`yes`/`no`/`ask`, default `ask`) to settings store
- [x] Add UI for extension change setting in Settings > General > File operations
- [x] Create `ExtensionChangeDialog.svelte` with Keep/Use buttons and "Always allow" checkbox
- [x] Create `RenameConflictDialog.svelte` with file comparison and Overwrite/Cancel/Continue buttons
- [x] Register `rename-conflict` and `extension-change` in dialog registry
- [x] Build top-right notification component (reusable) for validation errors, permission denied, hidden file info
- [x] Implement read-only volume alert dialog on rename attempt
- [x] Add Vitest tests for dialog logic
- [x] Run `./scripts/check.sh --svelte` and fix any issues

## Milestone 4: Integration and edge cases

- [x] Wire save flow: trim -> validate -> extension check -> conflict check -> backend rename
- [x] Post-rename cursor tracking: after file watcher refresh, move cursor to renamed file
- [x] Handle hidden file notification (dot-prefixed rename with hidden files off)
- [x] Handle external file deletion/rename during editing (cancel gracefully)
- [x] Add MCP exports to DualPaneExplorer: `startRename()`, `cancelRename()`, `isRenaming()`
- [x] Add new source files to `coverage-allowlist.json` as needed
- [ ] Manual testing with MCP servers (rename, conflict, extension change, read-only, permissions)
- [x] Run `./scripts/check.sh --svelte` and fix any issues

## Milestone 5: Docs, accessibility, and final checks

- [x] Add ARIA attributes (aria-label, aria-live, aria-invalid) to rename editor
- [x] Write `docs/features/rename.md`
- [x] Run full `./scripts/check.sh` and fix all issues
- [ ] Final manual test pass with MCP servers
