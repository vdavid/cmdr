/**
 * Pure-helper tests for the viewer's keyboard plumbing. Each helper returns
 * `true` when it consumed the event, `false` when the caller should fall
 * through to another handler.
 */

import { afterEach, describe, expect, it, vi } from 'vitest'

import {
  createViewerSelection,
  EOF_LINE,
  isWholeFileSelection,
  toRangeEnds,
  type LineOffset,
  type Selection,
} from './selection.svelte'
import { createViewerKeyboard, handleSearchToggleKey, handleTailToggleKey, handleToggleKey } from './viewer-keyboard'
import { createViewerScroll } from './viewer-scroll.svelte'

type KeyboardDeps = Parameters<typeof createViewerKeyboard>[0]

function makeKey(props: Partial<KeyboardEventInit & { key: string }>): KeyboardEvent {
  return new KeyboardEvent('keydown', { key: 'a', ...props })
}

/** Sends one cancelable keydown through the router and hands the event back. */
function press(
  keyboard: ReturnType<typeof createViewerKeyboard>,
  init: KeyboardEventInit & { key: string },
): KeyboardEvent {
  const e = makeKey({ cancelable: true, ...init })
  keyboard.handleKeyDown(e)
  return e
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
    getLastRenderedLine: () => 9,
    selection: { selection: null, selectAll: noop, selectToEof: noop, setFocus: noop },
    scroll: {
      scrollByLines: noop,
      scrollByPages: noop,
      scrollToStart: noop,
      scrollToEnd: noop,
      scrollByColumns: noop,
      ensureLineVisible: noop,
      ensureColumnVisible: noop,
    },
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
        selection,
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

/** A mutable selection cell plus the `KeyboardDeps.selection` view over it. */
function makeSelectionDeps(initial: Selection | null) {
  let current = initial
  const setFocus = vi.fn((point: LineOffset) => {
    current = { anchor: current === null ? point : current.anchor, focus: point }
  })
  return {
    setFocus,
    get focus(): LineOffset | null {
      return current === null ? null : current.focus
    },
    deps: {
      get selection() {
        return current
      },
      selectAll: vi.fn(),
      selectToEof: vi.fn(),
      setFocus,
    },
  }
}

/** Every scroll action the keyboard can reach, each a spy. */
function makeScrollSpies() {
  return {
    scrollByLines: vi.fn(),
    scrollByPages: vi.fn(),
    scrollToStart: vi.fn(),
    scrollToEnd: vi.fn(),
    scrollByColumns: vi.fn(),
    ensureLineVisible: vi.fn(),
    ensureColumnVisible: vi.fn(),
  }
}

describe('createViewerKeyboard: keyboard selection extension', () => {
  // Offsets: `alpha` 0-5, `beta` 6-10, `gamma` 11-16.
  const line = 'alpha beta gamma'

  /** A 10-line file of `line`s, the focus parked mid-file at the space after `alpha`. */
  function wire(overrides: Partial<KeyboardDeps> = {}) {
    const selection = makeSelectionDeps({ anchor: { line: 2, offset: 0 }, focus: { line: 2, offset: 5 } })
    const scroll = makeScrollSpies()
    const keyboard = createViewerKeyboard(
      makeKeyboardDeps({ getLineText: () => line, selection: selection.deps, scroll, ...overrides }),
    )
    return { keyboard, selection, scroll }
  }

  const chords: Array<[string, KeyboardEventInit & { key: string }, LineOffset]> = [
    ['Shift+Right steps one character', { key: 'ArrowRight', shiftKey: true }, { line: 2, offset: 6 }],
    ['Shift+Left steps one character back', { key: 'ArrowLeft', shiftKey: true }, { line: 2, offset: 4 }],
    ['Shift+Down steps one logical line', { key: 'ArrowDown', shiftKey: true }, { line: 3, offset: 5 }],
    ['Shift+Up steps one logical line back', { key: 'ArrowUp', shiftKey: true }, { line: 1, offset: 5 }],
    [
      '⌥⇧Right lands on the end of the next word',
      { key: 'ArrowRight', shiftKey: true, altKey: true },
      { line: 2, offset: 10 },
    ],
    [
      '⌥⇧Left lands on the start of the previous word',
      { key: 'ArrowLeft', shiftKey: true, altKey: true },
      { line: 2, offset: 0 },
    ],
    [
      '⌃⇧Right is the same word motion, for Linux and Windows',
      { key: 'ArrowRight', shiftKey: true, ctrlKey: true },
      { line: 2, offset: 10 },
    ],
    [
      '⌃⇧Left is the same word motion, for Linux and Windows',
      { key: 'ArrowLeft', shiftKey: true, ctrlKey: true },
      { line: 2, offset: 0 },
    ],
    ['Shift+Home extends to the line start', { key: 'Home', shiftKey: true }, { line: 2, offset: 0 }],
    ['Shift+End extends to the line end', { key: 'End', shiftKey: true }, { line: 2, offset: 16 }],
    [
      '⌘⇧Up extends to the start of the file',
      { key: 'ArrowUp', shiftKey: true, metaKey: true },
      { line: 0, offset: 0 },
    ],
    [
      '⌘⇧Down extends to the end of the file',
      { key: 'ArrowDown', shiftKey: true, metaKey: true },
      { line: 9, offset: 16 },
    ],
  ]

  it.each(chords)('%s', (_name, init, expected) => {
    const { keyboard, selection } = wire()
    const e = press(keyboard, init)
    expect(selection.focus).toEqual(expected)
    expect(e.defaultPrevented).toBe(true)
  })

  it('keeps the anchor where it was: extension moves the focus only', () => {
    const { keyboard, selection } = wire()
    press(keyboard, { key: 'ArrowDown', shiftKey: true, metaKey: true })
    expect(selection.deps.selection?.anchor).toEqual({ line: 2, offset: 0 })
    expect(selection.deps.selectToEof).not.toHaveBeenCalled()
  })

  it('scrolls the new focus line into view, on both axes', () => {
    const { keyboard, scroll } = wire()
    press(keyboard, { key: 'ArrowDown', shiftKey: true })
    expect(scroll.ensureLineVisible).toHaveBeenCalledWith(3)
    expect(scroll.ensureColumnVisible).toHaveBeenCalledWith({ line: 3, offset: 5 })
  })

  it('Shift+Up / Down extend instead of scrolling, which is what they used to do', () => {
    const { keyboard, scroll } = wire()
    press(keyboard, { key: 'ArrowDown', shiftKey: true })
    press(keyboard, { key: 'ArrowUp', shiftKey: true })
    expect(scroll.scrollByLines).not.toHaveBeenCalled()
  })

  it('keeps the desired column while walking down through a short line and back out', () => {
    const selection = makeSelectionDeps({ anchor: { line: 2, offset: 0 }, focus: { line: 2, offset: 16 } })
    const keyboard = createViewerKeyboard(
      makeKeyboardDeps({
        // Line 3 is two characters long; every other line is the full 16.
        getLineText: (n: number) => (n === 3 ? 'ab' : line),
        selection: selection.deps,
        scroll: makeScrollSpies(),
      }),
    )

    press(keyboard, { key: 'ArrowDown', shiftKey: true })
    expect(selection.focus).toEqual({ line: 3, offset: 2 })
    press(keyboard, { key: 'ArrowDown', shiftKey: true })
    expect(selection.focus).toEqual({ line: 4, offset: 16 })

    // A horizontal motion ends the run, so the next vertical step starts a new column.
    press(keyboard, { key: 'ArrowLeft', shiftKey: true })
    press(keyboard, { key: 'ArrowDown', shiftKey: true })
    press(keyboard, { key: 'ArrowDown', shiftKey: true })
    expect(selection.focus).toEqual({ line: 6, offset: 15 })
  })

  it('drops the desired column on a fresh gesture, so a stale run cannot aim the next one', () => {
    const selection = makeSelectionDeps({ anchor: { line: 2, offset: 0 }, focus: { line: 2, offset: 16 } })
    const keyboard = createViewerKeyboard(
      makeKeyboardDeps({
        getLineText: (n: number) => (n === 3 ? 'ab' : line),
        selection: selection.deps,
        scroll: makeScrollSpies(),
      }),
    )

    // Park a desired column of 16 by stepping down through the short line 3.
    press(keyboard, { key: 'ArrowDown', shiftKey: true })
    expect(selection.focus).toEqual({ line: 3, offset: 2 })

    // ⌘A restarts the gesture, and the run's aim goes with it. The stubbed `selectAll`
    // leaves the focus at (3, 2), so the next step reads the reset: column 2, not 16.
    keyboard.handleSelectAllShortcut()
    press(keyboard, { key: 'ArrowUp', shiftKey: true })
    expect(selection.focus).toEqual({ line: 2, offset: 2 })
  })

  it('consumes the key but leaves the selection alone when the target line is not cached', () => {
    const { keyboard, selection, scroll } = wire({ getLineText: (n: number) => (n === 2 ? line : undefined) })
    const e = press(keyboard, { key: 'ArrowDown', shiftKey: true })
    expect(selection.setFocus).not.toHaveBeenCalled()
    // The scroll is what fetches the line, so the next press can land.
    expect(scroll.ensureLineVisible).toHaveBeenCalledWith(3)
    expect(e.defaultPrevented).toBe(true)
  })

  it('⌘⇧Down with no line count yet selects to the end-of-file sentinel and scrolls there', () => {
    const { keyboard, selection, scroll } = wire({ getTotalLines: () => null })
    press(keyboard, { key: 'ArrowDown', shiftKey: true, metaKey: true })
    expect(selection.focus).toEqual({ line: EOF_LINE, offset: 0 })
    // The sentinel goes straight to `ensureLineVisible`, which owns the one branch that
    // reads it as "the end of the file". Routing it here instead would put that rule in
    // two places, and the composable would still be wrong for any other caller.
    expect(scroll.ensureLineVisible).toHaveBeenCalledWith(EOF_LINE)
    // It names no measurable character either, so nothing tries to scroll to its column.
    expect(scroll.ensureColumnVisible).not.toHaveBeenCalled()
  })

  it('scrolls to the line the focus is ALREADY on when that line is not cached', () => {
    // The uncached line isn't always a neighbour: a `char` step off a line whose own text
    // was evicted comes back as `{ focus: null, targetLine: from.line }`.
    const { keyboard, selection, scroll } = wire({ getLineText: () => undefined })
    const e = press(keyboard, { key: 'ArrowRight', shiftKey: true })
    expect(selection.setFocus).not.toHaveBeenCalled()
    expect(scroll.ensureLineVisible).toHaveBeenCalledWith(2)
    expect(e.defaultPrevented).toBe(true)
  })

  it('resolves a sentinel focus to the last rendered line before moving from it', () => {
    // ⌘A in ByteSeek-no-index mode parks the focus on the sentinel, and `moveFocus`
    // throws on one. Shift+Up from there steps off line 7, the last line on screen.
    const selection = makeSelectionDeps({ anchor: { line: 0, offset: 0 }, focus: { line: EOF_LINE, offset: 0 } })
    const keyboard = createViewerKeyboard(
      makeKeyboardDeps({
        getTotalLines: () => null,
        getLastRenderedLine: () => 7,
        getLineText: () => line,
        selection: selection.deps,
        scroll: makeScrollSpies(),
      }),
    )
    press(keyboard, { key: 'ArrowUp', shiftKey: true })
    expect(selection.focus).toEqual({ line: 6, offset: 16 })
  })

  it('is a no-op when the focus is on the sentinel and nothing is rendered', () => {
    const selection = makeSelectionDeps({ anchor: { line: 0, offset: 0 }, focus: { line: EOF_LINE, offset: 0 } })
    const scroll = makeScrollSpies()
    const keyboard = createViewerKeyboard(
      makeKeyboardDeps({ getLastRenderedLine: () => null, selection: selection.deps, scroll }),
    )
    press(keyboard, { key: 'ArrowUp', shiftKey: true })
    expect(selection.setFocus).not.toHaveBeenCalled()
    expect(scroll.ensureLineVisible).not.toHaveBeenCalled()
  })

  it('does nothing with no selection at all: a click is the discoverable way to start one', () => {
    const selection = makeSelectionDeps(null)
    const scroll = makeScrollSpies()
    const keyboard = createViewerKeyboard(makeKeyboardDeps({ selection: selection.deps, scroll }))
    const e = press(keyboard, { key: 'ArrowRight', shiftKey: true })
    expect(selection.setFocus).not.toHaveBeenCalled()
    // Still consumed, so the view doesn't scroll out from under the user instead.
    expect(e.defaultPrevented).toBe(true)
    expect(scroll.scrollByColumns).not.toHaveBeenCalled()
  })

  it('leaves the search input its own Shift+Arrow and ⌥⇧Arrow', () => {
    const input = document.createElement('input')
    document.body.append(input)
    input.focus()
    const selection = makeSelectionDeps({ anchor: { line: 2, offset: 0 }, focus: { line: 2, offset: 5 } })
    const base = makeKeyboardDeps({ getLineText: () => line, selection: selection.deps })
    const keyboard = createViewerKeyboard({
      ...base,
      search: { ...base.search, searchVisible: true, searchInputRef: input },
    })

    press(keyboard, { key: 'ArrowRight', shiftKey: true })
    press(keyboard, { key: 'ArrowRight', shiftKey: true, altKey: true })

    expect(selection.setFocus).not.toHaveBeenCalled()
    document.body.innerHTML = ''
  })

  it('leaves Shift+Enter and ⌘⌥R to the search bar', () => {
    const findPrev = vi.fn()
    const toggleUseRegex = vi.fn()
    const selection = makeSelectionDeps({ anchor: { line: 2, offset: 0 }, focus: { line: 2, offset: 5 } })
    const base = makeKeyboardDeps({ selection: selection.deps })
    const keyboard = createViewerKeyboard({
      ...base,
      search: { ...base.search, searchVisible: true, findPrev, toggleUseRegex },
    })

    press(keyboard, { key: 'Enter', shiftKey: true })
    expect(findPrev).toHaveBeenCalledOnce()

    press(keyboard, { key: 'r', metaKey: true, altKey: true })
    expect(toggleUseRegex).toHaveBeenCalledOnce()
  })
})

describe('createViewerKeyboard over a real scroll composable: the phantom trailing-newline line', () => {
  /**
   * A file ending in `\n` makes the `lineIndex` backend count one more line than it will
   * ever emit: `totalLines` is N+1 while `viewer_get_lines` stops at N-1. The template
   * draws a row for the phantom line anyway, so these press through the same seam the
   * page wires — the keyboard's `getLineText` reading the composable's `renderedLineText`
   * — with a real composable behind it.
   */
  function wireOverScroll(totalLines: number, cachedThrough: number, focus: LineOffset) {
    const scroll = createViewerScroll({
      getSessionId: () => 'sess-1',
      getTotalLines: () => totalLines,
      setTotalLines: () => {},
      getEstimatedLines: () => totalLines,
      getBackendType: () => 'lineIndex',
      onTimeoutError: () => {},
      getAllLines: () => null,
      getTextWidth: () => 0,
    })
    function cacheLines(from: number, to: number) {
      for (let i = from; i < to; i++) scroll.lineCache.set(i, 'alpha')
    }
    cacheLines(0, cachedThrough)

    const selection = makeSelectionDeps({ anchor: { line: 0, offset: 0 }, focus })
    const spies = makeScrollSpies()
    const keyboard = createViewerKeyboard(
      makeKeyboardDeps({
        getTotalLines: () => totalLines,
        getLineText: (n: number) => scroll.renderedLineText(n),
        getLastRenderedLine: () => null,
        selection: selection.deps,
        scroll: spies,
      }),
    )
    return { keyboard, selection, spies, scroll, cacheLines }
  }

  /** Puts the tail of a `totalLines`-line file on screen, as the keyboard's own scroll would. */
  function scrollToBottom(scroll: ReturnType<typeof createViewerScroll>, totalLines: number) {
    const el = document.createElement('div')
    Object.defineProperty(el, 'clientHeight', { value: 600 })
    scroll.contentRef = el
    el.scrollTop = totalLines * scroll.scrollLineHeight - 600
    scroll.handleScroll()
  }

  it('⌘⇧Down lands on the end of the last REAL line in one press while the phantom row is on screen', () => {
    // 11 lines by the backend's count, 10 real ones (0-9); line 10 is the phantom.
    const { keyboard, selection } = wireOverScroll(11, 10, { line: 2, offset: 2 })

    press(keyboard, { key: 'ArrowDown', shiftKey: true, metaKey: true })

    // Offset 0 of the phantom line IS the end of line 9: the range is half-open, so it
    // takes line 9 in full and nothing of line 10. Same landing the `fullLoad` backend
    // gives, which emits the phantom line as `''` instead of skipping it.
    expect(selection.focus).toEqual({ line: 10, offset: 0 })
  })

  it('Shift+Down onto the phantom row extends instead of going dead', () => {
    const { keyboard, selection } = wireOverScroll(11, 10, { line: 9, offset: 3 })

    press(keyboard, { key: 'ArrowDown', shiftKey: true })

    expect(selection.focus).toEqual({ line: 10, offset: 0 })
  })

  it('leaves a target OUTSIDE the rendered range unlanded, so the scroll fetches it first', () => {
    const { keyboard, selection, spies } = wireOverScroll(40_001, 100, { line: 2, offset: 2 })

    const e = press(keyboard, { key: 'ArrowDown', shiftKey: true, metaKey: true })

    expect(selection.setFocus).not.toHaveBeenCalled()
    expect(spies.ensureLineVisible).toHaveBeenCalledWith(40_000)
    expect(e.defaultPrevented).toBe(true)
  })

  it('lands on the second press, once that scroll has brought the phantom row into the window', () => {
    const { keyboard, selection, scroll, cacheLines } = wireOverScroll(40_001, 100, { line: 2, offset: 2 })

    press(keyboard, { key: 'ArrowDown', shiftKey: true, metaKey: true })
    expect(selection.setFocus).not.toHaveBeenCalled()

    // What `ensureLineVisible(40 000)` and the fetch it triggers leave behind: the tail of
    // the file on screen with every REAL line cached, line 40 000 still absent.
    scrollToBottom(scroll, 40_001)
    cacheLines(39_900, 40_000)

    press(keyboard, { key: 'ArrowDown', shiftKey: true, metaKey: true })
    expect(selection.focus).toEqual({ line: 40_000, offset: 0 })
  })

  it('a file with NO trailing newline still lands on its real last line in the documented two presses', () => {
    const { keyboard, selection, scroll, cacheLines } = wireOverScroll(40_000, 100, { line: 2, offset: 2 })

    press(keyboard, { key: 'ArrowDown', shiftKey: true, metaKey: true })
    expect(selection.setFocus).not.toHaveBeenCalled()

    scrollToBottom(scroll, 40_000)
    cacheLines(39_900, 40_000)

    press(keyboard, { key: 'ArrowDown', shiftKey: true, metaKey: true })
    expect(selection.focus).toEqual({ line: 39_999, offset: 'alpha'.length })
  })
})

describe('createViewerKeyboard: unmodified arrows scroll', () => {
  it('Left / Right scroll horizontally by one column', () => {
    const scroll = makeScrollSpies()
    const keyboard = createViewerKeyboard(makeKeyboardDeps({ scroll }))

    keyboard.handleKeyDown(makeKey({ key: 'ArrowLeft', cancelable: true }))
    keyboard.handleKeyDown(makeKey({ key: 'ArrowRight', cancelable: true }))

    expect(scroll.scrollByColumns.mock.calls).toEqual([[-1], [1]])
  })

  it('Up / Down still scroll by a line and Home / End still jump to the file edges', () => {
    const scroll = makeScrollSpies()
    const keyboard = createViewerKeyboard(makeKeyboardDeps({ scroll }))

    keyboard.handleKeyDown(makeKey({ key: 'ArrowDown', cancelable: true }))
    keyboard.handleKeyDown(makeKey({ key: 'Home', cancelable: true }))
    keyboard.handleKeyDown(makeKey({ key: 'End', cancelable: true }))

    expect(scroll.scrollByLines).toHaveBeenCalledWith(1)
    expect(scroll.scrollToStart).toHaveBeenCalledOnce()
    expect(scroll.scrollToEnd).toHaveBeenCalledOnce()
  })
})
