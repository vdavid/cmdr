/**
 * Keyboard shortcuts module.
 * Re-exports all public APIs for shortcut customization.
 */

// Key capture
export { formatKeyCombo, isModifierKey, isMacOS, toDisplayShortcut } from './key-capture'

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

// Conflict detection
export { findConflictsForShortcut, getConflictCount, getConflictingCommandIds } from './conflict-detector'

// Event → command matching for local handlers (the document dispatcher imports
// `lookupCommand` / `init` / `destroy` from `shortcut-dispatch` directly).
export { eventMatchesCommand, comboMatchesCommand } from './shortcut-dispatch'

// MCP shortcuts listener
export { setupMcpShortcutsListener, cleanupMcpShortcutsListener } from './mcp-shortcuts-listener'
