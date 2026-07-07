import { describe, it, expect, vi, beforeEach, beforeAll, afterAll } from 'vitest'
import { _setLocaleForTests } from '$lib/intl/locale'
import type { FilePaneAPI } from './types'

// Spies for every dependency the gating helper touches. `pastedAsFileMessage`
// deliberately uses the REAL `$lib/intl` (golden output), so intl is NOT mocked.
const { getSettingSpy, pasteClipboardAsFileSpy, findFileIndexSpy, onDirectoryDiffSpy, addToastSpy, moveCursorSpy } =
  vi.hoisted(() => ({
    getSettingSpy: vi.fn<(id: string) => unknown>(),
    pasteClipboardAsFileSpy: vi.fn<() => Promise<{ name: string; kind: 'text' | 'image' | 'pdf' } | null>>(),
    findFileIndexSpy: vi.fn(),
    onDirectoryDiffSpy: vi.fn(),
    addToastSpy: vi.fn<(pane: unknown, content: unknown, options?: unknown) => string>(),
    moveCursorSpy: vi.fn<() => Promise<void>>(),
  }))

vi.mock('$lib/settings', () => ({ getSetting: getSettingSpy }))
vi.mock('$lib/tauri-commands', () => ({
  pasteClipboardAsFile: pasteClipboardAsFileSpy,
  findFileIndex: findFileIndexSpy,
  onDirectoryDiff: onDirectoryDiffSpy,
}))
vi.mock('$lib/ui/toast', () => ({ addToastForPane: addToastSpy }))
vi.mock('$lib/file-operations/mkdir/new-folder-operations', () => ({ moveCursorToNewFolder: moveCursorSpy }))
// The toast body is passed to addToast (mocked) as an opaque component; never rendered here.
vi.mock('../PasteClipboardToastContent.svelte', () => ({ default: {} }))

import { pasteClipboardContentAsFile } from './paste-clipboard-as-file'
import { pastedAsFileMessage } from './paste-clipboard-as-file-message'

const startRenameSpy = vi.fn()

function buildDeps(overrides: Record<string, unknown> = {}) {
  return {
    volumeId: 'root',
    directory: '/dest/dir',
    listingId: 'lst-1',
    hasParent: false,
    showHiddenFiles: true,
    paneRef: { startRename: startRenameSpy } as unknown as FilePaneAPI,
    originPane: 'left' as const,
    onNothingCreated: vi.fn(),
    ...overrides,
  }
}

beforeEach(() => {
  vi.clearAllMocks()
  moveCursorSpy.mockResolvedValue(undefined)
})

