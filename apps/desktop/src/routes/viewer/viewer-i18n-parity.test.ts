/**
 * Base-locale (en) parity net for the viewer i18n migration.
 *
 * Every user-facing viewer string moved from hardcoded English into the
 * `viewer.*` catalog (resolved through `t()` / `getMessage()` / `<Trans>`). This
 * is a behavior-preserving MOVE: every rendered en string must be byte-identical
 * to the pre-migration copy. The goldens below are the exact literals that lived
 * in the viewer components and composables before the move. If a future copy edit
 * is intended, it lands in the catalog AND here together, never silently.
 *
 * The presentational components (`ViewerStatusBar`, `ViewerCopyDialogs`, etc.)
 * each have their own mount tests that pin en-US and assert the rendered text, so
 * this file focuses on the strings those tests don''t already cover: the page''s
 * error states, the search-state composition, the copy/save toasts, the window
 * title, and the ICU-formatted leaves.
 */

import { describe, it, expect, beforeAll, afterAll } from 'vitest'
import { _setLocaleForTests } from '$lib/intl/locale'
import { tString, getMessage } from '$lib/intl/messages.svelte'
import { mediaKindLabel, viewAsMediaLabel, formatMediaDimensions } from './media-view'

beforeAll(() => {
  _setLocaleForTests('en-US')
})
afterAll(() => {
  _setLocaleForTests(null)
})

describe('viewer error + load states (en)', () => {
  it('matches the pre-migration error strings', () => {
    expect(tString('viewer.error.timeout')).toBe("Couldn't load the file. The volume may be slow or unresponsive.")
    expect(tString('viewer.error.noPath')).toBe('No file path specified')
    expect(tString('viewer.error.readFailed')).toBe('Failed to read file')
    expect(tString('viewer.error.retry')).toBe('Retry')
    expect(tString('viewer.error.cancel')).toBe('Cancel')
    expect(tString('viewer.loading')).toBe('Loading...')
  })

  it('builds the window title with the file name', () => {
    expect(tString('viewer.window.titleSuffix', { fileName: 'report.log' })).toBe('report.log | Viewer')
    expect(getMessage('viewer.window.fallbackTitle')).toBe('Viewer')
  })

  it('builds the content aria-label with the file name', () => {
    expect(tString('viewer.content.ariaLabel', { fileName: 'report.log' })).toBe('File content: report.log')
    expect(tString('viewer.srHeading')).toBe('File viewer')
  })
})

describe('viewer search states (en)', () => {
  it('matches the position, with and without the limit-reached marker', () => {
    expect(tString('viewer.search.matchPosition', { current: 3, total: 17, more: 'no' })).toBe('3 of 17')
    expect(tString('viewer.search.matchPosition', { current: 1, total: 1, more: 'yes' })).toBe('1 of 1+')
  })

  it('matches the searching / partial / no-matches leaves', () => {
    expect(tString('viewer.search.searching')).toBe('Searching...')
    expect(tString('viewer.search.partial')).toBe('(partial)')
    expect(tString('viewer.search.noMatches')).toBe('No matches')
  })

  it('matches the search-bar control labels and tooltips', () => {
    expect(tString('viewer.search.placeholder')).toBe('Find in file...')
    expect(tString('viewer.search.ariaLabel')).toBe('Search text')
    expect(tString('viewer.search.caseSensitive')).toBe('Case sensitive')
    expect(tString('viewer.search.regex')).toBe('Regex')
    expect(tString('viewer.search.stop')).toBe('Stop searching')
    expect(tString('viewer.search.stopTooltip')).toBe('Stop scanning and keep results')
    expect(tString('viewer.search.previous')).toBe('Previous match')
    expect(tString('viewer.search.next')).toBe('Next match')
    expect(tString('viewer.search.close')).toBe('Close search')
    expect(tString('viewer.search.closeTooltip')).toBe('Close')
  })
})

