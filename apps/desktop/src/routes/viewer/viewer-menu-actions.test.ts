import { describe, expect, it, vi } from 'vitest'
import {
  editSurfaceFor,
  runViewerContextMenuAction,
  runViewerEditAction,
  type ViewerEditActionDeps,
} from './viewer-menu-actions'

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
    selectAllDocument: vi.fn(),
    readDomSelection: vi.fn(() => ''),
    copyDomText: vi.fn(),
    reportNothingToCopy: vi.fn(),
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
      runViewerEditAction('selectAll', 'text', deps)
      expect(deps.selectAllContent).toHaveBeenCalledOnce()
      expect(deps.writeClipboardText).not.toHaveBeenCalled()
    })

    it('copies the viewer selection when the search input does not have focus', () => {
      const deps = makeDeps()
      runViewerEditAction('copy', 'text', deps)
      expect(deps.copyContent).toHaveBeenCalledOnce()
      expect(deps.writeClipboardText).not.toHaveBeenCalled()
    })

    // The search bar can be open with focus back on the content, which is exactly the case
    // a native `selectAll:` would send to the status bar instead of the file.
    it('selects the whole file when the search bar is open but unfocused', () => {
      const { input, select } = fakeInput('needle')
      vi.spyOn(document, 'activeElement', 'get').mockReturnValue(document.body)
      const deps = makeDeps({ search: { searchVisible: true, searchInputRef: input } })
      runViewerEditAction('selectAll', 'text', deps)
      expect(deps.selectAllContent).toHaveBeenCalledOnce()
      expect(select).not.toHaveBeenCalled()
    })
  })

  describe('over the search query', () => {
    it('selects the query when the search input has focus', () => {
      const { input, select } = fakeInput('needle')
      const deps = makeDeps({ search: focusedSearch(input) })
      runViewerEditAction('selectAll', 'text', deps)
      expect(select).toHaveBeenCalledOnce()
      expect(deps.selectAllContent).not.toHaveBeenCalled()
    })

    it('copies the selected part of the query', () => {
      const { input } = fakeInput('needle', 2, 5)
      const deps = makeDeps({ search: focusedSearch(input) })
      runViewerEditAction('copy', 'text', deps)
      expect(deps.writeClipboardText).toHaveBeenCalledWith('edl')
      expect(deps.copyContent).not.toHaveBeenCalled()
    })

    // A bare caret in the query means the gesture belongs to the file, the same call
    // `viewer-keyboard.ts` makes for ⌘C. Both paths end in `copyContent`.
    it('falls back to the viewer selection when the query has only a caret', () => {
      const { input } = fakeInput('needle', 3, 3)
      const deps = makeDeps({ search: focusedSearch(input) })
      runViewerEditAction('copy', 'text', deps)
      expect(deps.copyContent).toHaveBeenCalledOnce()
      expect(deps.writeClipboardText).not.toHaveBeenCalled()
    })
  })
})

// The formatted Markdown document and the Binary / Hex views keep the browser's own
// selection, so their Copy reads that one. It used to return quietly when it found nothing,
// which is how a ⌘C over a formatted `.md` could flash the Edit menu and leave the old
// clipboard in place with no word said (ERR-ZNNZH).
describe('runViewerEditAction over a view with a browser selection', () => {
  it('copies the browser selection out of the formatted document', () => {
    const deps = makeDeps({ readDomSelection: vi.fn(() => 'a heading') })
    runViewerEditAction('copy', 'formattedDocument', deps)
    expect(deps.copyDomText).toHaveBeenCalledExactlyOnceWith('a heading')
    expect(deps.copyContent).not.toHaveBeenCalled()
    expect(deps.reportNothingToCopy).not.toHaveBeenCalled()
  })

  it('says so when the formatted document has nothing selected', () => {
    const deps = makeDeps()
    runViewerEditAction('copy', 'formattedDocument', deps)
    expect(deps.reportNothingToCopy).toHaveBeenCalledExactlyOnceWith('formattedDocument')
    expect(deps.copyDomText).not.toHaveBeenCalled()
  })

  it('says so when the Binary or Hex view has nothing selected', () => {
    const deps = makeDeps()
    runViewerEditAction('copy', 'bytes', deps)
    expect(deps.reportNothingToCopy).toHaveBeenCalledExactlyOnceWith('bytes')
  })

  it('selects the formatted document, not the window', () => {
    const deps = makeDeps()
    runViewerEditAction('selectAll', 'formattedDocument', deps)
    expect(deps.selectAllDocument).toHaveBeenCalledOnce()
    expect(deps.selectAllContent).not.toHaveBeenCalled()
  })

  it('still copies a selected search query first', () => {
    const { input } = fakeInput('needle', 0, 3)
    const deps = makeDeps({ search: focusedSearch(input), readDomSelection: vi.fn(() => 'a heading') })
    runViewerEditAction('copy', 'formattedDocument', deps)
    expect(deps.writeClipboardText).toHaveBeenCalledExactlyOnceWith('nee')
    expect(deps.copyDomText).not.toHaveBeenCalled()
  })

  // An image or a PDF: nothing of the page's to copy, and a PDF's own selection lives in its
  // embed, out of reach. Nothing to say that would be true either way.
  it('does nothing over a picture or a PDF', () => {
    const deps = makeDeps({ readDomSelection: vi.fn(() => 'status bar text') })
    runViewerEditAction('copy', 'other', deps)
    expect(deps.copyDomText).not.toHaveBeenCalled()
    expect(deps.copyContent).not.toHaveBeenCalled()
    expect(deps.reportNothingToCopy).not.toHaveBeenCalled()
  })
})

describe('editSurfaceFor', () => {
  it.each([
    ['text', null, 'text'],
    ['binary', null, 'bytes'],
    ['hex', null, 'bytes'],
    ['media', 'markdown', 'formattedDocument'],
    ['media', 'image', 'other'],
    ['media', 'pdf', 'other'],
  ] as const)('reads %s mode over %s as %s', (viewMode, mediaKind, surface) => {
    expect(editSurfaceFor(viewMode, mediaKind)).toBe(surface)
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
