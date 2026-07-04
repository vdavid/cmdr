/**
 * Tests for `cursor-nav-keys.ts`, the Brief/Full list cursor movement glue. They
 * pin: applyNavigation's shift-extend + commit + scroll, the Full-mode arrow math
 * (up/down clamp + overflow, left/right jump-to-edge), the Page/Home/End delegation
 * to `handleNavigationShortcut`, the `⌘←`/`⌘→` bail (Copy-path-between-panes), and
 * the Brief-mode delegation to the list's own key navigation.
 *
 * `handleNavigationShortcut` is mocked so arrow keys fall through to the explicit
 * handling; the list refs are stubbed with scroll/nav spies.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest'
import type { ListViewAPI } from './types'

const { shortcutSpy } = vi.hoisted(() => ({ shortcutSpy: vi.fn() }))
vi.mock('../navigation/keyboard-shortcuts', () => ({ handleNavigationShortcut: shortcutSpy }))

import { createCursorNavKeys, type CursorNavKeysDeps } from './cursor-nav-keys'

function setup(over: Partial<CursorNavKeysDeps> = {}) {
  const spies = {
    applyCursor: vi.fn(),
    extendSelection: vi.fn(),
    briefScroll: vi.fn(),
    fullScroll: vi.fn(),
    briefNav: vi.fn(),
  }
  const briefListRef = {
    scrollToIndex: spies.briefScroll,
    handleKeyNavigation: spies.briefNav,
  } as unknown as ListViewAPI
  const fullListRef = {
    scrollToIndex: spies.fullScroll,
    getVisibleItemsCount: () => 20,
  } as unknown as ListViewAPI
  const deps: CursorNavKeysDeps = {
    getCursorIndex: () => 5,
    applyCursor: spies.applyCursor,
    extendSelection: spies.extendSelection,
    getHasParent: () => true,
    getEffectiveTotalCount: () => 10,
    getBriefListRef: () => briefListRef,
    getFullListRef: () => fullListRef,
    ...over,
  }
  return { nav: createCursorNavKeys(deps), spies }
}

function key(name: string, opts: { metaKey?: boolean; shiftKey?: boolean } = {}) {
  const preventDefault = vi.fn()
  return {
    e: { key: name, metaKey: !!opts.metaKey, shiftKey: !!opts.shiftKey, preventDefault } as unknown as KeyboardEvent,
    preventDefault,
  }
}

describe('createCursorNavKeys', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    shortcutSpy.mockReturnValue(null)
  })

  it('applyNavigation commits the index and scrolls; extends selection only on shift', () => {
    const { nav, spies } = setup()
    const listRef = { scrollToIndex: vi.fn() }
    nav.applyNavigation(7, listRef, false, false)
    expect(spies.extendSelection).not.toHaveBeenCalled()
    expect(spies.applyCursor).toHaveBeenCalledWith(7)
    expect(listRef.scrollToIndex).toHaveBeenCalledWith(7)

    nav.applyNavigation(3, listRef, true, true)
    expect(spies.extendSelection).toHaveBeenCalledWith(5, 3, true, true) // from cursor 5, hasParent true
  })

  it('Full mode ArrowDown moves down one, clamped at the last row (overflow)', () => {
    const { nav, spies } = setup({ getCursorIndex: () => 5, getEffectiveTotalCount: () => 10 })
    const { e, preventDefault } = key('ArrowDown')
    expect(nav.handleFullModeKeys(e)).toBe(true)
    expect(preventDefault).toHaveBeenCalled()
    expect(spies.applyCursor).toHaveBeenCalledWith(6)

    spies.applyCursor.mockClear()
    const { nav: nav2, spies: s2 } = setup({ getCursorIndex: () => 9, getEffectiveTotalCount: () => 10 })
    nav2.handleFullModeKeys(key('ArrowDown').e)
    expect(s2.applyCursor).toHaveBeenCalledWith(9) // clamped
  })

  it('Full mode ArrowUp moves up one, clamped at 0', () => {
    const { nav, spies } = setup({ getCursorIndex: () => 0 })
    nav.handleFullModeKeys(key('ArrowUp').e)
    expect(spies.applyCursor).toHaveBeenCalledWith(0)
  })

  it('Full mode ArrowLeft/ArrowRight jump to first/last', () => {
    const { nav, spies } = setup({ getEffectiveTotalCount: () => 10 })
    nav.handleFullModeKeys(key('ArrowLeft').e)
    expect(spies.applyCursor).toHaveBeenCalledWith(0)
    nav.handleFullModeKeys(key('ArrowRight').e)
    expect(spies.applyCursor).toHaveBeenCalledWith(9)
  })

  it('Full mode delegates Page/Home/End to handleNavigationShortcut', () => {
    shortcutSpy.mockReturnValue({ newIndex: 2, overflow: false })
    const { nav, spies } = setup()
    expect(nav.handleFullModeKeys(key('PageUp').e)).toBe(true)
    expect(spies.applyCursor).toHaveBeenCalledWith(2)
  })

  it('bails on ⌘← / ⌘→ (Copy path between panes owns those)', () => {
    const { nav, spies } = setup()
    expect(nav.handleFullModeKeys(key('ArrowLeft', { metaKey: true }).e)).toBe(false)
    expect(nav.handleBriefModeKeys(key('ArrowRight', { metaKey: true }).e)).toBe(false)
    expect(spies.applyCursor).not.toHaveBeenCalled()
  })

  it('Brief mode delegates to the list key navigation and applies the result', () => {
    const { nav, spies } = setup()
    spies.briefNav.mockReturnValue({ newIndex: 4, overflow: false })
    const { e, preventDefault } = key('ArrowDown')
    expect(nav.handleBriefModeKeys(e)).toBe(true)
    expect(preventDefault).toHaveBeenCalled()
    expect(spies.applyCursor).toHaveBeenCalledWith(4)
    expect(spies.briefScroll).toHaveBeenCalledWith(4)
  })

  it('Brief mode returns false when the list does not handle the key', () => {
    const { nav, spies } = setup()
    spies.briefNav.mockReturnValue(undefined)
    expect(nav.handleBriefModeKeys(key('Tab').e)).toBe(false)
    expect(spies.applyCursor).not.toHaveBeenCalled()
  })
})
