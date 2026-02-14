/**
 * Types for the keyboard shortcut customization system.
 */

/** A conflict between commands sharing the same shortcut */
export interface ShortcutConflict {
    shortcut: string
    commandIds: string[]
}
