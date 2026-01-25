/**
 * Types for the keyboard shortcut customization system.
 * See docs/specs/shortcut-settings.md for full specification.
 */

/** Represents a parsed key combination */
export interface KeyCombo {
    meta: boolean
    ctrl: boolean
    alt: boolean
    shift: boolean
    key: string // Normalized key name (for example 'P', 'Backspace', 'F1')
}

/** A conflict between commands sharing the same shortcut */
export interface ShortcutConflict {
    shortcut: string
    commandIds: string[]
}

/** Custom shortcuts storage format */
export interface CustomShortcutsData {
    _schemaVersion: number
    shortcuts: Record<string, string[]>
}

/** Result of setting a shortcut - may include conflicts */
export interface SetShortcutResult {
    success: boolean
    conflicts?: ShortcutConflict
}
