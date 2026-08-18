/**
 * Renaming a run of files in one keyboard flow: ArrowDown saves what's in the
 * editor and reopens it on the row below, ArrowUp on the row above.
 *
 * The data-safety question these tests exist for: several saves are in flight at
 * once while the editor has already moved on, so each one has to keep writing
 * the name that was typed for ITS file.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'

const {
  executeRenameSaveSpy,
  checkPermissionSpy,
  getSettingSpy,
  validateFilenameSpy,
  pathInsideArchiveSpy,
  tStringSpy,
} = vi.hoisted(() => ({
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
  tStringSpy: vi.fn((key: string, _params?: Record<string, string>) => key),
}))

vi.mock('$lib/tauri-commands', () => ({
  getFileAt: vi.fn(),
  getFileRange: vi.fn(),
  refreshListing: vi.fn(),
  getIpcErrorMessage: (e: unknown) => String(e),
  isIpcError: () => false,
  moveToTrash: vi.fn(),
}))
// Only `validateFilename` is stubbed; the extension-policy tests below hand the
// spy the real implementation back, because what they're checking IS how the
// real validator grades an extension change under each policy.
vi.mock('$lib/utils/filename-validation', async (importOriginal) => ({
  ...(await importOriginal<typeof import('$lib/utils/filename-validation')>()),
  validateFilename: validateFilenameSpy,
}))
vi.mock('../rename/rename-activation', () => ({ cancelClickToRename: vi.fn() }))
vi.mock('../rename/rename-operations', () => ({
  executeRenameSave: executeRenameSaveSpy,
  performRename: vi.fn(),
  checkPermission: checkPermissionSpy,
}))
vi.mock('$lib/settings', () => ({ getSetting: getSettingSpy }))
vi.mock('$lib/ui/toast', () => ({ addToastForPane: vi.fn(), dismissTransientToastsForPane: vi.fn() }))
vi.mock('$lib/intl/messages.svelte', () => ({ tString: tStringSpy }))
vi.mock('./volume-capabilities', () => ({ pathInsideArchive: pathInsideArchiveSpy }))

import { getFileAt, getFileRange } from '$lib/tauri-commands'
import { addToastForPane } from '$lib/ui/toast'
import { buildFlow, chainListing, deferred } from './test-rename-flow'

/** The real validator, for the tests that are about how it grades a name. */
const { validateFilename: gradeName } = await vi.importActual<typeof import('$lib/utils/filename-validation')>(
  '$lib/utils/filename-validation',
)

/** `[file being renamed, name it is being given]` for every save the flow sent. */
function savedPairs(): [string, string][] {
  return executeRenameSaveSpy.mock.calls.map(([target, trimmedName]) => [target.originalName, trimmedName])
}

/** The message keys and params every toast the flow raised was built from. */
function toastedKeys() {
  return tStringSpy.mock.calls
}

beforeEach(() => {
  vi.clearAllMocks()
  checkPermissionSpy.mockResolvedValue(null)
  pathInsideArchiveSpy.mockReturnValue(false)
  validateFilenameSpy.mockReturnValue({ severity: 'ok', message: '' })
  getSettingSpy.mockImplementation((id) => (id === 'fileOperations.allowFileExtensionChanges' ? 'ask' : undefined))
  executeRenameSaveSpy.mockResolvedValue({ type: 'success', newName: 'renamed.txt' })
  vi.mocked(getFileRange).mockResolvedValue([] as never)
})

/** Answers a sibling-name read with the listing's own rows. */
function pageableListing(names: string[]) {
  vi.mocked(getFileRange).mockImplementation((_id, start, count) =>
    Promise.resolve(names.slice(start, start + count).map((name) => ({ name })) as never),
  )
}

