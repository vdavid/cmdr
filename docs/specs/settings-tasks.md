# Settings implementation tasks

Task list for implementing the settings system as specified in [settings.md](./settings.md).

## Legend

- `[ ]` Not started
- `[~]` In progress
- `[x]` Complete
- `[!]` Blocked

---

## Phase 1: Foundation

### 1.1 Settings registry (TypeScript)

- [ ] Create `src/lib/settings/settings-registry.ts` with `SettingDefinition` interface (spec §2.1)
- [ ] Implement `getSetting<T>(id)` with type safety and default fallback
- [ ] Implement `setSetting<T>(id, value)` with constraint validation
- [ ] Implement `getSettingDefinition(id)` for UI rendering
- [ ] Implement `getSettingsInSection(path)` for tree rendering
- [ ] Implement `searchSettings(query)` using uFuzzy (spec §13.2)
- [ ] Implement `resetSetting(id)` and `resetAllSettings()`
- [ ] Implement `isModified(id)` for blue dot indicators
- [ ] Add validation for `enum` types with `allowCustom` and custom ranges
- [ ] Add validation for `number` types with `min`/`max`/`step`
- [ ] Add validation for `duration` types with unit conversion
- [ ] Throw `SettingValidationError` with descriptive messages
- [ ] Write unit tests for all registry functions
- [ ] Write unit tests for constraint validation edge cases

### 1.2 Settings persistence (TypeScript)

- [ ] Create `src/lib/settings/settings-store.ts` for persistence layer
- [ ] Implement debounced save (500ms) with atomic write
- [ ] Implement schema version field and migration framework
- [ ] Implement forward compatibility (preserve unknown keys)
- [ ] Handle save errors: log, retry once, show toast
- [ ] Write unit tests for persistence layer
- [ ] Write unit tests for schema migration

### 1.3 Settings Tauri commands (Rust)

- [ ] Create `src-tauri/src/settings/mod.rs` module
- [ ] Implement `get_setting` command delegating to registry
- [ ] Implement `set_setting` command with validation
- [ ] Implement `reset_setting` and `reset_all_settings` commands
- [ ] Implement `get_all_settings` for initial load
- [ ] Expose commands in `lib.rs`
- [ ] Write Rust unit tests for settings commands

### 1.4 Port availability checker (Rust)

- [ ] Create `src-tauri/src/settings/port_checker.rs`
- [ ] Implement `check_port_available(port)` command
- [ ] Implement `find_available_port(start_port)` command (max 100 attempts)
- [ ] Write unit tests for port checker

---

## Phase 2: Settings window

### 2.1 Window setup

- [ ] Create settings window configuration in `tauri.conf.json`
- [ ] Set default size 800×600, min size 600×400
- [ ] Configure window to open centered on main window
- [ ] Implement Cmd+, shortcut to open/focus settings window
- [ ] Implement ESC to close settings window
- [ ] Prevent duplicate settings windows

### 2.2 Window layout (Svelte)

- [ ] Create `src/lib/settings/SettingsWindow.svelte` as root component
- [ ] Implement fixed 220px sidebar + flexible content area layout
- [ ] Create `src/lib/settings/SettingsSidebar.svelte` with search + tree
- [ ] Create `src/lib/settings/SettingsContent.svelte` with scrollable panels
- [ ] Implement scroll-to-section when tree item selected
- [ ] Implement active section highlighting in tree

### 2.3 Search implementation

- [ ] Create `src/lib/settings/SettingsSearch.svelte` component
- [ ] Build search index from registry on mount
- [ ] Implement uFuzzy search with same config as command palette
- [ ] Filter tree to show only sections with matches
- [ ] Highlight matched characters in results
- [ ] Implement keyboard navigation (Arrow, Enter, Escape)
- [ ] Implement empty state message (spec §13.4)
- [ ] Write unit tests for search filtering

---

## Phase 3: Setting components

### 3.1 Base components (using Ark UI)

