/**
 * Tests for `folder-size-count.ts` (⌥⇧⏎, Space on a folder). They pin:
 * - a count goes to the backend with the pane's listing, hidden setting, and paths,
 * - Esc stops only a count that is running, and a finished count is no longer running,
 * - overlapping counts of one pane keep it "running" until the last one ends,
 * - a gone volume says so; a pane with no listing does nothing,
 * - when Space should count a folder.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest'

const { ipc, addToast } = vi.hoisted(() => ({
  ipc: { countFolderSizes: vi.fn(), cancelFolderSizeCount: vi.fn(), getFileAt: vi.fn() },
  addToast: vi.fn(),
}))

vi.mock('$lib/tauri-commands', () => ({
  countFolderSizes: ipc.countFolderSizes,
  cancelFolderSizeCount: ipc.cancelFolderSizeCount,
  getFileAt: ipc.getFileAt,
}))
vi.mock('$lib/ui/toast', () => ({ addToast }))
vi.mock('$lib/intl/messages.svelte', () => ({ tString: (key: string) => key }))
vi.mock('$lib/logging/logger', () => ({
  getAppLogger: () => ({ warn: vi.fn(), info: vi.fn(), error: vi.fn(), debug: vi.fn() }),
}))

import { cancelCountInPane, countFolderOnSpace, countFoldersInPane, shouldCountOnSpace } from './folder-size-count'
import type { FileEntry } from '../types'

function deferred<T>() {
  let resolve: (value: T) => void = () => {}
  const promise = new Promise<T>((r) => {
    resolve = r
  })
  return { promise, resolve }
}

describe('countFoldersInPane', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    ipc.cancelFolderSizeCount.mockResolvedValue(true)
  })

  it('asks the backend for the pane’s folders and returns the outcome', async () => {
    ipc.countFolderSizes.mockResolvedValue({ counted: 3, unreadable: 0, cancelled: false })

    const outcome = await countFoldersInPane('L', true)

    expect(ipc.countFolderSizes).toHaveBeenCalledWith('L', true, null)
    expect(outcome).toEqual({ counted: 3, unreadable: 0, cancelled: false })
  })

  it('passes the folder Space selected', async () => {
    ipc.countFolderSizes.mockResolvedValue({ counted: 1, unreadable: 0, cancelled: false })

    await countFoldersInPane('L', false, ['/data/photos'])

    expect(ipc.countFolderSizes).toHaveBeenCalledWith('L', false, ['/data/photos'])
  })

  it('lets Esc stop a running count, and only a running one', async () => {
    const pending = deferred<{ counted: number; unreadable: number; cancelled: boolean }>()
    ipc.countFolderSizes.mockReturnValue(pending.promise)

    const counting = countFoldersInPane('L', false)
    expect(cancelCountInPane('R')).toBe(false)
    expect(cancelCountInPane('L')).toBe(true)
    expect(ipc.cancelFolderSizeCount).toHaveBeenCalledWith('L')

    expect(cancelCountInPane('L')).toBe(true) // still running until the backend answers

    pending.resolve({ counted: 0, unreadable: 0, cancelled: true })
    await counting
    expect(cancelCountInPane('L')).toBe(false)
  })

  it('keeps a newer count stoppable after Esc stopped the older one', async () => {
    const first = deferred<{ counted: number; unreadable: number; cancelled: boolean }>()
    const second = deferred<{ counted: number; unreadable: number; cancelled: boolean }>()
    ipc.countFolderSizes.mockReturnValueOnce(first.promise).mockReturnValueOnce(second.promise)

    const a = countFoldersInPane('L', false)
    cancelCountInPane('L')
    const b = countFoldersInPane('L', false)
    first.resolve({ counted: 0, unreadable: 0, cancelled: true })
    await a

    expect(cancelCountInPane('L')).toBe(true)
    second.resolve({ counted: 0, unreadable: 0, cancelled: true })
    await b
  })

  it('keeps a pane running until its last overlapping count ends', async () => {
    const first = deferred<{ counted: number; unreadable: number; cancelled: boolean }>()
    const second = deferred<{ counted: number; unreadable: number; cancelled: boolean }>()
    ipc.countFolderSizes.mockReturnValueOnce(first.promise).mockReturnValueOnce(second.promise)

    const a = countFoldersInPane('L', false)
    const b = countFoldersInPane('L', false)
    first.resolve({ counted: 0, unreadable: 0, cancelled: true })
    await a

    expect(cancelCountInPane('L')).toBe(true)
    second.resolve({ counted: 0, unreadable: 0, cancelled: true })
    await b
  })

  it('says so when the folder’s volume is gone', async () => {
    ipc.countFolderSizes.mockRejectedValue({ type: 'notConnected' })

    expect(await countFoldersInPane('L', false)).toBeNull()
    expect(addToast).toHaveBeenCalledWith('fileExplorer.folderSizes.notConnected', { level: 'warn' })
  })

  it('does nothing for a pane with no listing', async () => {
    expect(await countFoldersInPane('', false)).toBeNull()
    expect(ipc.countFolderSizes).not.toHaveBeenCalled()
  })
})

describe('shouldCountOnSpace', () => {
  const dir = (over: Partial<FileEntry> = {}) =>
    ({ name: 'photos', path: '/data/photos', isDirectory: true, isSymlink: false, ...over }) as FileEntry

  it('counts a folder Space just selected whose size isn’t known', () => {
    expect(shouldCountOnSpace(dir(), true, true)).toBe(true)
    expect(shouldCountOnSpace(dir({ recursiveSize: 5, recursiveSizeComplete: false }), true, true)).toBe(true)
  })

  it('leaves it alone when deselecting, switched off, or already known', () => {
    expect(shouldCountOnSpace(dir(), false, true)).toBe(false)
    expect(shouldCountOnSpace(dir(), true, false)).toBe(false)
    expect(shouldCountOnSpace(dir({ recursiveSize: 5, recursiveSizeComplete: true }), true, true)).toBe(false)
  })

  it('never counts a file, a link, or the parent row', () => {
    expect(shouldCountOnSpace(dir({ isDirectory: false }), true, true)).toBe(false)
    expect(shouldCountOnSpace(dir({ isSymlink: true }), true, true)).toBe(false)
    expect(shouldCountOnSpace(dir({ name: '..' }), true, true)).toBe(false)
    expect(shouldCountOnSpace(undefined, true, true)).toBe(false)
  })
})

describe('countFolderOnSpace', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    ipc.countFolderSizes.mockResolvedValue({ counted: 1, unreadable: 0, cancelled: false })
  })

  it('counts the folder at the row Space selected, read from the backend', async () => {
    ipc.getFileAt.mockResolvedValue({ name: 'photos', path: '/data/photos', isDirectory: true, isSymlink: false })

    await countFolderOnSpace({ listingId: 'L', backendRow: 4, includeHidden: true, selected: true, enabled: true })

    expect(ipc.getFileAt).toHaveBeenCalledWith('L', 4, true)
    expect(ipc.countFolderSizes).toHaveBeenCalledWith('L', true, ['/data/photos'])
  })

  it('does nothing for a deselect, a file, or the parent row', async () => {
    ipc.getFileAt.mockResolvedValue({ name: 'a.txt', path: '/data/a.txt', isDirectory: false, isSymlink: false })

    await countFolderOnSpace({ listingId: 'L', backendRow: 1, includeHidden: false, selected: false, enabled: true })
    await countFolderOnSpace({ listingId: 'L', backendRow: -1, includeHidden: false, selected: true, enabled: true })
    await countFolderOnSpace({ listingId: 'L', backendRow: 1, includeHidden: false, selected: true, enabled: true })

    expect(ipc.countFolderSizes).not.toHaveBeenCalled()
  })

  it('says how many folders it couldn’t read, once per count', async () => {
    ipc.countFolderSizes.mockResolvedValue({ counted: 2, unreadable: 3, cancelled: false })

    await countFoldersInPane('L', false)
    await countFoldersInPane('L', false, ['/a'])

    const unreadable = addToast.mock.calls.filter(([text]) =>
      String(text).includes('fileExplorer.folderSizes.unreadable'),
    )
    expect(unreadable).toHaveLength(2)
    // Same dedup id: a queued Space and the count it joined replace, never stack.
    expect(unreadable.every(([, opts]) => (opts as { id?: string }).id === 'folder-sizes-unreadable')).toBe(true)
  })
})
