/**
 * Pure-helper tests for the viewer's keyboard plumbing. Each helper returns
 * `true` when it consumed the event, `false` when the caller should fall
 * through to another handler.
 */

import { afterEach, describe, expect, it, vi } from 'vitest'

import { createViewerSelection, isWholeFileSelection, toRangeEnds } from './selection.svelte'
import { createViewerKeyboard, handleSearchToggleKey, handleTailToggleKey, handleToggleKey } from './viewer-keyboard'

type KeyboardDeps = Parameters<typeof createViewerKeyboard>[0]

function makeKey(props: Partial<KeyboardEventInit & { key: string }>): KeyboardEvent {
  return new KeyboardEvent('keydown', { key: 'a', ...props })
}

/**
 * A fully wired `createViewerKeyboard` dep set whose every action is a spy, so a test
 * only has to name the handful of deps it cares about. Defaults describe a 10-line
 * file with the search bar closed and nothing in flight.
 */
function makeKeyboardDeps(overrides: Partial<KeyboardDeps> = {}): KeyboardDeps {
  const noop = vi.fn()
  return {
    getTotalLines: () => 10,
    getTotalBytes: () => 100,
    getLineText: () => 'line',
    selection: { selectAll: noop, selectToEof: noop },
    scroll: { scrollByLines: noop, scrollByPages: noop, scrollToStart: noop, scrollToEnd: noop },
    search: {
      searchVisible: false,
      searchStatus: 'idle',
      searchInputRef: null,
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
    runCopy: noop,
    toggleTailMode: noop,
    toggleWordWrap: noop,
    closeWindow: noop,
    ...overrides,
  }
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
    const base = makeKeyboardDeps({ runCopy })
    const keyboard = createViewerKeyboard({
      ...base,
      search: { ...base.search, searchVisible: true, searchInputRef: input },
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

describe('createViewerKeyboard: ⌘A in ByteSeek-no-index mode', () => {
  it('mints an end-of-file selection that both the copy-size shortcut and the IPC mapper recognise', () => {
    const selection = createViewerSelection()
    const keyboard = createViewerKeyboard(
      makeKeyboardDeps({
        // No index yet, so there is no line count to select up to; the file is non-empty.
        getTotalLines: () => null,
        getTotalBytes: () => 1,
        selection: { selectAll: selection.selectAll, selectToEof: selection.selectToEof },
      }),
    )

    keyboard.handleSelectAllShortcut()

    // The copy flow must recognise this as the whole file and use the known file size;
    // otherwise it walks lines the user never scrolled through and lands on "size unknown".
    expect(isWholeFileSelection(selection.selection, null)).toBe(true)
    // And the read must go out as `RangeEnd::Eof`, never a line number no file has.
    expect(toRangeEnds(selection.selection)).toEqual({
      anchor: { kind: 'line', line: 0, offset: 0 },
      focus: { kind: 'eof' },
    })
  })
})
