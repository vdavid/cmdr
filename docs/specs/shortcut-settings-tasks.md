# Keyboard shortcut customization tasks

Task list for implementing keyboard shortcut customization as specified in
[shortcut-settings.md](./shortcut-settings.md).

## Legend

- `[ ]` Not started
- `[~]` In progress
- `[x]` Complete
- `[!]` Blocked

---

## Phase 1: Core infrastructure

### 1.1 Types and scope hierarchy

- [x] Create `src/lib/shortcuts/types.ts` with `KeyCombo`, `ShortcutConflict` interfaces (spec §3, §5)
- [x] Create `src/lib/shortcuts/scope-hierarchy.ts` with `CommandScope` type and hierarchy (spec §2)
- [x] Implement `getActiveScopes(scope)` function
- [x] Implement `scopesOverlap(scopeA, scopeB)` function
- [x] Write unit tests for scope hierarchy

### 1.2 Key capture

- [x] Create `src/lib/shortcuts/key-capture.ts` (spec §4)
- [x] Implement `formatKeyCombo(event)` with platform detection
- [x] Implement `normalizeKeyName(key)` with special key mappings
- [x] Implement `matchesShortcut(event, shortcut)` for matching
- [x] Implement `isMacOS()` platform helper
- [x] Write unit tests for all modifier combinations
- [x] Write unit tests for special keys (arrows, F1-F12, etc.)

### 1.3 Shortcuts store

- [x] Create `src/lib/shortcuts/shortcuts-store.ts` (spec §6)
- [x] Implement `initializeShortcuts()` — load from disk
- [x] Implement `getCustomShortcuts()` — get all customizations
- [x] Implement `setShortcut(commandId, index, shortcut)` — save one shortcut
- [x] Implement `addShortcut(commandId, shortcut)` — add new shortcut to command
- [x] Implement `removeShortcut(commandId, index)` — remove one shortcut
- [x] Implement `resetShortcut(commandId)` — reset single command to default
- [x] Implement `resetAllShortcuts()` — reset all to defaults
- [x] Implement `getEffectiveShortcuts(commandId)` — get custom or default
- [x] Implement `isShortcutModified(commandId)` — check if customized
- [x] Implement debounced save (500ms)
- [ ] Implement atomic write (temp + rename) — uses tauri-plugin-store
- [ ] Write unit tests for persistence layer

### 1.4 Conflict detection

- [x] Create `src/lib/shortcuts/conflict-detector.ts` (spec §5)
- [x] Implement `findConflictsForShortcut(shortcut, scope)` — find conflicting commands
- [x] Implement `getAllConflicts()` — find all conflicts in system
- [x] Implement `hasConflicts(commandId)` — check if command has conflicts
- [ ] Write unit tests for conflict detection

---

## Phase 2: UI implementation

### 2.1 Update KeyboardShortcutsSection

- [x] Refactor `KeyboardShortcutsSection.svelte` to use shortcuts store
- [x] Replace static `commands` with reactive data from store
- [x] Implement edit mode state management
- [x] Implement 500ms confirmation delay after key capture
- [x] Implement Escape to cancel editing
- [x] Implement Backspace/Delete to remove shortcut

### 2.2 Shortcut pill component

- [x] Implement normal display state (inline in KeyboardShortcutsSection)
- [x] Implement "Press keys..." editing state
- [x] Implement captured-but-not-saved state
- [x] Implement blue dot indicator for modified
- [x] Implement orange warning for conflicts
- [ ] Extract to separate component
- [ ] Write component tests

### 2.3 Add shortcut button

- [x] Implement [+] button to add new shortcut
- [x] Opens empty pill in edit mode
- [x] Captures and saves new shortcut

### 2.4 Conflict resolution dialog

- [x] Create conflict warning inline UI (spec §7.2)
- [x] Implement "Remove from other" action
- [x] Implement "Keep both" action
- [x] Implement "Cancel" action
- [ ] Write component tests

### 2.5 Filter chips

- [x] Implement "Modified" filter — show only customized commands
- [x] Implement "Conflicts" filter — show only conflicting commands
- [x] Add count badge to "Conflicts" chip
- [ ] Write component tests

### 2.6 Reset functionality

