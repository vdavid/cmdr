/**
 * Renaming a run of files in one keyboard flow: ArrowDown saves what's in the
 * editor and reopens it on the row below, ArrowUp on the row above.
 *
 * The data-safety question these tests exist for: several saves are in flight at
 * once while the editor has already moved on, so each one has to keep writing
 * the name that was typed for ITS file.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'

const { executeRenameSaveSpy, checkPermissionSpy, getSettingSpy, validateFilenameSpy, pathInsideArchiveSpy } =
  vi.hoisted(() => ({
    executeRenameSaveSpy:
      vi.fn<
        (
          target: { path: string; originalName: string },
          trimmedName: string,
          extensionPolicy: string,
          skipExtensionCheck?: boolean,
          volumeId?: string,
        ) => Promise<unknown>
      >(),
    checkPermissionSpy: vi.fn<() => Promise<string | null>>(),
    getSettingSpy: vi.fn<(id: string) => unknown>(),
    validateFilenameSpy: vi.fn(),
    pathInsideArchiveSpy: vi.fn<() => boolean>(),
  }))

vi.mock('$lib/tauri-commands', () => ({
  getFileAt: vi.fn(),
  getFileRange: vi.fn(),
  refreshListing: vi.fn(),
  getIpcErrorMessage: (e: unknown) => String(e),
  isIpcError: () => false,
  moveToTrash: vi.fn(),
}))
vi.mock('$lib/utils/filename-validation', () => ({
  validateFilename: validateFilenameSpy,
  getExtension: (name: string) => {
    const i = name.lastIndexOf('.')
    return i > 0 ? name.slice(i) : ''
  },
}))
vi.mock('../rename/rename-activation', () => ({ cancelClickToRename: vi.fn() }))
vi.mock('../rename/rename-operations', () => ({
  executeRenameSave: executeRenameSaveSpy,
  performRename: vi.fn(),
  checkPermission: checkPermissionSpy,
}))
vi.mock('$lib/settings', () => ({ getSetting: getSettingSpy }))
vi.mock('$lib/ui/toast', () => ({ addToastForPane: vi.fn(), dismissTransientToastsForPane: vi.fn() }))
vi.mock('$lib/intl/messages.svelte', () => ({ tString: (k: string) => k }))
vi.mock('./volume-capabilities', () => ({ pathInsideArchive: pathInsideArchiveSpy }))

import { getFileAt } from '$lib/tauri-commands'
import { buildFlow, chainListing, deferred } from './test-rename-flow'

/** `[file being renamed, name it is being given]` for every save the flow sent. */
function savedPairs(): [string, string][] {
  return executeRenameSaveSpy.mock.calls.map(([target, trimmedName]) => [target.originalName, trimmedName])
}

beforeEach(() => {
  vi.clearAllMocks()
  checkPermissionSpy.mockResolvedValue(null)
  pathInsideArchiveSpy.mockReturnValue(false)
  validateFilenameSpy.mockReturnValue({ severity: 'ok', message: '' })
  getSettingSpy.mockImplementation((id) => (id === 'fileOperations.allowFileExtensionChanges' ? 'ask' : undefined))
  executeRenameSaveSpy.mockResolvedValue({ type: 'success', newName: 'renamed.txt' })
})

