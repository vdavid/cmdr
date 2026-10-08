import { describe, expect, it, vi } from 'vitest'
import { runViewerContextMenuAction, runViewerEditAction, type ViewerEditActionDeps } from './viewer-menu-actions'

/** A stand-in for the search `<input>`: only the four fields the dispatcher reads plus `select()`. */
function fakeInput(value: string, selectionStart: number | null = null, selectionEnd: number | null = null) {
  const select = vi.fn()
  const input = { value, selectionStart, selectionEnd, select } as unknown as HTMLInputElement
  return { input, select }
}

function makeDeps(overrides: Partial<ViewerEditActionDeps> = {}): ViewerEditActionDeps {
  return {
    search: { searchVisible: false, searchInputRef: null },
    selectAllContent: vi.fn(),
    copyContent: vi.fn(),
    writeClipboardText: vi.fn(),
    ...overrides,
  }
}

/** Focuses `input` for real, so `isSearchInputFocused`'s `document.activeElement` check passes. */
function focusedSearch(input: HTMLInputElement) {
  const activeElement = input as unknown as Element
  vi.spyOn(document, 'activeElement', 'get').mockReturnValue(activeElement)
  return { searchVisible: true, searchInputRef: input }
}

describe('runViewerEditAction', () => {
  describe('over the file', () => {
    it('selects the whole file when the search input does not have focus', () => {
      const deps = makeDeps()
      runViewerEditAction('selectAll', deps)
      expect(deps.selectAllContent).toHaveBeenCalledOnce()
      expect(deps.writeClipboardText).not.toHaveBeenCalled()
    })

    it('copies the viewer selection when the search input does not have focus', () => {
      const deps = makeDeps()
      runViewerEditAction('copy', deps)
      expect(deps.copyContent).toHaveBeenCalledOnce()
      expect(deps.writeClipboardText).not.toHaveBeenCalled()
    })

    // The search bar can be open with focus back on the content, which is exactly the case
    // a native `selectAll:` would send to the status bar instead of the file.
    it('selects the whole file when the search bar is open but unfocused', () => {
      const { input, select } = fakeInput('needle')
      vi.spyOn(document, 'activeElement', 'get').mockReturnValue(document.body)
      const deps = makeDeps({ search: { searchVisible: true, searchInputRef: input } })
      runViewerEditAction('selectAll', deps)
      expect(deps.selectAllContent).toHaveBeenCalledOnce()
      expect(select).not.toHaveBeenCalled()
    })
  })

  describe('over the search query', () => {
    it('selects the query when the search input has focus', () => {
      const { input, select } = fakeInput('needle')
      const deps = makeDeps({ search: focusedSearch(input) })
      runViewerEditAction('selectAll', deps)
      expect(select).toHaveBeenCalledOnce()
      expect(deps.selectAllContent).not.toHaveBeenCalled()
    })

    it('copies the selected part of the query', () => {
      const { input } = fakeInput('needle', 2, 5)
      const deps = makeDeps({ search: focusedSearch(input) })
      runViewerEditAction('copy', deps)
      expect(deps.writeClipboardText).toHaveBeenCalledWith('edl')
      expect(deps.copyContent).not.toHaveBeenCalled()
    })

    // A bare caret in the query means the gesture belongs to the file, the same call
    // `viewer-keyboard.ts` makes for ⌘C. Both paths end in `copyContent`.
    it('falls back to the viewer selection when the query has only a caret', () => {
      const { input } = fakeInput('needle', 3, 3)
      const deps = makeDeps({ search: focusedSearch(input) })
      runViewerEditAction('copy', deps)
      expect(deps.copyContent).toHaveBeenCalledOnce()
      expect(deps.writeClipboardText).not.toHaveBeenCalled()
    })
  })
})

describe('runViewerContextMenuAction', () => {
  // The right-click lands on the text without moving focus, so the search box can still hold
  // it. The menu was opened over the file, so both actions act on the file regardless.
  it('copies the file selection even while the search box holds a selected query', () => {
    const { input } = fakeInput('needle', 0, 6)
    const deps = makeDeps({ search: focusedSearch(input) })

    runViewerContextMenuAction('copy', deps)

    expect(deps.copyContent).toHaveBeenCalledOnce()
    expect(deps.writeClipboardText).not.toHaveBeenCalled()
  })

  it('selects the whole file even while the search box has focus', () => {
    const { input, select } = fakeInput('needle')
    const deps = makeDeps({ search: focusedSearch(input) })

    runViewerContextMenuAction('selectAll', deps)

    expect(deps.selectAllContent).toHaveBeenCalledOnce()
    expect(select).not.toHaveBeenCalled()
  })
})
