/**
 * Pure-helper tests for the viewer's keyboard plumbing. Each helper returns
 * `true` when it consumed the event, `false` when the caller should fall
 * through to another handler.
 */

import { afterEach, describe, expect, it, vi } from 'vitest'

import { createViewerKeyboard, handleSearchToggleKey, handleTailToggleKey, handleToggleKey } from './viewer-keyboard'

function makeKey(props: Partial<KeyboardEventInit & { key: string }>): KeyboardEvent {
  return new KeyboardEvent('keydown', { key: 'a', ...props })
}

describe('handleTailToggleKey', () => {
  it('toggles on unmodified `F`', () => {
    const toggle = vi.fn()
    const handled = handleTailToggleKey(makeKey({ key: 'F' }), toggle)
    expect(handled).toBe(true)
    expect(toggle).toHaveBeenCalledOnce()
  })

  it('toggles on unmodified lower-case `f`', () => {
    const toggle = vi.fn()
    const handled = handleTailToggleKey(makeKey({ key: 'f' }), toggle)
    expect(handled).toBe(true)
    expect(toggle).toHaveBeenCalledOnce()
  })

  it('does NOT trigger when meta/ctrl/alt/shift is held', () => {
    const toggle = vi.fn()
    for (const mod of ['metaKey', 'ctrlKey', 'altKey', 'shiftKey'] as const) {
      const handled = handleTailToggleKey(makeKey({ key: 'f', [mod]: true }), toggle)
      expect(handled).toBe(false)
    }
    expect(toggle).not.toHaveBeenCalled()
  })

  it('ignores other keys', () => {
    const toggle = vi.fn()
    expect(handleTailToggleKey(makeKey({ key: 't' }), toggle)).toBe(false)
    expect(toggle).not.toHaveBeenCalled()
  })
})

describe('handleToggleKey (word wrap on `W`)', () => {
  it('toggles on unmodified `w`', () => {
    const toggle = vi.fn()
    expect(handleToggleKey(makeKey({ key: 'w' }), toggle)).toBe(true)
    expect(toggle).toHaveBeenCalledOnce()
  })

  it('does NOT trigger when meta is held', () => {
    const toggle = vi.fn()
    expect(handleToggleKey(makeKey({ key: 'w', metaKey: true }), toggle)).toBe(false)
    expect(toggle).not.toHaveBeenCalled()
  })
})

describe('handleSearchToggleKey', () => {
  it('toggles regex on ⌘⌥R', () => {
    const toggleUseRegex = vi.fn()
    const toggleCaseSensitive = vi.fn()
    const handled = handleSearchToggleKey(makeKey({ key: 'r', metaKey: true, altKey: true }), {
      toggleUseRegex,
      toggleCaseSensitive,
    })
    expect(handled).toBe(true)
    expect(toggleUseRegex).toHaveBeenCalledOnce()
    expect(toggleCaseSensitive).not.toHaveBeenCalled()
  })

  it('toggles case-sensitive on ⌘⌥C', () => {
    const toggleUseRegex = vi.fn()
    const toggleCaseSensitive = vi.fn()
    const handled = handleSearchToggleKey(makeKey({ key: 'c', metaKey: true, altKey: true }), {
      toggleUseRegex,
      toggleCaseSensitive,
    })
    expect(handled).toBe(true)
    expect(toggleCaseSensitive).toHaveBeenCalledOnce()
  })

  it('does NOT fire without alt', () => {
    const toggleUseRegex = vi.fn()
    const toggleCaseSensitive = vi.fn()
    const handled = handleSearchToggleKey(makeKey({ key: 'r', metaKey: true }), {
      toggleUseRegex,
      toggleCaseSensitive,
    })
    expect(handled).toBe(false)
  })
})

describe('createViewerKeyboard: ⌘C with the search bar open', () => {
  /** Wires the router against a focused search input holding `query`. */
  function wireWithFocusedSearchInput(query: string) {
    const input = document.createElement('input')
    input.type = 'search'
    input.value = query
    document.body.append(input)
    input.focus()

    const runCopy = vi.fn()
    const noop = vi.fn()
    const keyboard = createViewerKeyboard({
      getTotalLines: () => 10,
      getTotalBytes: () => 100,
      getLineText: () => 'line',
      selection: { selectAll: noop },
      scroll: { scrollByLines: noop, scrollByPages: noop, scrollToStart: noop, scrollToEnd: noop },
      search: {
        searchVisible: true,
        searchStatus: 'idle',
        searchInputRef: input,
        openSearch: noop,
        closeSearch: noop,
        stopSearch: noop,
        findNext: noop,
        findPrev: noop,
        toggleUseRegex: noop,
        toggleCaseSensitive: noop,
      },
      copy: { busy: false, cancelInFlight: () => Promise.resolve() },
      isCopyConfirmOpen: () => false,
      isCopyRefuseOpen: () => false,
      isContextMenuOpen: () => false,
      cancelCopyConfirm: noop,
      dismissCopyRefuse: noop,
      closeContextMenu: noop,
      logEscape: noop,
      runCopy,
      toggleTailMode: noop,
      toggleWordWrap: noop,
      closeWindow: noop,
    })
    return { keyboard, input, runCopy }
  }

  afterEach(() => {
    document.body.innerHTML = ''
  })

  it('copies the file selection when the input holds a bare caret', () => {
    const { keyboard, input, runCopy } = wireWithFocusedSearchInput('needle')
    input.setSelectionRange(6, 6)

    const e = makeKey({ key: 'c', metaKey: true, cancelable: true })
    keyboard.handleKeyDown(e)

    // The query is typed but nothing in the input is selected, so the only thing the
    // user can mean is the selection they made in the file.
    expect(runCopy).toHaveBeenCalledOnce()
    expect(e.defaultPrevented).toBe(true)
  })

  it('leaves ⌘C to the input when its own text is selected', () => {
    const { keyboard, input, runCopy } = wireWithFocusedSearchInput('needle')
    input.setSelectionRange(0, 6)

    const e = makeKey({ key: 'c', metaKey: true, cancelable: true })
    keyboard.handleKeyDown(e)

    expect(runCopy).not.toHaveBeenCalled()
    expect(e.defaultPrevented).toBe(false)
  })
})