describe('viewer toolbar + status bar (en)', () => {
  it('matches the toolbar control strings', () => {
    expect(tString('viewer.toolbar.viewMode.ariaLabel')).toBe('View mode')
    expect(tString('viewer.toolbar.viewMode.text')).toBe('Text')
    expect(tString('viewer.toolbar.viewMode.viewAsText')).toBe('View as text')
    expect(tString('viewer.toolbar.encoding.placeholder')).toBe('Encoding')
    expect(tString('viewer.toolbar.encoding.group.unicode')).toBe('Unicode')
    expect(tString('viewer.toolbar.encoding.group.western')).toBe('Western')
    expect(tString('viewer.toolbar.encoding.detectedSuffix', { label: 'UTF-8' })).toBe('UTF-8 (Detected)')
    expect(tString('viewer.toolbar.tail.label')).toBe('Tail')
    expect(tString('viewer.toolbar.tail.ariaLabel')).toBe('Tail mode: follow file changes')
    expect(tString('viewer.toolbar.tail.tooltip')).toBe('Auto-follow file changes')
    expect(tString('viewer.toolbar.reindexing')).toBe('Reindexing…')
  })

  it('matches the line count, badges, and tooltips', () => {
    expect(tString('viewer.statusBar.ariaLabel')).toBe('File information')
    expect(tString('viewer.statusBar.lineCount', { count: 1 })).toBe('1 line')
    expect(tString('viewer.statusBar.lineCount', { count: 42 })).toBe('42 lines')
    expect(tString('viewer.statusBar.badge.inMemory')).toBe('in memory')
    expect(tString('viewer.statusBar.badge.inMemoryTooltip')).toBe(
      'You have the file entirely in memory. You can quickly scroll to any line.',
    )
    expect(tString('viewer.statusBar.badge.indexed')).toBe('indexed')
    expect(tString('viewer.statusBar.badge.indexedTooltip')).toBe(
      'You have the file indexed, so the line numbers are accurate, and you can quickly scroll to any point.',
    )
    expect(tString('viewer.statusBar.badge.streamingIndexing')).toBe('streaming, indexing...')
    expect(tString('viewer.statusBar.badge.streamingIndexingTooltip', { seconds: 5 })).toBe(
      "This is a large file in streaming mode. We're building an index in background (max 5 sec)... Line numbers are currently approximate.",
    )
    expect(tString('viewer.statusBar.badge.streaming')).toBe('streaming')
    expect(tString('viewer.statusBar.badge.streamingTooltip', { seconds: 5 })).toBe(
      "This is a large file in streaming mode. Indexing would've taken longer than 5 sec, so we didn't do it. The line numbers are estimates.",
    )
    expect(tString('viewer.statusBar.badge.wrap')).toBe('wrap')
    expect(tString('viewer.statusBar.badge.wrapTooltip')).toBe('Lines wrap at the window edge')
  })

  it('matches the status-bar hints (middle-dot separators)', () => {
    expect(tString('viewer.statusBar.hint.image')).toBe('Click 100% / fit · Scroll zoom · Drag pan')
    expect(tString('viewer.statusBar.hint.text')).toBe('W wrap · F tail · ⌘F search')
  })
})

describe('viewer media labels (en)', () => {
  it('matches the kind labels and reverse-switch labels', () => {
    expect(mediaKindLabel('image')).toBe('Image')
    expect(mediaKindLabel('pdf')).toBe('PDF')
    expect(mediaKindLabel('text')).toBe('Text')
    expect(viewAsMediaLabel('image')).toBe('View as image')
    expect(viewAsMediaLabel('pdf')).toBe('View as PDF')
    expect(formatMediaDimensions({ width: 1920, height: 1080 })).toBe('1,920 × 1,080')
  })

  it('matches the inline media status strings', () => {
    expect(tString('viewer.image.loading')).toBe('Loading image')
    expect(tString('viewer.image.error')).toBe(
      "Sorry, we couldn't show this image. The file may be damaged or in a format we can't display.",
    )
    expect(tString('viewer.pdf.loading')).toBe('Loading PDF')
  })
})

