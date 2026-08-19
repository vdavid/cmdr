/**
 * The decision behind "a duplicate ends in the rename editor", and the one rule
 * the whole thing rests on: **the trigger opts in, nothing else does.**
 *
 * A same-folder copy dispatched by the Duplicate command or by a drag is
 * byte-for-byte the operation paste and F5 dispatch. The only thing telling them
 * apart is `duplicateFollowUp`, so the test that it's honoured is the one that
 * keeps those two gestures from growing an editor nobody asked for.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import type { OperationItemView } from '$lib/tauri-commands'
import type { FilePaneAPI } from './types'

const { getOperationLogDetailSpy, moveCursorSpy, findFileIndexSpy, onDirectoryDiffSpy, whenSettledSpy } = vi.hoisted(
  () => ({
    getOperationLogDetailSpy: vi.fn(),
    moveCursorSpy: vi.fn<() => Promise<void>>(),
    findFileIndexSpy: vi.fn(),
    onDirectoryDiffSpy: vi.fn(),
    whenSettledSpy: vi.fn<(operationId: string) => Promise<boolean>>(),
  }),
)

vi.mock('$lib/tauri-commands', () => ({
  getOperationLogDetail: getOperationLogDetailSpy,
  findFileIndex: findFileIndexSpy,
  onDirectoryDiff: onDirectoryDiffSpy,
}))
vi.mock('$lib/file-operations/mkdir/new-folder-operations', () => ({ moveCursorToNewFolder: moveCursorSpy }))
vi.mock('$lib/file-operations/settled-operations', () => ({ whenOperationSettled: whenSettledSpy }))

import {
  duplicateRenameDestination,
  duplicatedEntryName,
  openRenameOnDuplicate,
  type DuplicateRenameContext,
} from './duplicate-rename'

const FOLDER = '/Users/me/photos'

function context(overrides: Partial<DuplicateRenameContext> = {}): DuplicateRenameContext {
  return {
    duplicateFollowUp: 'openRenameEditor',
    operationType: 'copy',
    sourcePaths: [`${FOLDER}/photo.jpg`],
    sourceFolderPath: FOLDER,
    destinationPath: FOLDER,
    ...overrides,
  }
}

function item(overrides: Partial<OperationItemView> = {}): OperationItemView {
  return {
    seq: 1,
    entryType: 'file',
    rowRole: 'rollbackUnit',
    sourceVolumeId: 'root',
    sourcePath: `${FOLDER}/photo.jpg`,
    destVolumeId: 'root',
    destPath: `${FOLDER}/photo (1).jpg`,
    size: 12,
    mtime: null,
    outcome: 'done',
    overwrote: false,
    rollbackSkipReason: null,
    ...overrides,
  }
}

function makePane(currentPath = FOLDER) {
  const spies = {
    getCurrentPath: vi.fn(() => currentPath),
    getListingId: vi.fn(() => 'listing-1'),
    hasParentEntry: vi.fn(() => true),
    startRename: vi.fn(),
  }
  return { ref: spies as unknown as FilePaneAPI, spies }
}

beforeEach(() => {
  vi.clearAllMocks()
  moveCursorSpy.mockResolvedValue(undefined)
  whenSettledSpy.mockResolvedValue(true)
  getOperationLogDetailSpy.mockResolvedValue({ operation: {}, items: [item()], totalItems: 1 })
})

describe('duplicateRenameDestination — which settled transfers get an editor', () => {
  it('a single-item duplicate the trigger opted into names the folder it landed in', () => {
    expect(duplicateRenameDestination(context())).toBe(FOLDER)
  })

  it('a duplicate the trigger did NOT opt into gets nothing (the Duplicate command and drag)', () => {
    expect(duplicateRenameDestination(context({ duplicateFollowUp: 'nothing' }))).toBeNull()
  })

  it('more than one source is more than one thing to name', () => {
    expect(duplicateRenameDestination(context({ sourcePaths: [`${FOLDER}/a.jpg`, `${FOLDER}/b.jpg`] }))).toBeNull()
  })

  it('a copy into a DIFFERENT folder is a copy, not a duplicate', () => {
    expect(duplicateRenameDestination(context({ destinationPath: '/Users/me/backup' }))).toBeNull()
  })

  it('a move is never a duplicate', () => {
    expect(duplicateRenameDestination(context({ operationType: 'move' }))).toBeNull()
  })

  it('an empty birth slot answers nothing', () => {
    expect(duplicateRenameDestination(null)).toBeNull()
  })

  it('a trailing slash on either side is the same folder', () => {
    expect(duplicateRenameDestination(context({ destinationPath: `${FOLDER}/` }))).toBe(FOLDER)
  })

  it('root is the folder that IS a trailing slash', () => {
    expect(duplicateRenameDestination(context({ destinationPath: '/', sourceFolderPath: '/' }))).toBe('/')
  })
})

describe('duplicatedEntryName — reading the generated name off the journal', () => {
  it('a duplicated file is its own row', () => {
    expect(duplicatedEntryName(FOLDER, item())).toBe('photo (1).jpg')
  })

  it('a duplicated folder is the first segment below the destination, off any leaf', () => {
    expect(duplicatedEntryName(FOLDER, item({ destPath: `${FOLDER}/docs (1)/sub/b.txt` }))).toBe('docs (1)')
  })

  it('root needs no second slash', () => {
    expect(duplicatedEntryName('/', item({ destPath: '/photo (1).jpg' }))).toBe('photo (1).jpg')
  })

  it('a missing destination path is no name, not an error', () => {
    expect(duplicatedEntryName(FOLDER, item({ destPath: null }))).toBeNull()
    expect(duplicatedEntryName(FOLDER, item({ destPath: '' }))).toBeNull()
  })

  it('a row that never landed is no name', () => {
    expect(duplicatedEntryName(FOLDER, item({ outcome: 'skipped' }))).toBeNull()
    expect(duplicatedEntryName(FOLDER, item({ outcome: 'failed' }))).toBeNull()
  })

  it('no row at all is no name', () => {
    expect(duplicatedEntryName(FOLDER, undefined)).toBeNull()
  })

  it('a path outside the destination is not this duplicate', () => {
    expect(duplicatedEntryName(FOLDER, item({ destPath: '/Users/me/backup/photo (1).jpg' }))).toBeNull()
  })
})

describe('openRenameOnDuplicate — the settled tail', () => {
  it('lands the cursor on the new item and opens the editor on exactly that name', async () => {
    const pane = makePane()

    await openRenameOnDuplicate({ context: context(), operationId: 'op-1', paneRef: pane.ref, showHiddenFiles: false })

    expect(getOperationLogDetailSpy).toHaveBeenCalledExactlyOnceWith('op-1', 1, 0)
    expect(moveCursorSpy).toHaveBeenCalledTimes(1)
    expect(moveCursorSpy.mock.calls[0]?.slice(0, 5)).toEqual(['listing-1', 'photo (1).jpg', pane.ref, true, false])
    expect(pane.spies.startRename).toHaveBeenCalledExactlyOnceWith({
      suppressExtensionWarning: true,
      expectedName: 'photo (1).jpg',
    })
  })

  it('a duplicate from a trigger that did not opt in never even asks the journal', async () => {
    const pane = makePane()

    await openRenameOnDuplicate({
      context: context({ duplicateFollowUp: 'nothing' }),
      operationId: 'op-1',
      paneRef: pane.ref,
      showHiddenFiles: false,
    })

    expect(getOperationLogDetailSpy).not.toHaveBeenCalled()
    expect(pane.spies.startRename).not.toHaveBeenCalled()
  })

  it('a multi-item duplicate never asks the journal', async () => {
    const pane = makePane()

    await openRenameOnDuplicate({
      context: context({ sourcePaths: [`${FOLDER}/a.jpg`, `${FOLDER}/b.jpg`] }),
      operationId: 'op-1',
      paneRef: pane.ref,
      showHiddenFiles: false,
    })

    expect(getOperationLogDetailSpy).not.toHaveBeenCalled()
    expect(pane.spies.startRename).not.toHaveBeenCalled()
  })

  it('an unnamed operation gets no editor', async () => {
    const pane = makePane()

    await openRenameOnDuplicate({ context: context(), operationId: null, paneRef: pane.ref, showHiddenFiles: false })

    expect(pane.spies.startRename).not.toHaveBeenCalled()
  })

  it('a pane showing another folder is left alone', async () => {
    const pane = makePane('/Users/me/elsewhere')

    await openRenameOnDuplicate({ context: context(), operationId: 'op-1', paneRef: pane.ref, showHiddenFiles: false })

    expect(getOperationLogDetailSpy).not.toHaveBeenCalled()
    expect(pane.spies.startRename).not.toHaveBeenCalled()
  })

  it('an empty journal page keeps the generated name, silently', async () => {
    getOperationLogDetailSpy.mockResolvedValue({ operation: {}, items: [], totalItems: 0 })
    const pane = makePane()

    await openRenameOnDuplicate({ context: context(), operationId: 'op-1', paneRef: pane.ref, showHiddenFiles: false })

    expect(moveCursorSpy).not.toHaveBeenCalled()
    expect(pane.spies.startRename).not.toHaveBeenCalled()
  })

  it('a row with no destination path keeps the generated name, silently', async () => {
    getOperationLogDetailSpy.mockResolvedValue({ operation: {}, items: [item({ destPath: null })], totalItems: 1 })
    const pane = makePane()

    await openRenameOnDuplicate({ context: context(), operationId: 'op-1', paneRef: pane.ref, showHiddenFiles: false })

    expect(pane.spies.startRename).not.toHaveBeenCalled()
  })

  it('does NOT read the journal until the operation has SETTLED', async () => {
    // The regression anchor. The journal batches item rows in memory and flushes
    // them inside its finalize barrier, which runs AFTER the handler emitted
    // `write-complete`, so a single-item duplicate has no readable row at
    // complete time. Reading there returns an empty page and the editor silently
    // never opens: green stubs, dead feature. Move this read back to the
    // terminal event and this test says so.
    const pane = makePane()
    let releaseSettle: (settled: boolean) => void = () => {}
    whenSettledSpy.mockReturnValue(
      new Promise<boolean>((resolve) => {
        releaseSettle = resolve
      }),
    )

    const running = openRenameOnDuplicate({
      context: context(),
      operationId: 'op-1',
      paneRef: pane.ref,
      showHiddenFiles: false,
    })
    await Promise.resolve()

    expect(whenSettledSpy).toHaveBeenCalledExactlyOnceWith('op-1')
    expect(getOperationLogDetailSpy).not.toHaveBeenCalled()

    releaseSettle(true)
    await running

    expect(getOperationLogDetailSpy).toHaveBeenCalledExactlyOnceWith('op-1', 1, 0)
    expect(pane.spies.startRename).toHaveBeenCalledTimes(1)
  })

  it('an operation that never settles keeps the generated name, silently', async () => {
    whenSettledSpy.mockResolvedValue(false)
    const pane = makePane()

    await openRenameOnDuplicate({ context: context(), operationId: 'op-1', paneRef: pane.ref, showHiddenFiles: false })

    expect(getOperationLogDetailSpy).not.toHaveBeenCalled()
    expect(pane.spies.startRename).not.toHaveBeenCalled()
  })

  it('a journal that cannot be read is not an error the user hears about', async () => {
    getOperationLogDetailSpy.mockRejectedValue(new Error('journal closed'))
    const pane = makePane()

    await expect(
      openRenameOnDuplicate({ context: context(), operationId: 'op-1', paneRef: pane.ref, showHiddenFiles: false }),
    ).resolves.toBeUndefined()
    expect(pane.spies.startRename).not.toHaveBeenCalled()
  })
})
