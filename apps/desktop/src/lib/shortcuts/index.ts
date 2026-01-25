/**
 * Keyboard shortcuts module.
 * Re-exports all public APIs for shortcut customization.
 */

// Types
export type { KeyCombo, ShortcutConflict, CustomShortcutsData, SetShortcutResult } from './types'

// Scope hierarchy
export { getActiveScopes, scopesOverlap, getAllScopes, type CommandScope } from './scope-hierarchy'

// Key capture
export {
    formatKeyCombo,
    parseKeyCombo,
    normalizeKeyName,
    matchesShortcut,
    isModifierKey,
    isCompleteCombo,
    isMacOS,
} from './key-capture'

// Shortcuts store
export {
    initializeShortcuts,
    getCustomShortcuts,
    getEffectiveShortcuts,
    getDefaultShortcuts,
    isShortcutModified,
    setShortcut,
    addShortcut,
    removeShortcut,
    resetShortcut,
    resetAllShortcuts,
    onShortcutChange,
    forceSave,
} from './shortcuts-store'

// Conflict detection
export {
    findConflictsForShortcut,
    hasConflicts,
    getAllConflicts,
    getConflictCount,
    getConflictingCommandIds,
} from './conflict-detector'

// Keyboard handler
export { handleKeyDown, findCommandsWithShortcut } from './keyboard-handler'
