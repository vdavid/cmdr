import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'

// Shared test state: the mock factory closures capture these references
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
    // Tier 2: showInPalette false, basic nav; should NOT be in dispatch
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
    // The ⌥⌘A regression's own command: a ⌘-carrying combo the matchers test against.
    {
      id: 'selection.selectAll',
      name: 'Select all',
      scope: 'Main window/File list',
      showInPalette: false,
      shortcuts: ['⌘A'],
    },
    // A Shift+digit default: `⇧8` is never what a layout types, so the matcher's
    // physical-key fallback is what makes it bindable at all.
    {
      id: 'selection.invert',
      name: 'Invert selection',
      scope: 'Main window/File list',
      showInPalette: true,
      shortcuts: ['⇧8'],
    },
  ],
}))

import { getEffectiveShortcuts, onShortcutChange } from './shortcuts-store'
import { commands } from '$lib/commands/command-registry'
import {
  lookupCommand,
  initShortcutDispatch,
  destroyShortcutDispatch,
  comboMatchesCommand,
  eventMatchesCommand,
} from './shortcut-dispatch'

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

    it('a more specific scope wins a shared combo regardless of registry order', () => {
      // app.quit (scope App) sits EARLIER in the registry than file.copy
      // (Main window/File list). With both claiming F5, the deeper scope must
      // win — not whichever happens to be declared first.
      customOverrides.set('app.quit', ['F5'])
      initShortcutDispatch()
      expect(lookupCommand('F5')).toBe('file.copy')
    })

    it('equal specificity falls back to registry order (stable, pinned)', () => {
      // file.rename and file.copy share the Main window/File list scope;
      // rename is declared first, so it keeps the combo.
      customOverrides.set('file.rename', ['F5'])
      initShortcutDispatch()
      expect(lookupCommand('F5')).toBe('file.rename')
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

  describe('comboMatchesCommand', () => {
    it('matches the command exactly', () => {
      expect(comboMatchesCommand('⌘A', 'selection.selectAll')).toBe(true)
    })

    it('rejects a modifier SUPERSET (the ⌥⌘A regression)', () => {
      // Pressing ⌥⌘A opens Ask Cmdr. FilePane's old `e.key === 'a' && e.metaKey` was
      // also true for it, so the pane selected every file on the way. Nothing that
      // resolves through here may ever accept a longer combo.
      expect(comboMatchesCommand('⌥⌘A', 'selection.selectAll')).toBe(false)
      expect(comboMatchesCommand('⌘⌥A', 'selection.selectAll')).toBe(false)
      expect(comboMatchesCommand('⌘⌃A', 'selection.selectAll')).toBe(false)
      expect(comboMatchesCommand('⌘⇧A', 'selection.selectAll')).toBe(false)
    })

    it('rejects a Shift superset unless allowShift is passed', () => {
      expect(comboMatchesCommand('⇧↓', 'nav.down')).toBe(false)
      expect(comboMatchesCommand('⇧↓', 'nav.down', { allowShift: true })).toBe(true)
    })

    it('strips Shift from a combo that carries other modifiers too', () => {
      // `formatKeyCombo` emits ⌘⌃⌥⇧ order, so `⇧` leads only when it's the ONLY
      // modifier. A strip anchored at the start would silently do nothing here and
      // report "no match" — the quiet failure `allowShift` exists to prevent.
      expect(comboMatchesCommand('⌘⇧A', 'selection.selectAll', { allowShift: true })).toBe(true)
    })

    it('strips the non-mac `Shift+` form the same way', () => {
      customOverrides.set('selection.selectAll', ['Ctrl+A'])
      expect(comboMatchesCommand('Ctrl+Shift+A', 'selection.selectAll', { allowShift: true })).toBe(true)
      expect(comboMatchesCommand('Ctrl+Shift+A', 'selection.selectAll')).toBe(false)
    })

    it('allowShift never widens beyond Shift', () => {
      expect(comboMatchesCommand('⌘⌥A', 'selection.selectAll', { allowShift: true })).toBe(false)
    })

    it('follows a rebind, not the registry default', () => {
      customOverrides.set('selection.selectAll', ['⌘E'])
      expect(comboMatchesCommand('⌘E', 'selection.selectAll')).toBe(true)
      expect(comboMatchesCommand('⌘A', 'selection.selectAll')).toBe(false)
    })
  })

  describe('eventMatchesCommand', () => {
    // `formatKeyCombo` emits ⌘-form modifiers only when `isMacOS()` is true, and
    // happy-dom reports a Linux UA.
    const navigatorSpy = vi.spyOn(globalThis, 'navigator', 'get')
    beforeEach(() => {
      navigatorSpy.mockReturnValue({ userAgent: 'Mozilla/5.0 (Macintosh; Intel Mac OS X)' } as Navigator)
    })
    afterEach(() => navigatorSpy.mockReset())

    it('accepts the exact keypress and refuses the ⌥⌘A superset', () => {
      const cmdA = new KeyboardEvent('keydown', { key: 'a', metaKey: true })
      const optCmdA = new KeyboardEvent('keydown', { key: 'a', metaKey: true, altKey: true })

      expect(eventMatchesCommand(cmdA, 'selection.selectAll')).toBe(true)
      expect(eventMatchesCommand(optCmdA, 'selection.selectAll')).toBe(false)
    })

    it('matches a ⇧<digit> default by the physical key, whatever the layout types', () => {
      const usStar = new KeyboardEvent('keydown', { key: '*', code: 'Digit8', shiftKey: true })
      const huParen = new KeyboardEvent('keydown', { key: '(', code: 'Digit8', shiftKey: true })
      expect(eventMatchesCommand(usStar, 'selection.invert')).toBe(true)
      expect(eventMatchesCommand(huParen, 'selection.invert')).toBe(true)
    })

    it('keeps the fallback exact: no Shift, another digit, or an extra modifier is not ⇧8', () => {
      const plainEight = new KeyboardEvent('keydown', { key: '8', code: 'Digit8' })
      const shiftSeven = new KeyboardEvent('keydown', { key: '&', code: 'Digit7', shiftKey: true })
      const cmdShiftEight = new KeyboardEvent('keydown', { key: '*', code: 'Digit8', shiftKey: true, metaKey: true })
      expect(eventMatchesCommand(plainEight, 'selection.invert')).toBe(false)
      expect(eventMatchesCommand(shiftSeven, 'selection.invert')).toBe(false)
      expect(eventMatchesCommand(cmdShiftEight, 'selection.invert')).toBe(false)
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
