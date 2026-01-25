/**
 * Tests for the keyboard shortcuts system.
 */

import { describe, it, expect } from 'vitest'
import { getActiveScopes, scopesOverlap, getAllScopes } from './scope-hierarchy'
import { formatKeyCombo, normalizeKeyName, isModifierKey, matchesShortcut, isCompleteCombo } from './key-capture'

// ============================================================================
// Scope hierarchy tests
// ============================================================================

describe('scope-hierarchy', () => {
    describe('getActiveScopes', () => {
        it('returns only App for App scope', () => {
            const scopes = getActiveScopes('App')
            expect(scopes).toEqual(['App'])
        })

        it('returns Main window and App for Main window scope', () => {
            const scopes = getActiveScopes('Main window')
            expect(scopes).toEqual(['Main window', 'App'])
        })

        it('returns File list, Main window, and App for File list scope', () => {
            const scopes = getActiveScopes('File list')
            expect(scopes).toEqual(['File list', 'Main window', 'App'])
        })

        it('returns Command palette, Main window, and App for Command palette scope', () => {
            const scopes = getActiveScopes('Command palette')
            expect(scopes).toEqual(['Command palette', 'Main window', 'App'])
        })

        it('returns About window and App for About window scope', () => {
            const scopes = getActiveScopes('About window')
            expect(scopes).toEqual(['About window', 'App'])
        })
    })

    describe('scopesOverlap', () => {
        it('App overlaps with everything', () => {
            expect(scopesOverlap('App', 'App')).toBe(true)
            expect(scopesOverlap('App', 'Main window')).toBe(true)
            expect(scopesOverlap('App', 'File list')).toBe(true)
            expect(scopesOverlap('App', 'About window')).toBe(true)
        })

        it('File list overlaps with Main window', () => {
            expect(scopesOverlap('File list', 'Main window')).toBe(true)
            expect(scopesOverlap('Main window', 'File list')).toBe(true)
        })

        it('File list does not overlap with About window', () => {
            expect(scopesOverlap('File list', 'About window')).toBe(false)
            expect(scopesOverlap('About window', 'File list')).toBe(false)
        })

        it('Command palette does not overlap with About window', () => {
            expect(scopesOverlap('Command palette', 'About window')).toBe(false)
        })

        it('Settings window does not overlap with Main window children', () => {
            expect(scopesOverlap('Settings window', 'File list')).toBe(false)
            expect(scopesOverlap('Settings window', 'Navigation')).toBe(false)
        })
    })

    describe('getAllScopes', () => {
        it('returns all defined scopes', () => {
            const scopes = getAllScopes()
            expect(scopes).toContain('App')
            expect(scopes).toContain('Main window')
            expect(scopes).toContain('File list')
            expect(scopes).toContain('Command palette')
            expect(scopes).toContain('About window')
            expect(scopes).toContain('Settings window')
            expect(scopes.length).toBeGreaterThanOrEqual(10)
        })
    })
})

// ============================================================================
// Key capture tests
// ============================================================================

describe('key-capture', () => {
    // Helper to create mock keyboard events
    function createKeyEvent(key: string, modifiers: Partial<KeyboardEvent> = {}): KeyboardEvent {
        return {
            key,
            metaKey: modifiers.metaKey ?? false,
            ctrlKey: modifiers.ctrlKey ?? false,
            altKey: modifiers.altKey ?? false,
            shiftKey: modifiers.shiftKey ?? false,
        } as KeyboardEvent
    }

    describe('normalizeKeyName', () => {
        it('uppercases single characters', () => {
            expect(normalizeKeyName('a')).toBe('A')
            expect(normalizeKeyName('z')).toBe('Z')
            expect(normalizeKeyName('p')).toBe('P')
        })

        it('keeps uppercase characters', () => {
            expect(normalizeKeyName('A')).toBe('A')
        })

        it('handles space specially', () => {
            expect(normalizeKeyName(' ')).toBe('Space')
        })

        it('passes through unknown special keys', () => {
            expect(normalizeKeyName('F1')).toBe('F1')
            expect(normalizeKeyName('F12')).toBe('F12')
        })
    })

    describe('isModifierKey', () => {
        it('returns true for modifier keys', () => {
            expect(isModifierKey('Meta')).toBe(true)
            expect(isModifierKey('Control')).toBe(true)
            expect(isModifierKey('Alt')).toBe(true)
            expect(isModifierKey('Shift')).toBe(true)
            expect(isModifierKey('OS')).toBe(true)
        })

        it('returns false for regular keys', () => {
            expect(isModifierKey('a')).toBe(false)
            expect(isModifierKey('Enter')).toBe(false)
            expect(isModifierKey('Escape')).toBe(false)
            expect(isModifierKey('F1')).toBe(false)
        })
    })

    describe('formatKeyCombo', () => {
        // Note: These tests assume non-macOS environment (userAgent check)
        // In a real test environment, we'd mock navigator.userAgent

        it('formats single key', () => {
            const event = createKeyEvent('p')
            const result = formatKeyCombo(event)
            // On non-macOS, just the key
            expect(result).toBe('P')
        })

        it('formats Ctrl+key', () => {
            const event = createKeyEvent('p', { ctrlKey: true })
            const result = formatKeyCombo(event)
            expect(result).toBe('Ctrl+P')
        })

        it('formats Ctrl+Shift+key', () => {
            const event = createKeyEvent('p', { ctrlKey: true, shiftKey: true })
            const result = formatKeyCombo(event)
            expect(result).toBe('Ctrl+Shift+P')
        })

        it('formats Ctrl+Alt+Shift+key', () => {
            const event = createKeyEvent('p', { ctrlKey: true, altKey: true, shiftKey: true })
            const result = formatKeyCombo(event)
            expect(result).toBe('Ctrl+Alt+Shift+P')
        })

        it('ignores pure modifier key presses', () => {
            const event = createKeyEvent('Control', { ctrlKey: true })
            const result = formatKeyCombo(event)
            expect(result).toBe('Ctrl')
        })
    })

    describe('matchesShortcut', () => {
        it('matches exact shortcut', () => {
            const event = createKeyEvent('p', { ctrlKey: true })
            expect(matchesShortcut(event, 'Ctrl+P')).toBe(true)
        })

        it('does not match different shortcut', () => {
            const event = createKeyEvent('p', { ctrlKey: true })
            expect(matchesShortcut(event, 'Ctrl+Q')).toBe(false)
        })

        it('does not match with different modifiers', () => {
            const event = createKeyEvent('p', { ctrlKey: true })
            expect(matchesShortcut(event, 'Ctrl+Shift+P')).toBe(false)
        })
    })

    describe('isCompleteCombo', () => {
        it('returns true for regular keys', () => {
            expect(isCompleteCombo(createKeyEvent('p'))).toBe(true)
            expect(isCompleteCombo(createKeyEvent('Enter'))).toBe(true)
        })

        it('returns false for modifier-only', () => {
            expect(isCompleteCombo(createKeyEvent('Control'))).toBe(false)
            expect(isCompleteCombo(createKeyEvent('Meta'))).toBe(false)
            expect(isCompleteCombo(createKeyEvent('Shift'))).toBe(false)
        })
    })
})