describe('viewer context menu + copy dialogs (en)', () => {
  it('matches the context-menu strings', () => {
    expect(tString('viewer.contextMenu.ariaLabel')).toBe('Viewer actions')
    expect(tString('viewer.contextMenu.copy')).toBe('Copy')
    expect(tString('viewer.contextMenu.selectAll')).toBe('Select all')
  })

  it('matches the copy-dialog strings', () => {
    expect(tString('viewer.copyDialog.confirmTitleUnknown')).toBe('Copy this selection to the clipboard?')
    expect(tString('viewer.copyDialog.confirmTitleKnown', { size: '24 MB' })).toBe('Copy 24 MB to the clipboard?')
    expect(tString('viewer.copyDialog.confirmBody')).toBe(
      'Large pastes can slow down other apps. Try search (⌘F) to narrow it down.',
    )
    expect(tString('viewer.copyDialog.cancel')).toBe('Cancel')
    expect(tString('viewer.copyDialog.saveAsFile')).toBe('Save as file…')
    expect(tString('viewer.copyDialog.copy')).toBe('Copy')
    expect(tString('viewer.copyDialog.refuseBody')).toBe(
      "That's larger than the 100 MB clipboard limit. Try search (⌘F) to find what you need, or save the selection as a file.",
    )
  })
})

describe('viewer reload toast + copy/save toasts (en)', () => {
  it('matches the reload-toast strings', () => {
    expect(tString('viewer.reloadToast.grew')).toBe('File changed on disk. Reload?')
    expect(tString('viewer.reloadToast.rotated')).toBe('File replaced on disk. Reload to see the new content.')
    expect(tString('viewer.reloadToast.reload')).toBe('Reload')
    expect(tString('viewer.reloadToast.dismissTooltip')).toBe('Dismiss without reloading')
  })

  it('matches the copy toasts', () => {
    expect(tString('viewer.copy.onClipboard', { size: '24 MB' })).toBe('24 MB on your clipboard')
    expect(tString('viewer.copy.clipboardUnreachable')).toBe("Couldn't reach the clipboard. Try again?")
    expect(tString('viewer.copy.readTooLong')).toBe('The read took too long. Try a smaller selection?')
    expect(tString('viewer.copy.copyFailed')).toBe("Couldn't copy the selection. Try again?")
    expect(tString('viewer.copy.readFailed')).toBe("Couldn't read the selection. Try again?")
  })

  it('matches the save-as strings', () => {
    expect(tString('viewer.saveAs.title')).toBe('Save selection')
    expect(getMessage('viewer.saveAs.defaultName')).toBe('selection')
    expect(tString('viewer.saveAs.panelFailed')).toBe("Couldn't open the save panel. Try again?")
    expect(tString('viewer.saveAs.saved', { name: 'notes.txt' })).toBe('Selection saved to notes.txt')
    expect(tString('viewer.saveAs.tooLong')).toBe('Saving took too long. Try a smaller selection?')
    expect(tString('viewer.saveAs.saveFailed')).toBe("Couldn't save the selection. Try again?")
  })
})

describe('viewer binary-warning labels (en)', () => {
  it('matches the lowercase in-sentence kind words', () => {
    expect(getMessage('viewer.binaryWarning.kind.image')).toBe('image')
    expect(getMessage('viewer.binaryWarning.kind.document')).toBe('document')
    expect(tString('viewer.binaryWarning.dismiss')).toBe('Close')
    expect(tString('viewer.binaryWarning.suppressForever')).toBe('Never show this warning again')
  })
})

describe('viewer selection announcements (en)', () => {
  it('matches the screen-reader announcement strings', () => {
    expect(tString('viewer.selection.toEndOfFile', { line: '12' })).toBe('Selected from line 12 to the end of the file')
    expect(tString('viewer.selection.singleLine', { chars: '5', line: '5' })).toBe('Selected 5 characters on line 5')
    expect(tString('viewer.selection.multiLine', { startLine: '1', endLine: '4', chars: '16' })).toBe(
      'Selected lines 1 to 4, 16 characters',
    )
  })
})