- [ ] Create `src/lib/settings/components/SettingRow.svelte` wrapper
- [ ] Create `src/lib/settings/components/SettingSwitch.svelte`
- [ ] Create `src/lib/settings/components/SettingSelect.svelte` with custom option support
- [ ] Create `src/lib/settings/components/SettingRadioGroup.svelte` with inline descriptions
- [ ] Create `src/lib/settings/components/SettingToggleGroup.svelte`
- [ ] Create `src/lib/settings/components/SettingSlider.svelte` with NumberInput combo
- [ ] Create `src/lib/settings/components/SettingNumberInput.svelte` with validation
- [ ] Create `src/lib/settings/components/SettingTextInput.svelte`
- [ ] Create `src/lib/settings/components/SettingDuration.svelte` (number + unit dropdown)
- [ ] Implement "Coming soon" badge for disabled settings
- [ ] Implement restart indicator for settings that require restart
- [ ] Implement blue dot for modified settings
- [ ] Implement "Reset to default" link for modified settings
- [ ] Write unit tests for each component

### 3.2 Section components

- [ ] Create `src/lib/settings/sections/AppearanceSection.svelte` (spec §4)
- [ ] Create `src/lib/settings/sections/FileOperationsSection.svelte` (spec §5)
- [ ] Create `src/lib/settings/sections/UpdatesSection.svelte` (spec §6)
- [ ] Create `src/lib/settings/sections/NetworkSection.svelte` (spec §7)
- [ ] Create `src/lib/settings/sections/McpServerSection.svelte` (spec §10)
- [ ] Create `src/lib/settings/sections/LoggingSection.svelte` (spec §11)
- [ ] Create `src/lib/settings/sections/AdvancedSection.svelte` (spec §12)

---

## Phase 4: Appearance section

### 4.1 UI density

- [ ] Add `appearance.uiDensity` to registry with Compact/Comfortable/Spacious options
- [ ] Implement ToggleGroup UI
- [ ] Map density to internal values (rowHeight, iconSize)
- [ ] Apply density changes immediately to main window
- [ ] Write integration test for density changes

### 4.2 App icons for documents

- [ ] Add `appearance.useAppIconsForDocuments` to registry
- [ ] Migrate from `config.rs` constant to setting
- [ ] Implement Switch UI
- [ ] Wire to icon loading logic
- [ ] Write integration test

### 4.3 File size format

- [ ] Add `appearance.fileSizeFormat` to registry (binary/si)
- [ ] Implement Select UI with inline descriptions (not tooltips)
- [ ] Create `formatFileSize(bytes, format)` utility
- [ ] Update file list to use setting
- [ ] Write unit tests for formatFileSize

### 4.4 Date/time format

- [ ] Add `appearance.dateTimeFormat` to registry
- [ ] Implement RadioGroup with system/iso/short/custom options
- [ ] Implement custom format input with live preview
- [ ] Implement collapsible format placeholder help
- [ ] Create `formatDateTime(date, format)` utility
- [ ] Update file list to use setting
- [ ] Write unit tests for formatDateTime

---

## Phase 5: File operations section

### 5.1 Delete settings (disabled)

- [ ] Add `fileOperations.confirmBeforeDelete` to registry (disabled)
- [ ] Add `fileOperations.deletePermanently` to registry (disabled)
- [ ] Implement Switch UIs with "Coming soon" badges

### 5.2 Progress update interval

- [ ] Add `fileOperations.progressUpdateInterval` to registry
- [ ] Constraints: slider snaps 100/250/500/1000/2000, custom 50-5000ms
- [ ] Implement Slider + NumberInput combo UI
- [ ] Migrate from `operations.rs` constant to setting
- [ ] Wire to file operations progress emitter
- [ ] Write integration test

### 5.3 Max conflicts to show

- [ ] Add `fileOperations.maxConflictsToShow` to registry
- [ ] Options: 1, 2, 3, 5, 10, 50, 100 (default), 200, 500, custom 1-1000
- [ ] Implement Select with custom option UI
- [ ] Migrate from `write_operations/types.rs` constant
- [ ] Wire to conflict resolution logic
- [ ] Write integration test

---

## Phase 6: Updates section

- [ ] Add `updates.autoCheck` to registry
- [ ] Implement Switch UI
- [ ] Wire to update checker enable/disable
- [ ] Write integration test

---

## Phase 7: Network section

### 7.1 Share cache duration

- [ ] Add `network.shareCacheDuration` to registry
- [ ] Options: 30s, 5min, 1h, 1d, 30d, custom
- [ ] Implement Select with custom duration input
- [ ] Migrate from `smb_client.rs` constant
- [ ] Wire to SMB cache TTL
- [ ] Write integration test

