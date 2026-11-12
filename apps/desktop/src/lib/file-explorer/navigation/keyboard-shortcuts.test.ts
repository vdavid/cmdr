/**
 * Tests for keyboard-shortcuts.ts
 */
import { describe, it, expect, vi, afterEach, beforeEach, onTestFinished } from 'vitest'
import { handleNavigationShortcut, type NavigationContext } from './keyboard-shortcuts'
import { resetShortcut, setShortcut, removeShortcut } from '#lib/shortcuts/shortcuts-store.js'
import { withPlatform } from '#lib/platform/test-platform.js'

vi.mock('#lib/ipc/bindings.js', () => ({
  commands: { updateMenuAccelerator: vi.fn(() => Promise.resolve({ status: 'ok' })) },
}))

/** Helper to create a keyboard event */
function createKeyboardEvent(
  key: string,
  options: { altKey?: boolean; metaKey?: boolean; shiftKey?: boolean } = {},
): KeyboardEvent {
  return new KeyboardEvent('keydown', { key, ...options })
}

/** Go to first file's first default key: ⌥↑ on macOS, Home on Windows and Linux. */
function firstHomeKey(platform: 'macos' | 'windows' | 'linux'): KeyboardEvent {
  return platform === 'macos' ? createKeyboardEvent('ArrowUp', { altKey: true }) : createKeyboardEvent('Home')
}

/**
 * The keys come from the registry (`nav.home`, `nav.end`, `nav.pageUp`, `nav.pageDown`), so a
 * rebind moves them in every list that uses this: both file views, the places browser, and the
 * servers hub. On macOS too: that's one of the macOS changes the keymap work makes on purpose.
 */
describe.each(['macos', 'windows', 'linux'] as const)('a rebind of the navigation commands on %s', (platform) => {
  const context: NavigationContext = { currentIndex: 50, totalCount: 100 }

  beforeEach(() => withPlatform(platform))
  afterEach(() => {
    resetShortcut('nav.home')
    resetShortcut('nav.pageDown')
  })

  it('moves Go to first file: a replaced key stops working, and the new one jumps', () => {
    setShortcut('nav.home', 0, 'F18')
    expect(handleNavigationShortcut(firstHomeKey(platform), context)).toBeNull()
    expect(handleNavigationShortcut(createKeyboardEvent('F18'), context)).toEqual({
      newIndex: 0,
      handled: true,
      overflow: true,
    })
  })

  it('removes a key: a removed one stops working, the rest still jump', () => {
    removeShortcut('nav.home', 0)
    expect(handleNavigationShortcut(firstHomeKey(platform), context)).toBeNull()
    if (platform === 'macos') {
      expect(handleNavigationShortcut(createKeyboardEvent('Home'), context)).toEqual({
        newIndex: 0,
        handled: true,
        overflow: true,
      })
    }
  })

  it('moves Page down to the new key, Shift still extending', () => {
    setShortcut('nav.pageDown', 0, 'F19')
    expect(handleNavigationShortcut(createKeyboardEvent('PageDown'), context)).toBeNull()
    expect(handleNavigationShortcut(createKeyboardEvent('F19', { shiftKey: true }), context)).toEqual({
      newIndex: 70,
      handled: true,
      overflow: false,
    })
  })
})

/** Off macOS, `Alt+↑` is Go to parent (Explorer's key) and `Alt+↓` is unbound, so neither jumps in a list. */
describe.each(['windows', 'linux'] as const)('Alt+arrows on %s', (platform) => {
  beforeEach(() => withPlatform(platform))

  it.each(['ArrowUp', 'ArrowDown'])('leave the cursor alone: Alt+%s', (key) => {
    const event = createKeyboardEvent(key, { altKey: true })
    expect(handleNavigationShortcut(event, { currentIndex: 50, totalCount: 100 })).toBeNull()
  })
})

