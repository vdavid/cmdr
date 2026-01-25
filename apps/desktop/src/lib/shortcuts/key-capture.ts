/**
 * Key capture and formatting utilities.
 * Platform-specific - stores shortcuts as display strings.
 * See docs/specs/shortcut-settings.md §4 for specification.
 */

import type { KeyCombo } from './types'

/** Check if running on macOS */
export function isMacOS(): boolean {
    if (typeof navigator === 'undefined') return false
    return navigator.userAgent.toLowerCase().includes('mac')
}

/** Special key name mappings */
const macKeyNames: Record<string, string> = {
    Backspace: '⌫',
    Delete: '⌦',
    Enter: '↩',
    Return: '↩',
    Escape: '⎋',
    Tab: 'Tab',
    ArrowUp: '↑',
    ArrowDown: '↓',
    ArrowLeft: '←',
    ArrowRight: '→',
    ' ': 'Space',
    PageUp: 'PgUp',
    PageDown: 'PgDn',
    Home: 'Home',
    End: 'End',
}

const nonMacKeyNames: Record<string, string> = {
    Backspace: 'Backspace',
    Delete: 'Delete',
    Enter: 'Enter',
    Return: 'Enter',
    Escape: 'Esc',
    Tab: 'Tab',
    ArrowUp: '↑',
    ArrowDown: '↓',
    ArrowLeft: '←',
    ArrowRight: '→',
    ' ': 'Space',
    PageUp: 'PgUp',
    PageDown: 'PgDn',
    Home: 'Home',
    End: 'End',
}

/**
 * Normalize a key name for display.
 * Single characters are uppercased, special keys are mapped.
 */
export function normalizeKeyName(key: string): string {
    // Single printable characters are uppercased
    if (key.length === 1 && key !== ' ') {
        return key.toUpperCase()
    }

    const keyMap = isMacOS() ? macKeyNames : nonMacKeyNames
    return keyMap[key] ?? key
}

/**
 * Check if a key is a modifier (should not be captured alone).
 */
export function isModifierKey(key: string): boolean {
    return ['Meta', 'Control', 'Alt', 'Shift', 'OS'].includes(key)
}

/**
 * Format a keyboard event into a display string.
 * macOS: ⌘⇧P
 * Windows/Linux: Ctrl+Shift+P
 */
export function formatKeyCombo(event: KeyboardEvent): string {
    const parts: string[] = []

    if (isMacOS()) {
        if (event.metaKey) parts.push('⌘')
        if (event.ctrlKey) parts.push('⌃')
        if (event.altKey) parts.push('⌥')
        if (event.shiftKey) parts.push('⇧')
    } else {
        if (event.ctrlKey) parts.push('Ctrl')
        if (event.altKey) parts.push('Alt')
        if (event.shiftKey) parts.push('Shift')
        if (event.metaKey) parts.push('Win')
    }

    // Don't include modifier keys themselves as the main key
    if (!isModifierKey(event.key)) {
        const key = normalizeKeyName(event.key)
        parts.push(key)
    }

    return isMacOS() ? parts.join('') : parts.join('+')
}

/**
 * Parse a KeyCombo from a keyboard event.
 */
export function parseKeyCombo(event: KeyboardEvent): KeyCombo {
    return {
        meta: event.metaKey,
        ctrl: event.ctrlKey,
        alt: event.altKey,
        shift: event.shiftKey,
        key: isModifierKey(event.key) ? '' : normalizeKeyName(event.key),
    }
}

/**
 * Check if a keyboard event matches a stored shortcut string.
 */
export function matchesShortcut(event: KeyboardEvent, shortcut: string): boolean {
    return formatKeyCombo(event) === shortcut
}

/**
 * Check if a key combo is complete (has a non-modifier key).
 */
export function isCompleteCombo(event: KeyboardEvent): boolean {
    return !isModifierKey(event.key)
}