### 7.2 Network timeout mode

- [ ] Add `network.timeoutMode` to registry (normal/slow/custom)
- [ ] Implement RadioGroup with inline descriptions
- [ ] Implement custom timeout NumberInput
- [ ] Map modes to actual timeout values (15s/45s/custom)
- [ ] Wire to network operations
- [ ] Write integration test

---

## Phase 8: Keyboard shortcuts

### 8.1 Data layer

- [ ] Create `src/lib/settings/shortcuts/shortcut-store.ts`
- [ ] Implement shortcut persistence (separate from main settings)
- [ ] Implement conflict detection
- [ ] Implement reset to defaults (with confirmation)
- [ ] Write unit tests

### 8.2 UI components

- [ ] Create `src/lib/settings/shortcuts/ShortcutsSection.svelte`
- [ ] Implement dual search: action name (left) + key combo (right, narrower)
- [ ] Implement filter chips: All, Modified, Conflicts (with count badge)
- [ ] Create virtualized command list grouped by scope
- [ ] Create `ShortcutPill.svelte` component
- [ ] Implement click-to-edit on shortcut pills
- [ ] Implement key capture mode ("Press keys...")
- [ ] Implement 500ms confirmation delay
- [ ] Implement conflict warning with "Remove from other" option
- [ ] Implement Escape to cancel, Backspace to remove
- [ ] Implement [+] button to add additional shortcut
- [ ] Implement blue dot for modified shortcuts
- [ ] Implement "Reset all to defaults" button with confirmation dialog
- [ ] Implement per-row context menu with "Reset to default" (with confirmation)
- [ ] Write integration tests

### 8.3 Key combination search

- [ ] Implement key capture in search field (not text typing)
- [ ] Display captured combo visually
- [ ] Filter commands by exact shortcut match
- [ ] Implement clear button (×)

---

## Phase 9: Themes

### 9.1 Theme mode

- [ ] Add `theme.mode` to registry (light/dark/system)
- [ ] Implement ToggleGroup with icons (☀️ 🌙 💻)
- [ ] Wire to CSS custom properties / media query
- [ ] Ensure immediate preview
- [ ] Write integration test

### 9.2 Future placeholders

- [ ] Add "Coming soon" placeholder for preset themes
- [ ] Add "Coming soon" placeholder for custom theme editor

---

## Phase 10: Developer section

### 10.1 MCP server

- [ ] Add `developer.mcpEnabled` to registry
- [ ] Add `developer.mcpPort` to registry (1024-65535)
- [ ] Implement Switch with restart indicator
- [ ] Implement NumberInput with validation
- [ ] Implement port availability auto-check on blur
- [ ] Implement "Find available port" button when port is taken
- [ ] Gray out port input when MCP disabled
- [ ] Wire to MCP server startup
- [ ] Write integration tests

### 10.2 Logging

- [ ] Add `developer.verboseLogging` to registry
- [ ] Implement Switch UI
- [ ] Wire to logger configuration
- [ ] Implement "Open log file" button (opens in Finder)
- [ ] Implement "Copy diagnostic info" button with toast feedback
- [ ] Write integration test

---

## Phase 11: Advanced section

### 11.1 Generated UI

- [ ] Create `src/lib/settings/sections/AdvancedSection.svelte`
- [ ] Implement warning banner (spec §12.1)
- [ ] Implement "Reset all to defaults" button with confirmation
- [ ] Implement generated setting rows from registry
- [ ] Filter registry for `showInAdvanced: true` settings
- [ ] Map types to components (spec §12.3)
- [ ] Implement scrollable container (unlike other sections)

### 11.2 Advanced settings

- [ ] Add `advanced.dragThreshold` to registry (default 5px)
- [ ] Add `advanced.prefetchBufferSize` to registry (default 200)
- [ ] Add `advanced.virtualizationBufferRows` to registry (default 20)
- [ ] Add `advanced.virtualizationBufferColumns` to registry (default 2)
- [ ] Add `advanced.fileWatcherDebounce` to registry (default 200ms)
- [ ] Add `advanced.serviceResolveTimeout` to registry (default 5s)
- [ ] Add `advanced.mountTimeout` to registry (default 20s)
- [ ] Add `advanced.updateCheckInterval` to registry (default 60min)
- [ ] Migrate each from hardcoded constants
- [ ] Wire each to consuming code
- [ ] Write integration tests