describe('handleNavigationShortcut', () => {
  describe('Home shortcut', () => {
    const context: NavigationContext = {
      currentIndex: 50,
      totalCount: 100,
    }

    it('handles Option+ArrowUp as Home on macOS (always overflow)', () => {
      const undo = withPlatform('macos')
      onTestFinished(undo)
      const event = createKeyboardEvent('ArrowUp', { altKey: true })
      const result = handleNavigationShortcut(event, context)
      expect(result).toEqual({ newIndex: 0, handled: true, overflow: true })
    })

    it('handles Home key (always overflow)', () => {
      const event = createKeyboardEvent('Home')
      const result = handleNavigationShortcut(event, context)
      expect(result).toEqual({ newIndex: 0, handled: true, overflow: true })
    })

    it('does not handle Home with metaKey', () => {
      const event = createKeyboardEvent('Home', { metaKey: true })
      const result = handleNavigationShortcut(event, context)
      expect(result).toBeNull()
    })
  })

  describe('End shortcut', () => {
    const context: NavigationContext = {
      currentIndex: 50,
      totalCount: 100,
    }

    it('handles Option+ArrowDown as End on macOS (always overflow)', () => {
      const undo = withPlatform('macos')
      onTestFinished(undo)
      const event = createKeyboardEvent('ArrowDown', { altKey: true })
      const result = handleNavigationShortcut(event, context)
      expect(result).toEqual({ newIndex: 99, handled: true, overflow: true })
    })

    it('handles End key (always overflow)', () => {
      const event = createKeyboardEvent('End')
      const result = handleNavigationShortcut(event, context)
      expect(result).toEqual({ newIndex: 99, handled: true, overflow: true })
    })

    it('does not handle End with metaKey', () => {
      const event = createKeyboardEvent('End', { metaKey: true })
      const result = handleNavigationShortcut(event, context)
      expect(result).toBeNull()
    })

    it('handles End with empty list', () => {
      const emptyContext: NavigationContext = {
        currentIndex: 0,
        totalCount: 0,
      }
      const event = createKeyboardEvent('End')
      const result = handleNavigationShortcut(event, emptyContext)
      expect(result).toEqual({ newIndex: 0, handled: true, overflow: true })
    })
  })

  describe('PageUp in Full mode', () => {
    it('moves up by visible items (no overflow)', () => {
      const context: NavigationContext = {
        currentIndex: 50,
        totalCount: 100,
        visibleItems: 20,
      }
      const event = createKeyboardEvent('PageUp')
      const result = handleNavigationShortcut(event, context)
      // pageSize = max(1, 20 - 1) = 19
      expect(result).toEqual({ newIndex: 31, handled: true, overflow: false })
    })

    it('uses default page size when visibleItems not provided', () => {
      const context: NavigationContext = {
        currentIndex: 50,
        totalCount: 100,
      }
      const event = createKeyboardEvent('PageUp')
      const result = handleNavigationShortcut(event, context)
      // pageSize defaults to 20
      expect(result).toEqual({ newIndex: 30, handled: true, overflow: false })
    })

    it('clamps to 0 (overflow=true)', () => {
      const context: NavigationContext = {
        currentIndex: 5,
        totalCount: 100,
        visibleItems: 20,
      }
      const event = createKeyboardEvent('PageUp')
      const result = handleNavigationShortcut(event, context)
      expect(result).toEqual({ newIndex: 0, handled: true, overflow: true })
    })

    it('exact boundary (raw = 0) is not overflow', () => {
      const context: NavigationContext = {
        currentIndex: 19,
        totalCount: 100,
        visibleItems: 20,
      }
      // pageSize = 19, raw = 19 - 19 = 0 → not clamped
      const event = createKeyboardEvent('PageUp')
      const result = handleNavigationShortcut(event, context)
      expect(result).toEqual({ newIndex: 0, handled: true, overflow: false })
    })
  })

  describe('PageDown in Full mode', () => {
    it('moves down by visible items (no overflow)', () => {
      const context: NavigationContext = {
        currentIndex: 50,
        totalCount: 100,
        visibleItems: 20,
      }
      const event = createKeyboardEvent('PageDown')
      const result = handleNavigationShortcut(event, context)
      // pageSize = max(1, 20 - 1) = 19
      expect(result).toEqual({ newIndex: 69, handled: true, overflow: false })
    })

    it('uses default page size when visibleItems not provided', () => {
      const context: NavigationContext = {
        currentIndex: 50,
        totalCount: 100,
      }
      const event = createKeyboardEvent('PageDown')
      const result = handleNavigationShortcut(event, context)
      expect(result).toEqual({ newIndex: 70, handled: true, overflow: false })
    })

    it('clamps to totalCount - 1 (overflow=true)', () => {
      const context: NavigationContext = {
        currentIndex: 95,
        totalCount: 100,
        visibleItems: 20,
      }
      const event = createKeyboardEvent('PageDown')
      const result = handleNavigationShortcut(event, context)
      expect(result).toEqual({ newIndex: 99, handled: true, overflow: true })
    })

    it('exact boundary (raw = totalCount-1) is not overflow', () => {
      const context: NavigationContext = {
        currentIndex: 80,
        totalCount: 100,
        visibleItems: 20,
      }
      // pageSize = 19, raw = 80 + 19 = 99 = lastIndex → not clamped
      const event = createKeyboardEvent('PageDown')
      const result = handleNavigationShortcut(event, context)
      expect(result).toEqual({ newIndex: 99, handled: true, overflow: false })
    })
  })

  describe('PageUp in Brief mode', () => {
    // Brief mode: items arranged in columns
    // itemsPerColumn = 10, visibleColumns = 3
    // Layout: Col0[0-9], Col1[10-19], Col2[20-29], Col3[30-39], etc.

    it('moves left by visible columns minus 1 (no overflow)', () => {
      const context: NavigationContext = {
        currentIndex: 35, // Column 3, row 5
        totalCount: 50,
        itemsPerColumn: 10,
        visibleColumns: 3,
      }
      const event = createKeyboardEvent('PageUp')
      const result = handleNavigationShortcut(event, context)
      // columnsToMove = max(1, 3 - 1) = 2
      // currentColumn = 3, targetColumn = 1
      // targetColumnStart = 10, bottommost = min(49, 10 + 10 - 1) = 19
      expect(result).toEqual({ newIndex: 19, handled: true, overflow: false })
    })

    it('jumps to first item when near start (overflow=true)', () => {
      const context: NavigationContext = {
        currentIndex: 5, // Column 0, row 5
        totalCount: 50,
        itemsPerColumn: 10,
        visibleColumns: 3,
      }
      const event = createKeyboardEvent('PageUp')
      const result = handleNavigationShortcut(event, context)
      // targetColumn would be -2, which is <= 0, so jump to 0
      expect(result).toEqual({ newIndex: 0, handled: true, overflow: true })
    })

    it('jumps to first item from column 1 (overflow=true)', () => {
      const context: NavigationContext = {
        currentIndex: 15, // Column 1, row 5
        totalCount: 50,
        itemsPerColumn: 10,
        visibleColumns: 3,
      }
      const event = createKeyboardEvent('PageUp')
      const result = handleNavigationShortcut(event, context)
      // targetColumn = 1 - 2 = -1, which is <= 0
      expect(result).toEqual({ newIndex: 0, handled: true, overflow: true })
    })
  })

  describe('PageDown in Brief mode', () => {
    it('moves right by visible columns minus 1 (no overflow)', () => {
      const context: NavigationContext = {
        currentIndex: 15, // Column 1, row 5
        totalCount: 50,
        itemsPerColumn: 10,
        visibleColumns: 3,
      }
      const event = createKeyboardEvent('PageDown')
      const result = handleNavigationShortcut(event, context)
      // columnsToMove = max(1, 3 - 1) = 2
      // currentColumn = 1, targetColumn = 3
      // totalColumns = ceil(50/10) = 5, so 3 < 4 (last column index)
      // targetColumnStart = 30, bottommost = min(49, 30 + 10 - 1) = 39
      expect(result).toEqual({ newIndex: 39, handled: true, overflow: false })
    })

    it('jumps to last item when near end (overflow=true)', () => {
      const context: NavigationContext = {
        currentIndex: 35, // Column 3, row 5
        totalCount: 50,
        itemsPerColumn: 10,
        visibleColumns: 3,
      }
      const event = createKeyboardEvent('PageDown')
      const result = handleNavigationShortcut(event, context)
      // totalColumns = 5, targetColumn = 3 + 2 = 5 >= 4 (last column index)
      expect(result).toEqual({ newIndex: 49, handled: true, overflow: true })
    })

    it('handles partial last column (overflow=true)', () => {
      const context: NavigationContext = {
        currentIndex: 25, // Column 2, row 5
        totalCount: 45, // Last column only has 5 items
        itemsPerColumn: 10,
        visibleColumns: 3,
      }
      const event = createKeyboardEvent('PageDown')
      const result = handleNavigationShortcut(event, context)
      // totalColumns = ceil(45/10) = 5, targetColumn = 2 + 2 = 4 (last column)
      // 4 >= 4, so jump to last item
      expect(result).toEqual({ newIndex: 44, handled: true, overflow: true })
    })
  })

  describe('unhandled keys', () => {
    const context: NavigationContext = {
      currentIndex: 50,
      totalCount: 100,
    }

    it('returns null for ArrowUp without modifiers', () => {
      const event = createKeyboardEvent('ArrowUp')
      const result = handleNavigationShortcut(event, context)
      expect(result).toBeNull()
    })

    it('returns null for ArrowDown without modifiers', () => {
      const event = createKeyboardEvent('ArrowDown')
      const result = handleNavigationShortcut(event, context)
      expect(result).toBeNull()
    })

    it('returns null for Enter', () => {
      const event = createKeyboardEvent('Enter')
      const result = handleNavigationShortcut(event, context)
      expect(result).toBeNull()
    })

    it('returns null for letter keys', () => {
      const event = createKeyboardEvent('a')
      const result = handleNavigationShortcut(event, context)
      expect(result).toBeNull()
    })
  })
})