describe('pasteClipboardContentAsFile — setting gating (three values)', () => {
  it('doNothing: replicates today (warn via onNothingCreated), never calls the command', async () => {
    getSettingSpy.mockReturnValue('doNothing')
    const deps = buildDeps()

    await pasteClipboardContentAsFile(deps)

    expect(deps.onNothingCreated).toHaveBeenCalledTimes(1)
    expect(pasteClipboardAsFileSpy).not.toHaveBeenCalled()
    expect(addToastSpy).not.toHaveBeenCalled()
    expect(startRenameSpy).not.toHaveBeenCalled()
    expect(moveCursorSpy).not.toHaveBeenCalled()
  })

  it('createFile + a created file: calls command, lands cursor, info toast, NO rename', async () => {
    getSettingSpy.mockReturnValue('createFile')
    pasteClipboardAsFileSpy.mockResolvedValue({ name: 'pasted.txt', kind: 'text' })
    const deps = buildDeps()

    await pasteClipboardContentAsFile(deps)

    expect(pasteClipboardAsFileSpy).toHaveBeenCalledWith('root', '/dest/dir')
    expect(moveCursorSpy).toHaveBeenCalledWith(
      'lst-1',
      'pasted.txt',
      deps.paneRef,
      false,
      true,
      onDirectoryDiffSpy,
      findFileIndexSpy,
    )
    expect(addToastSpy).toHaveBeenCalledTimes(1)
    const [pane, , opts] = addToastSpy.mock.calls[0]
    expect(pane).toBe('left')
    expect(opts).toMatchObject({ level: 'info', timeoutMs: 7000, props: { filename: 'pasted.txt', kind: 'text' } })
    expect(startRenameSpy).not.toHaveBeenCalled()
    expect(deps.onNothingCreated).not.toHaveBeenCalled()
  })

  it('createFile + nothing pasteable (null): warns via onNothingCreated, no toast, no cursor, no rename', async () => {
    getSettingSpy.mockReturnValue('createFile')
    pasteClipboardAsFileSpy.mockResolvedValue(null)
    const deps = buildDeps()

    await pasteClipboardContentAsFile(deps)

    expect(pasteClipboardAsFileSpy).toHaveBeenCalledTimes(1)
    expect(deps.onNothingCreated).toHaveBeenCalledTimes(1)
    expect(addToastSpy).not.toHaveBeenCalled()
    expect(moveCursorSpy).not.toHaveBeenCalled()
    expect(startRenameSpy).not.toHaveBeenCalled()
  })

  it('createFileAndRename + a created file: also starts a rename with the extension warning suppressed', async () => {
    getSettingSpy.mockReturnValue('createFileAndRename')
    pasteClipboardAsFileSpy.mockResolvedValue({ name: 'pasted.png', kind: 'image' })
    const deps = buildDeps()

    await pasteClipboardContentAsFile(deps)

    expect(addToastSpy).toHaveBeenCalledTimes(1)
    // Thread the created name so the pane's rename guard only activates once the
    // cursor is actually on the new file (defends the row-index race where the
    // synthetic diff hasn't landed and a neighbor is under the cursor).
    expect(startRenameSpy).toHaveBeenCalledWith({ suppressExtensionWarning: true, expectedName: 'pasted.png' })
  })

  it('createFileAndRename + null: no rename, warns via onNothingCreated', async () => {
    getSettingSpy.mockReturnValue('createFileAndRename')
    pasteClipboardAsFileSpy.mockResolvedValue(null)
    const deps = buildDeps()

    await pasteClipboardContentAsFile(deps)

    expect(startRenameSpy).not.toHaveBeenCalled()
    expect(deps.onNothingCreated).toHaveBeenCalledTimes(1)
  })

  it('orders the work: cursor lands BEFORE the toast fires', async () => {
    getSettingSpy.mockReturnValue('createFile')
    pasteClipboardAsFileSpy.mockResolvedValue({ name: 'pasted.pdf', kind: 'pdf' })
    const order: string[] = []
    moveCursorSpy.mockImplementation(() => {
      order.push('cursor')
      return Promise.resolve()
    })
    addToastSpy.mockImplementation(() => {
      order.push('toast')
      return 'toast-id'
    })

    await pasteClipboardContentAsFile(buildDeps())

    expect(order).toEqual(['cursor', 'toast'])
  })
})

describe('pastedAsFileMessage — golden output (en-US)', () => {
  beforeAll(() => {
    _setLocaleForTests('en-US')
  })
  afterAll(() => {
    _setLocaleForTests(null)
  })

  it('text', () => {
    expect(pastedAsFileMessage('text', 'pasted.txt')).toBe('Pasted clipboard text as pasted.txt')
  })

  it('image', () => {
    expect(pastedAsFileMessage('image', 'pasted.png')).toBe('Pasted clipboard image as pasted.png')
  })

  it('PDF (uppercased noun)', () => {
    expect(pastedAsFileMessage('pdf', 'pasted.pdf')).toBe('Pasted clipboard PDF as pasted.pdf')
  })

  it('carries the deduped filename verbatim', () => {
    expect(pastedAsFileMessage('image', 'pasted (2).png')).toBe('Pasted clipboard image as pasted (2).png')
  })
})
