/**
 * Keyboard shortcuts module.
 * Re-exports all public APIs for shortcut customization.
 */

// Types
export type { ShortcutConflict } from './types'

// Scope hierarchy
export { getActiveScopes, scopesOverlap, getAllScopes, type CommandScope } from './scope-hierarchy'

// Key capture
export {
  formatKeyCombo,
  normalizeKeyName,
  matchesShortcut,
  isModifierKey,
  isCompleteCombo,
  isMacOS,
  toDisplayShortcut,
  toCanonicalShortcut,
} from './key-capture'

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
} from './shortcuts-store'

// Conflict detection
export {
  findConflictsForShortcut,
  getAllConflicts,
  getConflictCount,
  getConflictingCommandIds,
} from './conflict-detector'

// Event → command matching for local handlers (the document dispatcher imports
// `lookupCommand` / `init` / `destroy` from `shortcut-dispatch` directly).
export { eventMatchesCommand, comboMatchesCommand } from './shortcut-dispatch'

// MCP shortcuts listener
export { setupMcpShortcutsListener, cleanupMcpShortcutsListener } from './mcp-shortcuts-listener'