- [x] Implement "Reset all to defaults" button with confirmation dialog
- [x] Implement per-row reset button (when modified)
- [ ] Implement per-row context menu with "Reset to default"
- [x] Confirmation dialog for reset operations
- [ ] Write component tests

---

## Phase 3: Keyboard handler integration

### 3.1 Create keyboard handler

- [x] Create `src/lib/shortcuts/keyboard-handler.ts` (spec §8)
- [x] Implement `handleKeyDown(event, currentScope)` returning command ID
- [x] Priority: more specific scopes first
- [ ] Write unit tests

### 3.2 Integrate with main app

- [ ] Refactor `+page.svelte` to use new keyboard handler
- [ ] Remove hardcoded shortcut checks
- [ ] Track current scope based on focus
- [ ] Test all existing shortcuts still work

---

## Phase 4: Testing

### 4.1 Unit tests (TypeScript)

- [x] Scope hierarchy: all scope combinations
- [x] Key capture: modifiers (meta, ctrl, alt, shift)
- [x] Key capture: special keys (arrows, function keys, etc.)
- [x] Key capture: platform-specific formatting
- [ ] Shortcuts store: CRUD operations
- [ ] Shortcuts store: persistence round-trip
- [ ] Conflict detection: same scope
- [ ] Conflict detection: overlapping scopes
- [ ] Conflict detection: non-overlapping scopes (no conflict)
- [x] Run: `pnpm vitest run src/lib/shortcuts`

### 4.2 Component tests (Svelte)

- [ ] ShortcutPill: all states (normal, editing, captured, modified, conflict)
- [ ] KeyboardShortcutsSection: edit flow
- [ ] KeyboardShortcutsSection: filter chips
- [ ] Conflict dialog: all three actions
- [ ] Run: `pnpm vitest run src/lib/settings`

### 4.3 Integration tests

- [ ] Edit shortcut → verify saves to store
- [ ] Create conflict → resolve → verify result
- [ ] Reset single → verify returns to default
- [ ] Reset all → verify all return to defaults
- [ ] Modified filter → shows only customized
- [ ] Conflicts filter → shows only conflicting

### 4.4 E2E tests (Linux)

- [x] Add shortcut editing test to `test/e2e-linux/settings.spec.ts`
- [ ] Test: navigate to keyboard shortcuts section
- [ ] Test: click shortcut pill, capture new key combo
- [ ] Test: verify shortcut displays correctly
- [ ] Test: reset to defaults

---

## Phase 5: Checks and verification

### 5.1 Svelte checks

- [x] Run: `pnpm prettier --write src/lib/shortcuts src/lib/settings`
- [x] Run: `pnpm eslint src/lib/shortcuts src/lib/settings`
- [x] Run: `pnpm svelte-check`
- [x] Run: `pnpm vitest run src/lib/settings src/lib/shortcuts`
- [ ] Run: `./scripts/check.sh --check knip`

### 5.2 Rust checks

- [ ] Run: `./scripts/check.sh --check rustfmt` (blocked: missing GTK deps)
- [ ] Run: `./scripts/check.sh --check clippy` (blocked: missing GTK deps)
- [ ] Run: `./scripts/check.sh --check rust-tests` (blocked: missing GTK deps)

### 5.3 Full verification

- [ ] Run: `./scripts/check.sh` (all checks)
- [ ] Verify no regressions in existing functionality
- [ ] Manual smoke test of shortcut editing
- [ ] Review for any TODO comments left in code

---

## Phase 6: Documentation

### 6.1 Update feature docs

- [x] Create `docs/features/settings.md` with:
  - Keyboard shortcuts customization section
  - How to add a new command with shortcuts
  - How conflict detection works
- [ ] Update spec if implementation differs

### 6.2 Code documentation

- [x] Add inline comments where architecture is non-obvious
- [x] Ensure all public functions have meaningful JSDoc (not obvious ones)

---

## Dependencies

```
Phase 1 (Core) ─── Phase 2 (UI) ─── Phase 3 (Integration)
                         │
                         └──────── Phase 4 (Testing)
                                         │
                                         └── Phase 5 (Checks)
                                                   │
                                                   └── Phase 6 (Docs)
```

---

## Estimated scope

- **New files**: ~8 TypeScript modules, ~2 Svelte components
- **Modified files**: ~3 existing files
- **Tests**: ~25 unit tests, ~10 component tests, ~5 E2E scenarios