describe('chaining the rename to the next file with the arrow keys', () => {
  /** Makes every save hang, so a whole chain can be in flight at once. */
  function slowBackend() {
    const inFlight: ReturnType<typeof deferred<unknown>>[] = []
    executeRenameSaveSpy.mockImplementation(() => {
      const save = deferred<unknown>()
      inFlight.push(save)
      return save.promise
    })
    return inFlight
  }

  it('DATA SAFETY: each chained save writes the name typed for its own file, with all of them still in flight', () => {
    const inFlight = slowBackend()
    const listing = chainListing(['a.txt', 'b.txt', 'c.txt'])
    const { rename, flow } = buildFlow(listing.staleEntryUnderCursor, true, listing.deps)

    flow.startRename() // the cursor is on a.txt
    flow.handleRenameInput('one.txt')
    flow.handleRenameStep('down', rename.sessionId)
    flow.handleRenameInput('two.txt')
    flow.handleRenameStep('down', rename.sessionId)
    flow.handleRenameInput('three.txt')
    flow.handleRenameSubmit()

    expect(inFlight).toHaveLength(3) // nothing has come back yet
    expect(savedPairs()).toEqual([
      ['a.txt', 'one.txt'],
      ['b.txt', 'two.txt'],
      ['c.txt', 'three.txt'],
    ])
    expect(executeRenameSaveSpy.mock.calls.map(([target]) => target.path)).toEqual([
      '/dir/a.txt',
      '/dir/b.txt',
      '/dir/c.txt',
    ])
  })

  it('renames the row that was beside the editor, not the file the cursor still reports', () => {
    // `entryUnderCursor` is filled by an async read keyed on the cursor index, so
    // right after a hop it still names the file the chain just left. Activating
    // through it would write the next name onto that file.
    const listing = chainListing(['a.txt', 'b.txt'])
    const { rename, flow } = buildFlow(listing.staleEntryUnderCursor, true, listing.deps)

    flow.startRename()
    flow.handleRenameInput('one.txt')
    flow.handleRenameStep('down', rename.sessionId)

    expect(rename.target?.path).toBe('/dir/b.txt')
    expect(rename.target?.originalName).toBe('b.txt')
  })

  it('scrolls the editor along with the cursor, so it stays on screen', () => {
    const listing = chainListing(['a.txt', 'b.txt', 'c.txt'])
    const { rename, flow } = buildFlow(listing.staleEntryUnderCursor, true, listing.deps)

    flow.startRename()
    flow.handleRenameStep('down', rename.sessionId)
    flow.handleRenameStep('down', rename.sessionId)

    expect(listing.moves).toEqual([2, 3])
  })

  it('opens the next editor on the untouched name, ready to be typed over', () => {
    const listing = chainListing(['a.txt', 'b.txt'])
    const { rename, flow } = buildFlow(listing.staleEntryUnderCursor, true, listing.deps)

    flow.startRename()
    flow.handleRenameInput('one.txt')
    flow.handleRenameStep('down', rename.sessionId)

    expect(rename.active).toBe(true)
    expect(rename.currentName).toBe('b.txt')
  })

  it('at the last row the key does nothing: the editor stays open with the edit intact', () => {
    const listing = chainListing(['a.txt', 'b.txt'], 2) // the cursor is on b.txt, the last row
    const { rename, flow } = buildFlow(listing.staleEntryUnderCursor, true, listing.deps)

    flow.startRename()
    flow.handleRenameInput('edited.txt')
    flow.handleRenameStep('down', rename.sessionId)

    expect(executeRenameSaveSpy).not.toHaveBeenCalled()
    expect(rename.active).toBe(true)
    expect(rename.currentName).toBe('edited.txt')
    expect(rename.target?.originalName).toBe('b.txt')
    expect(listing.moves).toEqual([])
  })

  it('at the first real row the key does nothing, and never steps onto the parent row', () => {
    const listing = chainListing(['a.txt', 'b.txt']) // the cursor is on a.txt, right below `..`
    const { rename, flow } = buildFlow(listing.staleEntryUnderCursor, true, listing.deps)

    flow.startRename()
    flow.handleRenameInput('edited.txt')
    flow.handleRenameStep('up', rename.sessionId)

    expect(executeRenameSaveSpy).not.toHaveBeenCalled()
    expect(rename.active).toBe(true)
    expect(rename.currentName).toBe('edited.txt')
    expect(rename.target?.originalName).toBe('a.txt')
    expect(listing.moves).toEqual([])
  })

  it('steps upwards the same way it steps down', () => {
    const listing = chainListing(['a.txt', 'b.txt', 'c.txt'], 3)
    const { rename, flow } = buildFlow(listing.staleEntryUnderCursor, true, listing.deps)

    flow.startRename()
    flow.handleRenameInput('renamed.txt')
    flow.handleRenameStep('up', rename.sessionId)

    expect(savedPairs()).toEqual([['c.txt', 'renamed.txt']])
    expect(rename.target?.originalName).toBe('b.txt')
    expect(listing.moves).toEqual([2])
  })

  it('an untouched name hops without touching the disk', () => {
    const listing = chainListing(['a.txt', 'b.txt'])
    const { rename, flow } = buildFlow(listing.staleEntryUnderCursor, true, listing.deps)

    flow.startRename()
    flow.handleRenameStep('down', rename.sessionId)

    expect(executeRenameSaveSpy).not.toHaveBeenCalled()
    expect(rename.target?.originalName).toBe('b.txt')
  })

  it('holding the arrow down rips through the directory: ten paired saves, one live session', () => {
    slowBackend()
    const names = Array.from({ length: 11 }, (_, i) => `f${String(i)}.txt`)
    const listing = chainListing(names)
    const { rename, flow } = buildFlow(listing.staleEntryUnderCursor, true, listing.deps)

    flow.startRename()
    for (let step = 0; step < 10; step++) {
      flow.handleRenameInput(`renamed-${String(step)}.txt`)
      flow.handleRenameStep('down', rename.sessionId)
    }

    expect(savedPairs()).toEqual(
      Array.from({ length: 10 }, (_, i) => [`f${String(i)}.txt`, `renamed-${String(i)}.txt`]),
    )
    expect(rename.active).toBe(true)
    expect(rename.target?.originalName).toBe('f10.txt')
    expect(listing.moves).toEqual([2, 3, 4, 5, 6, 7, 8, 9, 10, 11])
  })

  it('reads the neighbour from the backend when it has scrolled out of the loaded window', async () => {
    const listing = chainListing(['a.txt', 'b.txt'])
    listing.unload(2)
    vi.mocked(getFileAt).mockResolvedValue({ name: 'b.txt', path: '/dir/b.txt', isDirectory: false } as never)
    const { rename, flow } = buildFlow(listing.staleEntryUnderCursor, true, listing.deps)

    flow.startRename()
    flow.handleRenameInput('one.txt')
    flow.handleRenameStep('down', rename.sessionId)

    await vi.waitFor(() => {
      expect(rename.target?.path).toBe('/dir/b.txt')
    })
    // Backend indices skip the `..` row.
    expect(getFileAt).toHaveBeenCalledWith('lst-1', 1, false)
    expect(savedPairs()).toEqual([['a.txt', 'one.txt']])
  })

  it('a step from an editor that has already been replaced is dropped', () => {
    const listing = chainListing(['a.txt', 'b.txt', 'c.txt'])
    const { rename, flow } = buildFlow(listing.staleEntryUnderCursor, true, listing.deps)

    flow.startRename()
    const outgoing = rename.sessionId
    flow.handleRenameInput('one.txt')
    flow.handleRenameStep('down', outgoing)
    executeRenameSaveSpy.mockClear()

    // The outgoing editor's key repeat, landing after its replacement took over.
    flow.handleRenameStep('down', outgoing)

    expect(executeRenameSaveSpy).not.toHaveBeenCalled()
    expect(rename.target?.originalName).toBe('b.txt')
    expect(listing.moves).toEqual([2])
  })

  it('does nothing when no rename is running', () => {
    const listing = chainListing(['a.txt', 'b.txt'])
    const { rename, flow } = buildFlow(listing.staleEntryUnderCursor, true, listing.deps)

    flow.handleRenameStep('down', rename.sessionId)

    expect(rename.active).toBe(false)
    expect(listing.moves).toEqual([])
  })
})
