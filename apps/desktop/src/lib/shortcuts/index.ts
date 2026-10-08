/**
 * Keyboard shortcuts module.
 * Re-exports all public APIs for shortcut customization.
 */

// Key capture
export { formatKeyCombo, capturedKeyCombo, isModifierKey, isMacOS, toDisplayShortcut } from './key-capture'

// Shortcuts store
export {
  initializeShortcuts,
  getEffectiveShortcuts,
  getDefaultShortcuts,
  isShortcutModified,
  setShortcut,
  addShortcut,
  removeShortcut,
  resetShortcut,
  resetAllShortcuts,
  onShortcutChange,
  flushPendingSave,
  isNativeShortcutCommand,
  isFixedKeyCommand,
  resyncMenuAccelerators,
} from './shortcuts-store'

// The whole registry as one map, for the native popup menus' accelerator labels
export { boundShortcuts } from './bound-shortcuts'

// Conflict detection
export { findConflictsForShortcut, getConflictCount, getConflictingCommandIds } from './conflict-detector'

// Event → command matching for local handlers (the document dispatcher imports
// `lookupCommand` / `init` / `destroy` from `shortcut-dispatch` directly).
export { eventMatchesCommand, comboMatchesCommand, resolveKeyCombo } from './shortcut-dispatch'

// ❌ `claimKey` is deliberately NOT re-exported here. A local handler imports it from
// the leaf `$lib/shortcuts/claim-key`, so a test that mocks this barrel can't replace
// the claim with a no-op and hide a double-dispatch. See `claim-key.ts`.

// MCP shortcuts listener
export { setupMcpShortcutsListener, cleanupMcpShortcutsListener } from './mcp-shortcuts-listener'
