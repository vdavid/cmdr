/**
 * `createViewerCopyOrchestrator`: what the user is TOLD landed on their clipboard.
 *
 * The size in that toast is a claim about their data, so it has to be the size of the
 * text actually written, never the estimate that only picked the band. The estimate
 * prorates partial lines by a UTF-16 fraction and takes shortcuts on a whole-file
 * selection, so the two genuinely differ.
 */

import { describe, expect, it, vi, beforeEach, beforeAll, afterAll } from 'vitest'

import { createViewerCopyOrchestrator } from './viewer-copy.svelte'
import { formatByteSize } from '#lib/units/index.js'
import { tString } from '#lib/intl/messages.svelte.js'
import { _setLocaleForTests } from '#lib/intl/locale.js'
import type { CopyOutcome, SaveResult } from './viewer-copy.svelte'

const { addToast, showSavePanel } = vi.hoisted(() => ({
  addToast: vi.fn<(message: string) => string>(),
  showSavePanel: vi.fn<() => Promise<string | null>>(),
}))
vi.mock('#lib/ui/toast/toast-store.svelte.js', () => ({ addToast }))
vi.mock('@tauri-apps/plugin-dialog', () => ({ save: showSavePanel }))

const writeText = vi.fn<(text: string) => Promise<void>>()

beforeAll(() => {
  _setLocaleForTests('en-US')
  Object.defineProperty(navigator, 'clipboard', { value: { writeText }, configurable: true })
})
afterAll(() => {
  _setLocaleForTests(null)
})
beforeEach(() => {
  addToast.mockClear()
  writeText.mockReset()
  writeText.mockResolvedValue(undefined)
})

/**
 * An orchestrator over a copy composable whose `runCopy` answers with `outcome`, and
 * whose `saveAs` with `saved`.
 */
function wire(outcome: CopyOutcome, saved: SaveResult = { ok: true, savedAs: { kind: 'utf8' } }) {
  const copy = {
    busy: false,
    inFlightReadId: null,
    runCopy: () => Promise.resolve(outcome),
    cancelInFlight: () => Promise.resolve(),
    saveAs: () => Promise.resolve(saved),
  }
  return createViewerCopyOrchestrator({ copy, getFileName: () => 'notes.txt' })
}

describe('the "on clipboard" toast', () => {
  it('reports the size of the text that was copied, not the estimate that sized the band', async () => {
    // "alpha\nbeta\ngam" is 14 bytes. The estimate said 16, the whole 16-byte file: the
    // whole-file shortcut hands back the file size whenever a selection starts at (0, 0)
    // and reaches the last line, even when it stops partway into that line.
    const text = 'alpha\nbeta\ngam'
    const orchestrator = wire({ kind: 'silent', text, bytes: 16 })

    await orchestrator.handleCopy()

    expect(writeText).toHaveBeenCalledWith(text)
    expect(addToast).toHaveBeenCalledWith(tString('viewer.copy.onClipboard', { size: formatByteSize(14) }), {
      level: 'info',
    })
  })

  it('says nothing about a size when the clipboard itself refused the write', async () => {
    writeText.mockRejectedValue(new Error('denied'))
    const orchestrator = wire({ kind: 'silent', text: 'alpha', bytes: 5 })

    await orchestrator.handleCopy()

    expect(addToast).toHaveBeenCalledWith(tString('viewer.copy.clipboardUnreachable'), { level: 'warn' })
  })
})

describe('the "saved" toast', () => {
  beforeEach(() => {
    showSavePanel.mockResolvedValue('/Users/someone/notes.selection.txt')
  })

  it('names the file for a save that wrote UTF-8 text', async () => {
    const orchestrator = wire({ kind: 'empty' })

    await orchestrator.handleSaveAs()

    expect(addToast).toHaveBeenCalledWith(tString('viewer.saveAs.saved', { name: 'notes.selection.txt' }), {
      level: 'info',
    })
  })

  it('says the file kept its original encoding when the selection held characters the decode lost', async () => {
    // A UTF-8 copy would have swapped those characters for U+FFFD, so the backend wrote
    // the source's own bytes; the user should hear why the file isn't UTF-8.
    const orchestrator = wire({ kind: 'empty' }, { ok: true, savedAs: { kind: 'sourceEncoding', encoding: 'utf16Le' } })

    await orchestrator.handleSaveAs()

    expect(addToast).toHaveBeenCalledWith(
      tString('viewer.saveAs.savedInSourceEncoding', { name: 'notes.selection.txt' }),
      { level: 'info' },
    )
  })
})