/** The directory names the last validation graded a typed name against. */
function lastValidatedAgainst(): string[] {
  const calls = validateFilenameSpy.mock.calls
  return calls[calls.length - 1][3] as string[]
}

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

  it('keeps the live session editing while the earlier saves land behind it, in any order', async () => {
    const inFlight = slowBackend()
    const listing = chainListing(['a.txt', 'b.txt', 'c.txt'])
    const { rename, flow } = buildFlow(listing.staleEntryUnderCursor, true, listing.deps)

    flow.startRename()
    flow.handleRenameInput('one.txt')
    flow.handleRenameStep('down', rename.sessionId)
    flow.handleRenameInput('two.txt')
    flow.handleRenameStep('down', rename.sessionId)
    flow.handleRenameInput('three.txt')

    // Both earlier saves come back while the third name is still being typed,
    // and the second one beats the first.
    inFlight[1].resolve({ type: 'success', newName: 'two.txt' })
    inFlight[0].resolve({ type: 'success', newName: 'one.txt' })
    await inFlight[1].promise
    await inFlight[0].promise
    await Promise.resolve()

    expect(rename.active).toBe(true)
    expect(rename.target?.originalName).toBe('c.txt')
    expect(rename.currentName).toBe('three.txt')
    // Neither may drag the cursor back to the file it renamed.
    expect(flow.pendingCursorName).toBeNull()
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

  it('leaves the editor alone when the neighbour cannot be read at all', async () => {
    const listing = chainListing(['a.txt', 'b.txt'])
    listing.unload(2)
    vi.mocked(getFileAt).mockResolvedValue(null)
    const { rename, flow } = buildFlow(listing.staleEntryUnderCursor, true, listing.deps)

    flow.startRename()
    flow.handleRenameInput('one.txt')
    flow.handleRenameStep('down', rename.sessionId)

    await vi.waitFor(() => {
      expect(getFileAt).toHaveBeenCalled()
    })
    // A row we can't read is the same as no row there: nothing is sent, nothing
    // moves, and the name the user typed is still in the editor.
    expect(executeRenameSaveSpy).not.toHaveBeenCalled()
    expect(rename.active).toBe(true)
    expect(rename.currentName).toBe('one.txt')
    expect(rename.target?.originalName).toBe('a.txt')
    expect(listing.moves).toEqual([])
  })

  it('sends what the user typed while the neighbour was being read, not what was there at the keypress', async () => {
    const listing = chainListing(['a.txt', 'b.txt'])
    listing.unload(2)
    const fetched = deferred<unknown>()
    vi.mocked(getFileAt).mockReturnValue(fetched.promise as never)
    const { rename, flow } = buildFlow(listing.staleEntryUnderCursor, true, listing.deps)

    flow.startRename()
    flow.handleRenameInput('one.txt')
    flow.handleRenameStep('down', rename.sessionId)
    // The read costs a round trip, and the user keeps typing through it.
    flow.handleRenameInput('one-final.txt')
    fetched.resolve({ name: 'b.txt', path: '/dir/b.txt', isDirectory: false })

    await vi.waitFor(() => {
      expect(rename.target?.path).toBe('/dir/b.txt')
    })
    expect(savedPairs()).toEqual([['a.txt', 'one-final.txt']])
  })

  it('drops a step whose neighbour arrives after the user has ended the rename', async () => {
    const listing = chainListing(['a.txt', 'b.txt'])
    listing.unload(2)
    const fetched = deferred<unknown>()
    vi.mocked(getFileAt).mockReturnValue(fetched.promise as never)
    const { rename, flow } = buildFlow(listing.staleEntryUnderCursor, true, listing.deps)

    flow.startRename()
    flow.handleRenameInput('one.txt')
    flow.handleRenameStep('down', rename.sessionId)
    flow.handleRenameCancel(rename.sessionId) // Escape, while the read is still out
    fetched.resolve({ name: 'b.txt', path: '/dir/b.txt', isDirectory: false })
    await Promise.resolve()
    await Promise.resolve()

    // The user said stop: the step must not reopen the editor on the next file,
    // nor send the name they walked away from.
    expect(rename.active).toBe(false)
    expect(executeRenameSaveSpy).not.toHaveBeenCalled()
    expect(listing.moves).toEqual([])
  })

  it('a save still in flight when the user ends the chain can never reopen or steer the editor', async () => {
    const inFlight = slowBackend()
    const listing = chainListing(['a.txt', 'b.txt', 'c.txt'])
    const { rename, flow, onRequestFocus } = buildFlow(listing.staleEntryUnderCursor, true, listing.deps)

    flow.startRename()
    flow.handleRenameInput('one.txt')
    flow.handleRenameStep('down', rename.sessionId)
    flow.handleRenameCancel(rename.sessionId) // Escape ends the chain
    onRequestFocus.mockClear()

    inFlight[0].resolve({ type: 'success', newName: 'one.txt' })
    await inFlight[0].promise
    await Promise.resolve()

    expect(rename.active).toBe(false)
    expect(flow.pendingCursorName).toBeNull()
    expect(onRequestFocus).not.toHaveBeenCalled()
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

describe('what becomes of the name in the editor when the arrow moves on', () => {
  /** The options the flow attached to the toast it raised. */
  function lastToastOptions() {
    const calls = vi.mocked(addToastForPane).mock.calls
    return calls[calls.length - 1][2]
  }

  it('drops a name it already knows is unusable, names the file that kept its own, and still hops', () => {
    validateFilenameSpy.mockReturnValue({ severity: 'error', message: 'unusable' })
    const listing = chainListing(['a.txt', 'b.txt'])
    const { rename, flow } = buildFlow(listing.staleEntryUnderCursor, true, listing.deps)

    flow.startRename()
    flow.handleRenameInput('a/b.txt')
    flow.handleRenameStep('down', rename.sessionId)

    expect(executeRenameSaveSpy).not.toHaveBeenCalled()
    // The user keeps moving; only the edit is dropped.
    expect(rename.active).toBe(true)
    expect(rename.target?.originalName).toBe('b.txt')
    expect(listing.moves).toEqual([2])
    expect(toastedKeys()).toContainEqual([
      'fileExplorer.rename.chainKeptOriginalName',
      { reason: 'unusable', name: 'a.txt' },
    ])
    // The next keystroke dismisses this pane's transient toasts, which is exactly
    // when the user is typing the next name.
    expect(lastToastOptions()).toMatchObject({ level: 'warn', dismissal: 'persistent' })
  })

  it('drops a name the backend finds taken, with a toast and never a dialog', async () => {
    executeRenameSaveSpy.mockResolvedValue({
      type: 'conflict',
      validity: { valid: true, hasConflict: true, isCaseOnlyRename: false },
    })
    const listing = chainListing(['a.txt', 'b.txt'])
    const { rename, flow } = buildFlow(listing.staleEntryUnderCursor, true, listing.deps)

    flow.startRename()
    flow.handleRenameInput('taken.txt')
    flow.handleRenameStep('down', rename.sessionId)

    await vi.waitFor(() => {
      expect(toastedKeys()).toContainEqual([
        'fileExplorer.rename.chainKeptOriginalName',
        { reason: 'fileOperations.validation.conflict', name: 'a.txt' },
      ])
    })
    // The chain must not stop to ask about a file the user has moved past.
    expect(flow.conflictDialogState).toBeNull()
    expect(flow.extensionDialogState).toBeNull()
    expect(rename.active).toBe(true)
    expect(rename.target?.originalName).toBe('b.txt')
    // The toast says which name was taken, not only which file kept its own.
    expect(toastedKeys()).toContainEqual(['fileOperations.validation.conflict', { name: 'taken.txt' }])
    expect(lastToastOptions()).toMatchObject({ level: 'warn', dismissal: 'persistent' })
  })

  it('commits an extension change with no dialog while the policy asks', () => {
    validateFilenameSpy.mockImplementation(gradeName)
    const listing = chainListing(['a.txt', 'b.txt'])
    const { rename, flow } = buildFlow(listing.staleEntryUnderCursor, true, listing.deps)

    flow.startRename()
    flow.handleRenameInput('a.png')
    flow.handleRenameStep('down', rename.sessionId)

    expect(savedPairs()).toEqual([['a.txt', 'a.png']])
    expect(executeRenameSaveSpy.mock.calls[0][3]).toBe(true) // the dialog is skipped
    expect(flow.extensionDialogState).toBeNull()
  })

  it('drops an extension change while the policy forbids one: skipping the dialog is not overriding the setting', () => {
    getSettingSpy.mockImplementation((id) => (id === 'fileOperations.allowFileExtensionChanges' ? 'no' : undefined))
    validateFilenameSpy.mockImplementation(gradeName)
    const listing = chainListing(['a.txt', 'b.txt'])
    const { rename, flow } = buildFlow(listing.staleEntryUnderCursor, true, listing.deps)

    flow.startRename()
    flow.handleRenameInput('a.png')
    flow.handleRenameStep('down', rename.sessionId)

    expect(executeRenameSaveSpy).not.toHaveBeenCalled()
    expect(rename.target?.originalName).toBe('b.txt')
    expect(toastedKeys()).toContainEqual([
      'fileExplorer.rename.chainKeptOriginalName',
      { reason: 'fileOperations.validation.extensionChangeBlocked', name: 'a.txt' },
    ])
  })

  it('sends the name anyway when only the sibling-name list calls it taken', () => {
    // That snapshot is read when the session opens, and the chain's own renames
    // rewrite the directory under it, so mid-chain it is stale by construction.
    // Dropping the edit on it would throw away a name that is perfectly free.
    validateFilenameSpy.mockReturnValue({ severity: 'warning', message: 'looks taken' })
    const listing = chainListing(['a.txt', 'b.txt'])
    const { rename, flow } = buildFlow(listing.staleEntryUnderCursor, true, listing.deps)

    flow.startRename()
    flow.handleRenameInput('b.txt')
    flow.handleRenameStep('down', rename.sessionId)

    expect(savedPairs()).toEqual([['a.txt', 'b.txt']])
  })
})

describe('the directory names the red border checks a typed name against', () => {
  it('reads the directory once for a whole chain, however many rows it crosses', async () => {
    const names = ['a.txt', 'b.txt', 'c.txt', 'd.txt']
    pageableListing(names)
    const listing = chainListing(names)
    const { rename, flow } = buildFlow(listing.staleEntryUnderCursor, true, listing.deps)

    flow.startRename()
    flow.handleRenameStep('down', rename.sessionId)
    flow.handleRenameStep('down', rename.sessionId)
    flow.handleRenameStep('down', rename.sessionId)

    await vi.waitFor(() => {
      expect(getFileRange).toHaveBeenCalled()
    })
    // Paging the whole listing per activation is what makes chaining crawl on a
    // big directory, and it learns the same thing every time.
    expect(getFileRange).toHaveBeenCalledTimes(1)
  })

  it('reads the directory again for a rename started outside a chain', async () => {
    const names = ['a.txt', 'b.txt']
    pageableListing(names)
    const listing = chainListing(names)
    const { flow } = buildFlow(listing.staleEntryUnderCursor, true, listing.deps)

    flow.startRename()
    await vi.waitFor(() => {
      expect(getFileRange).toHaveBeenCalledTimes(1)
    })
    flow.cancelRename()
    flow.startRename()

    await vi.waitFor(() => {
      expect(getFileRange).toHaveBeenCalledTimes(2)
    })
  })

  it("follows the chain's own renames, so a name it freed stops looking taken", async () => {
    const names = ['a.txt', 'b.txt', 'c.txt']
    pageableListing(names)
    executeRenameSaveSpy.mockResolvedValue({ type: 'success', newName: 'z.txt' })
    const listing = chainListing(names)
    const { rename, flow } = buildFlow(listing.staleEntryUnderCursor, true, listing.deps)

    flow.startRename()
    flow.handleRenameInput('z.txt')
    flow.handleRenameStep('down', rename.sessionId)

    // Typing is what re-reads the list, so the assertion types again each poll
    // until the save has landed and patched it.
    await vi.waitFor(() => {
      flow.handleRenameInput('a.txt')
      expect(lastValidatedAgainst()).toEqual(['b.txt', 'c.txt', 'z.txt'])
    })
  })
})
