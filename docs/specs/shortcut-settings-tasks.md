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

- [ ] Create `src/lib/shortcuts/types.ts` with `KeyCombo`, `ShortcutConflict` interfaces (spec §3, §5)
- [ ] Create `src/lib/shortcuts/scope-hierarchy.ts` with `CommandScope` type and hierarchy (spec §2)
- [ ] Implement `getActiveScopes(scope)` function
- [ ] Implement `scopesOverlap(scopeA, scopeB)` function
- [ ] Write unit tests for scope hierarchy

### 1.2 Key capture

- [ ] Create `src/lib/shortcuts/key-capture.ts` (spec §4)
- [ ] Implement `formatKeyCombo(event)` with platform detection
- [ ] Implement `normalizeKeyName(key)` with special key mappings
- [ ] Implement `matchesShortcut(event, shortcut)` for matching
- [ ] Implement `isMacOS()` platform helper
- [ ] Write unit tests for all modifier combinations
- [ ] Write unit tests for special keys (arrows, F1-F12, etc.)

### 1.3 Shortcuts store

- [ ] Create `src/lib/shortcuts/shortcuts-store.ts` (spec §6)
- [ ] Implement `initializeShortcuts()` — load from disk
- [ ] Implement `getCustomShortcuts()` — get all customizations
- [ ] Implement `setShortcut(commandId, index, shortcut)` — save one shortcut
- [ ] Implement `addShortcut(commandId, shortcut)` — add new shortcut to command
- [ ] Implement `removeShortcut(commandId, index)` — remove one shortcut
- [ ] Implement `resetShortcut(commandId)` — reset single command to default
- [ ] Implement `resetAllShortcuts()` — reset all to defaults
- [ ] Implement `getEffectiveShortcuts(commandId)` — get custom or default
- [ ] Implement `isShortcutModified(commandId)` — check if customized
- [ ] Implement debounced save (500ms)
- [ ] Implement atomic write (temp + rename)
- [ ] Write unit tests for persistence layer

### 1.4 Conflict detection

- [ ] Create `src/lib/shortcuts/conflict-detector.ts` (spec §5)
- [ ] Implement `findConflictsForShortcut(shortcut, scope)` — find conflicting commands
- [ ] Implement `getAllConflicts()` — find all conflicts in system
- [ ] Implement `hasConflicts(commandId)` — check if command has conflicts
- [ ] Write unit tests for conflict detection

---

## Phase 2: UI implementation

### 2.1 Update KeyboardShortcutsSection

- [ ] Refactor `KeyboardShortcutsSection.svelte` to use shortcuts store
- [ ] Replace static `commands` with reactive data from store
- [ ] Implement edit mode state management
- [ ] Implement 500ms confirmation delay after key capture
- [ ] Implement Escape to cancel editing
- [ ] Implement Backspace/Delete to remove shortcut

### 2.2 Shortcut pill component

- [ ] Create `src/lib/settings/components/ShortcutPill.svelte`
- [ ] Implement normal display state
- [ ] Implement "Press keys..." editing state
- [ ] Implement captured-but-not-saved state
- [ ] Implement blue dot indicator for modified
- [ ] Implement orange warning for conflicts
- [ ] Write component tests

### 2.3 Add shortcut button

- [ ] Implement [+] button to add new shortcut
- [ ] Opens empty pill in edit mode
- [ ] Captures and saves new shortcut

### 2.4 Conflict resolution dialog

- [ ] Create conflict warning inline UI (spec §7.2)
- [ ] Implement "Remove from other" action
- [ ] Implement "Keep both" action
- [ ] Implement "Cancel" action
- [ ] Write component tests

### 2.5 Filter chips

- [ ] Implement "Modified" filter — show only customized commands
- [ ] Implement "Conflicts" filter — show only conflicting commands
- [ ] Add count badge to "Conflicts" chip
- [ ] Write component tests

### 2.6 Reset functionality

- [ ] Implement "Reset all to defaults" button with confirmation dialog
- [ ] Implement per-row context menu with "Reset to default"
- [ ] Confirmation dialog for both
- [ ] Write component tests

---

## Phase 3: Keyboard handler integration

### 3.1 Create keyboard handler

- [ ] Create `src/lib/shortcuts/keyboard-handler.ts` (spec §8)
- [ ] Implement `handleKeyDown(event, currentScope)` returning command ID
- [ ] Priority: more specific scopes first
- [ ] Write unit tests

### 3.2 Integrate with main app

- [ ] Refactor `+page.svelte` to use new keyboard handler
- [ ] Remove hardcoded shortcut checks
- [ ] Track current scope based on focus
- [ ] Test all existing shortcuts still work

---

## Phase 4: Testing

### 4.1 Unit tests (TypeScript)

- [ ] Scope hierarchy: all scope combinations
- [ ] Key capture: modifiers (meta, ctrl, alt, shift)
- [ ] Key capture: special keys (arrows, function keys, etc.)
- [ ] Key capture: platform-specific formatting
- [ ] Shortcuts store: CRUD operations
- [ ] Shortcuts store: persistence round-trip
- [ ] Conflict detection: same scope
- [ ] Conflict detection: overlapping scopes
- [ ] Conflict detection: non-overlapping scopes (no conflict)
- [ ] Run: `pnpm vitest run src/lib/shortcuts`

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

- [ ] Add shortcut editing test to `test/e2e-linux/settings.spec.ts`
- [ ] Test: navigate to keyboard shortcuts section
- [ ] Test: click shortcut pill, capture new key combo
- [ ] Test: verify shortcut displays correctly
- [ ] Test: reset to defaults

---

## Phase 5: Checks and verification

### 5.1 Svelte checks

- [ ] Run: `./scripts/check.sh --check desktop-svelte-prettier`
- [ ] Run: `./scripts/check.sh --check desktop-svelte-eslint`
- [ ] Run: `./scripts/check.sh --check svelte-check`
- [ ] Run: `./scripts/check.sh --check svelte-tests`
- [ ] Run: `./scripts/check.sh --check knip`

### 5.2 Rust checks

- [ ] Run: `./scripts/check.sh --check rustfmt`
- [ ] Run: `./scripts/check.sh --check clippy`
- [ ] Run: `./scripts/check.sh --check rust-tests`

### 5.3 Full verification

- [ ] Run: `./scripts/check.sh` (all checks)
- [ ] Verify no regressions in existing functionality
- [ ] Manual smoke test of shortcut editing
- [ ] Review for any TODO comments left in code

---

## Phase 6: Documentation

### 6.1 Update feature docs

- [ ] Create or update `docs/features/settings.md` with:
  - Keyboard shortcuts customization section
  - How to add a new command with shortcuts
  - How conflict detection works
- [ ] Update spec if implementation differs

### 6.2 Code documentation

- [ ] Add inline comments where architecture is non-obvious
- [ ] Ensure all public functions have meaningful JSDoc (not obvious ones)

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