---

## Phase 12: Registry ↔ UI completeness check

- [ ] Create `scripts/check/settings-completeness.go` (or add to existing checker)
- [ ] Parse `settings-registry.ts` to extract all setting IDs
- [ ] Scan settings section components for setting ID references
- [ ] Verify every registry ID is referenced in at least one component
- [ ] Verify every component only references registered IDs
- [ ] Add to `./scripts/check.sh` pipeline
- [ ] Document check in `docs/tooling/settings-check.md`

---

## Phase 13: Accessibility

- [ ] Add visible focus states to all setting components
- [ ] Add ARIA labels to Switch/Toggle components
- [ ] Verify color contrast meets WCAG AA
- [ ] Test full keyboard navigation through all settings
- [ ] Test with VoiceOver (macOS screen reader)
- [ ] Implement focus trap in settings window

---

## Phase 14: Testing

### 14.1 Unit tests (Svelte/TypeScript)

- [ ] Registry functions: all CRUD operations
- [ ] Registry: constraint validation for all types
- [ ] Persistence: save/load cycle
- [ ] Persistence: schema migration
- [ ] Search: uFuzzy integration
- [ ] Search: filtering and highlighting
- [ ] Each setting component: render, change, validation
- [ ] Shortcuts: conflict detection
- [ ] Shortcuts: reset to defaults
- [ ] Run: `./scripts/check.sh --check svelte-tests`

### 14.2 Unit tests (Rust)

- [ ] Settings Tauri commands: get/set/reset
- [ ] Port checker: availability detection
- [ ] Port checker: find available port
- [ ] Run: `./scripts/check.sh --check rust-tests`

### 14.3 Integration tests

- [ ] Settings window opens with Cmd+,
- [ ] Settings window closes with ESC
- [ ] Settings persist across app restart
- [ ] Search filters tree correctly
- [ ] Each setting type applies immediately
- [ ] Restart indicator shows for MCP settings
- [ ] Keyboard shortcuts editor captures keys
- [ ] Shortcut conflicts are detected and handled
- [ ] Theme mode switches immediately
- [ ] Advanced section scrolls independently

### 14.4 E2E tests

- [ ] Add settings scenarios to `test/e2e-smoke/`
- [ ] Run: `./scripts/check.sh --check desktop-e2e`

---

## Phase 15: Documentation

- [ ] Create `docs/features/settings.md` with:
  - Overview of settings system
  - How to add a new setting (registry + UI + wiring)
  - How the completeness check works
  - Troubleshooting common issues
- [ ] Update `AGENTS.md` if settings affect agent workflows
- [ ] Add inline code comments where architecture is non-obvious

---

## Phase 16: Final verification

- [ ] Run full check suite: `./scripts/check.sh`
- [ ] Verify no regressions in existing functionality
- [ ] Manual smoke test of all settings
- [ ] Review for any TODO comments left in code
- [ ] Verify ADR 018 accurately reflects implementation

---

## Dependencies

```
Phase 1 (Foundation) ──┬── Phase 2 (Window) ──┬── Phase 3 (Components)
                       │                       │
                       │                       ├── Phase 4 (Appearance)
                       │                       ├── Phase 5 (File ops)
                       │                       ├── Phase 6 (Updates)
                       │                       ├── Phase 7 (Network)
                       │                       ├── Phase 8 (Shortcuts)
                       │                       ├── Phase 9 (Themes)
                       │                       ├── Phase 10 (Developer)
                       │                       └── Phase 11 (Advanced)
                       │
                       └── Phase 12 (Completeness check)

All implementation phases ── Phase 13 (Accessibility)
                         ── Phase 14 (Testing)
                         ── Phase 15 (Documentation)
                         ── Phase 16 (Final verification)
```

---

## Estimated scope

- **New files**: ~30 Svelte components, ~5 TypeScript modules, ~3 Rust modules
- **Modified files**: ~15 existing files for wiring settings
- **Tests**: ~50 unit tests, ~10 integration tests, ~5 E2E scenarios
- **Documentation**: 3 new docs, 1 updated doc
