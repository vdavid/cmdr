/**
 * Tests for the keyboard shortcuts system.
 */

import { describe, it, expect } from 'vitest'
import { getActiveScopes, scopesOverlap, getAllScopes } from './scope-hierarchy'
import { formatKeyCombo, normalizeKeyName, isModifierKey, matchesShortcut, isCompleteCombo } from './key-capture'
import { menuCommands } from './shortcuts-store'

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

    it('returns File list, Main window, and App for Main window/File list scope', () => {
      const scopes = getActiveScopes('Main window/File list')
      expect(scopes).toEqual(['Main window/File list', 'Main window', 'App'])
    })

    it('returns Command palette, Main window, and App for Command palette scope', () => {
      const scopes = getActiveScopes('Command palette')
      expect(scopes).toEqual(['Command palette', 'Main window', 'App'])
    })

    it('returns About window and App for About window scope', () => {
      const scopes = getActiveScopes('About window')
      expect(scopes).toEqual(['About window', 'App'])
    })

    it('returns Brief mode under File list, Main window, and App', () => {
      // The file list renders in Brief mode too, so a Brief-mode key sits *under*
      // the file list in the chain (and inherits Main window + App).
      const scopes = getActiveScopes('Main window/Brief mode')
      expect(scopes).toEqual(['Main window/Brief mode', 'Main window/File list', 'Main window', 'App'])
    })

    it('returns Full mode under File list, Main window, and App', () => {
      const scopes = getActiveScopes('Main window/Full mode')
      expect(scopes).toEqual(['Main window/Full mode', 'Main window/File list', 'Main window', 'App'])
    })

    it('returns Network as a sibling of File list (under Main window, App)', () => {
      // Sibling views replace the file list in a pane, so they sit beside it (not
      // under it): under Main window + App, but not under Main window/File list.
      const scopes = getActiveScopes('Main window/Network')
      expect(scopes).toEqual(['Main window/Network', 'Main window', 'App'])
    })

    it('returns Share browser as a sibling of File list (under Main window, App)', () => {
      const scopes = getActiveScopes('Main window/Share browser')
      expect(scopes).toEqual(['Main window/Share browser', 'Main window', 'App'])
    })

    it('returns Volume chooser as a sibling of File list (under Main window, App)', () => {
      const scopes = getActiveScopes('Main window/Volume chooser')
      expect(scopes).toEqual(['Main window/Volume chooser', 'Main window', 'App'])
    })

    it('returns empty array for an unknown scope', () => {
      expect(getActiveScopes('Nonexistent scope')).toEqual([])
    })
  })

  describe('scopesOverlap', () => {
    it('App overlaps with everything', () => {
      expect(scopesOverlap('App', 'App')).toBe(true)
      expect(scopesOverlap('App', 'Main window')).toBe(true)
      expect(scopesOverlap('App', 'Main window/File list')).toBe(true)
      expect(scopesOverlap('App', 'About window')).toBe(true)
    })

    it('File list overlaps with itself', () => {
      expect(scopesOverlap('Main window/File list', 'Main window/File list')).toBe(true)
    })

    it('File list overlaps with Main window', () => {
      expect(scopesOverlap('Main window/File list', 'Main window')).toBe(true)
      expect(scopesOverlap('Main window', 'Main window/File list')).toBe(true)
    })

    it('File list does not overlap with About window', () => {
      expect(scopesOverlap('Main window/File list', 'About window')).toBe(false)
      expect(scopesOverlap('About window', 'Main window/File list')).toBe(false)
    })

    it('Command palette does not overlap with About window', () => {
      expect(scopesOverlap('Command palette', 'About window')).toBe(false)
    })

    it('Brief mode overlaps with File list (the list renders in Brief mode)', () => {
      expect(scopesOverlap('Main window/Brief mode', 'Main window/File list')).toBe(true)
      expect(scopesOverlap('Main window/File list', 'Main window/Brief mode')).toBe(true)
    })

    it('Brief mode does NOT overlap with Full mode (mutually exclusive siblings)', () => {
      // The registry deliberately binds ←/→ in both modes; they never coexist, so
      // they must not be reported as conflicting.
      expect(scopesOverlap('Main window/Brief mode', 'Main window/Full mode')).toBe(false)
      expect(scopesOverlap('Main window/Full mode', 'Main window/Brief mode')).toBe(false)
    })

    it('Network does NOT overlap with File list (sibling views in a pane)', () => {
      expect(scopesOverlap('Main window/Network', 'Main window/File list')).toBe(false)
      expect(scopesOverlap('Main window/File list', 'Main window/Network')).toBe(false)
    })

    it('Volume chooser overlaps with Main window', () => {
      expect(scopesOverlap('Main window/Volume chooser', 'Main window')).toBe(true)
      expect(scopesOverlap('Main window', 'Main window/Volume chooser')).toBe(true)
    })

    it('an unknown scope does not overlap with anything', () => {
      expect(scopesOverlap('Nonexistent scope', 'App')).toBe(false)
      expect(scopesOverlap('App', 'Nonexistent scope')).toBe(false)
    })
  })

  describe('getAllScopes', () => {
    it('returns all defined scopes', () => {
      const scopes = getAllScopes()
      expect(scopes).toContain('App')
      expect(scopes).toContain('Main window')
      expect(scopes).toContain('Main window/File list')
      expect(scopes).toContain('Main window/Brief mode')
      expect(scopes).toContain('Main window/Full mode')
      expect(scopes).toContain('Main window/Network')
      expect(scopes).toContain('Main window/Share browser')
      expect(scopes).toContain('Main window/Volume chooser')
      expect(scopes).toContain('Command palette')
      expect(scopes).toContain('About window')
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

// ============================================================================
// menuCommands list (kept in sync with src-tauri/src/menu/{macos,linux}.rs)
// ============================================================================

describe('menuCommands', () => {
  it('includes the four Select menu commands so accelerator sync covers them', () => {
    // The Select menu contains Select all, Deselect all, Select files…, and Deselect
    // files… (Select all / Deselect all moved from Edit; the file-selection focus is
    // the reason — see `src-tauri/src/menu/CLAUDE.md` § Decisions).
    expect(menuCommands).toContain('selection.selectAll')
    expect(menuCommands).toContain('selection.deselectAll')
    expect(menuCommands).toContain('selection.selectFiles')
    expect(menuCommands).toContain('selection.deselectFiles')
  })

  it('includes both Go-menu jump commands so accelerator sync covers them', () => {
    // The Go menu contains "Go to path…" (⌘G) and "Go to latest download" (⌘J); both
    // are native menu items, so their accelerators must sync from custom shortcuts.
    expect(menuCommands).toContain('nav.goToPath')
    expect(menuCommands).toContain('downloads.goToLatest')
  })

  it('has no duplicate command IDs', () => {
    const unique = new Set(menuCommands)
    expect(unique.size).toBe(menuCommands.length)
  })
})
