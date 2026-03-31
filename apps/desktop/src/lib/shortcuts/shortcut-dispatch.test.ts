import { describe, it, expect, beforeEach, vi } from 'vitest'

// Shared test state — the mock factory closures capture these references
const listeners = new Set<(commandId: string) => void>()
const customOverrides = new Map<string, string[]>()

// Mock the shortcuts store before importing the module under test
vi.mock('./shortcuts-store', () => ({
  getEffectiveShortcuts: vi.fn(),
  onShortcutChange: vi.fn((listener: (commandId: string) => void) => {
    listeners.add(listener)
    return () => listeners.delete(listener)
  }),
}))

// Mock the command registry with a controlled set of commands
vi.mock('$lib/commands/command-registry', () => ({
  commands: [
    // Tier 1: showInPalette true, has shortcut
    { id: 'app.quit', name: 'Quit', scope: 'App', showInPalette: true, shortcuts: ['⌘Q'] },
    {
      id: 'file.rename',
      name: 'Rename',
      scope: 'Main window/File list',
      showInPalette: true,
      shortcuts: ['F2', '⇧F6'],
    },
    { id: 'file.copy', name: 'Copy', scope: 'Main window/File list', showInPalette: true, shortcuts: ['F5'] },
    { id: 'view.showHidden', name: 'Toggle hidden', scope: 'Main window', showInPalette: true, shortcuts: ['⌘⇧.'] },
    // Tier 1: showInPalette false but in ALWAYS_DISPATCH_IDS
    {
      id: 'app.commandPalette',
      name: 'Open command palette',
      scope: 'App',
      showInPalette: false,
      shortcuts: ['⌘⇧P'],
    },
    // Tier 2: showInPalette false, basic nav — should NOT be in dispatch
    {
      id: 'nav.up',
      name: 'Select previous',
      scope: 'Main window/File list',
      showInPalette: false,
      shortcuts: ['↑'],
    },
    { id: 'nav.down', name: 'Select next', scope: 'Main window/File list', showInPalette: false, shortcuts: ['↓'] },
    // Tier 2: palette-internal
    {
      id: 'palette.close',
      name: 'Close palette',
      scope: 'Command palette',
      showInPalette: false,
      shortcuts: ['Escape'],
    },
  ],
}))

import { getEffectiveShortcuts, onShortcutChange } from './shortcuts-store'
import { commands } from '$lib/commands/command-registry'
import { lookupCommand, initShortcutDispatch, destroyShortcutDispatch } from './shortcut-dispatch'

/**
 * Wire up getEffectiveShortcuts to return registry defaults
 * unless a custom override exists.
 */
function setupEffectiveShortcuts() {
  vi.mocked(getEffectiveShortcuts).mockImplementation((commandId: string) => {
    const override = customOverrides.get(commandId)
    if (override) {
      return [...override]
    }
    const cmd = commands.find((c) => c.id === commandId)
    return [...(cmd?.shortcuts ?? [])]
  })
}

describe('shortcut-dispatch', () => {
  beforeEach(() => {
    destroyShortcutDispatch()
    customOverrides.clear()
    listeners.clear()
    vi.clearAllMocks()
    setupEffectiveShortcuts()
  })

  describe('lookupCommand', () => {
    it('returns the correct command ID for a Tier 1 shortcut', () => {
      initShortcutDispatch()
      expect(lookupCommand('⌘Q')).toBe('app.quit')
    })

    it('handles commands with multiple shortcuts', () => {
      initShortcutDispatch()
      expect(lookupCommand('F2')).toBe('file.rename')
      expect(lookupCommand('⇧F6')).toBe('file.rename')
    })

    it('returns undefined for unregistered key combos', () => {
      initShortcutDispatch()
      expect(lookupCommand('⌘Z')).toBeUndefined()
      expect(lookupCommand('F12')).toBeUndefined()
    })

    it('returns undefined for Tier 2 (non-palette) command shortcuts', () => {
      initShortcutDispatch()
      // nav.up (↑) and nav.down (↓) have showInPalette: false
      expect(lookupCommand('↑')).toBeUndefined()
      expect(lookupCommand('↓')).toBeUndefined()
    })

    it('includes app.commandPalette despite showInPalette: false', () => {
      initShortcutDispatch()
      expect(lookupCommand('⌘⇧P')).toBe('app.commandPalette')
    })

    it('returns undefined before init is called', () => {
      // Map is empty before init
      expect(lookupCommand('⌘Q')).toBeUndefined()
    })
  })

  describe('custom shortcut overrides', () => {
    it('uses the new binding after a custom override', () => {
      initShortcutDispatch()

      // Override file.copy from F5 to F9
      customOverrides.set('file.copy', ['F9'])

      // Trigger the change listener
      for (const listener of listeners) {
        listener('file.copy')
      }

      expect(lookupCommand('F9')).toBe('file.copy')
      expect(lookupCommand('F5')).toBeUndefined()
    })

    it('handles adding a shortcut to a command that had none', () => {
      initShortcutDispatch()

      customOverrides.set('view.showHidden', ['⌘⇧.', '⌘⇧H'])

      for (const listener of listeners) {
        listener('view.showHidden')
      }

      expect(lookupCommand('⌘⇧H')).toBe('view.showHidden')
      expect(lookupCommand('⌘⇧.')).toBe('view.showHidden')
    })
  })

  describe('initShortcutDispatch', () => {
    it('subscribes to shortcut changes', () => {
      initShortcutDispatch()
      expect(onShortcutChange).toHaveBeenCalledOnce()
    })
  })

  describe('destroyShortcutDispatch', () => {
    it('clears the map after destroy', () => {
      initShortcutDispatch()
      expect(lookupCommand('⌘Q')).toBe('app.quit')

      destroyShortcutDispatch()
      expect(lookupCommand('⌘Q')).toBeUndefined()
    })
  })
})
