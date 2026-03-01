/**
 * Key capture and formatting utilities.
 * Platform-specific - stores shortcuts as display strings.
 */

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

/** Modifier symbols used in macOS shortcut format */
const macModifierToLinux: Record<string, string> = {
    '⌘': 'Ctrl',
    '⌥': 'Alt',
    '⇧': 'Shift',
    '⌃': 'Ctrl',
}

const macModifierSymbols = new Set(Object.keys(macModifierToLinux))

/**
 * Convert a macOS-format shortcut string to the current platform's format.
 * On macOS, returns as-is. On Linux, converts symbols to names with `+` separator.
 * Special case: when both ⌃ and ⌘ are present, one maps to Ctrl and the other to Shift
 * (since both would otherwise become Ctrl).
 */
export function toPlatformShortcut(shortcut: string): string {
    if (isMacOS()) return shortcut

    // Check if the shortcut contains any macOS modifier symbols
    const chars = Array.from(shortcut)
    const hasModifierSymbols = chars.some((ch) => macModifierSymbols.has(ch))
    if (!hasModifierSymbols) return shortcut

    // Parse the macOS symbol string character by character
    const modifiers: string[] = []
    let key = ''
    let hasCmdSymbol = false
    let hasCtrlSymbol = false

    for (const ch of chars) {
        if (macModifierSymbols.has(ch)) {
            if (ch === '⌘') hasCmdSymbol = true
            if (ch === '⌃') hasCtrlSymbol = true
            modifiers.push(ch)
        } else {
            key += ch
        }
    }

    // Build the Linux modifier list, handling the ⌃+⌘ collision
    const linuxModifiers: string[] = []
    const hasCollision = hasCmdSymbol && hasCtrlSymbol

    for (const mod of modifiers) {
        if (hasCollision && mod === '⌃') {
            // When both ⌃ and ⌘ are present, ⌃ maps to Shift instead of Ctrl
            linuxModifiers.push('Shift')
        } else {
            linuxModifiers.push(macModifierToLinux[mod])
        }
    }

    // Deduplicate modifiers while preserving order
    const seen = new Set<string>()
    const uniqueModifiers = linuxModifiers.filter((m) => {
        if (seen.has(m)) return false
        seen.add(m)
        return true
    })

    return [...uniqueModifiers, key].join('+')
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
