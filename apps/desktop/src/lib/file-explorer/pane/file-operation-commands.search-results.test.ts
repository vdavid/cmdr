import { describe, it, expect, vi, beforeEach } from 'vitest'

// The search-results (snapshot) pane half of the `file-operation-commands`
// specs: every opener that has to resolve `snapshot.entries[i]` instead of a
// backend listing. Stubs and fixtures are shared with
// `file-operation-commands.test.ts` through `file-operation-commands.test-harness.ts`,
// which each factory below reaches by a lazy `await import`.
vi.mock('$lib/tauri-commands', async () => {
  const { spies } = await import('./file-operation-commands.test-harness')
  return { DEFAULT_VOLUME_ID: 'root', getFileAt: spies.getFileAt, getFilesAtIndices: spies.getFilesAtIndices }
})

vi.mock('$lib/ui/toast', async () => ({
  addToast: (await import('./file-operation-commands.test-harness')).spies.addToast,
}))

vi.mock('$lib/search/snapshot-store.svelte', async () => {
  const { spies, resolveSnapshotEntriesStub } = await import('./file-operation-commands.test-harness')
  return { getSnapshot: spies.getSnapshot, resolveSnapshotEntries: resolveSnapshotEntriesStub }
})

vi.mock('$lib/stores/volume-store.svelte', () => ({ getVolumes: () => [] }))

vi.mock('$lib/search/capabilities', () => ({
  SEARCH_RESULTS_NOT_A_FOLDER_TOAST: "Search results aren't a folder. Pick a real destination.",
}))

vi.mock('$lib/file-viewer/open-viewer', async () => ({
  openFileViewer: (await import('./file-operation-commands.test-harness')).spies.openFileViewer,
}))

vi.mock('$lib/file-operations/mkdir/new-folder-operations', async () => ({
  getInitialFolderName: (await import('./file-operation-commands.test-harness')).spies.getInitialFolderName,
}))

vi.mock('$lib/file-operations/mkfile/new-file-operations', async () => ({
  getInitialFileName: (await import('./file-operation-commands.test-harness')).spies.getInitialFileName,
}))

// `buildTransferPropsFromSnapshot` stays real: it is the builder under test here.
vi.mock('./transfer-operations', async (importOriginal) => {
  const actual = await importOriginal<typeof import('./transfer-operations')>()
  const { spies } = await import('./file-operation-commands.test-harness')
  return {
    ...actual,
    buildTransferPropsFromSelection: spies.buildTransferPropsFromSelection,
    buildTransferPropsFromCursor: spies.buildTransferPropsFromCursor,
  }
})

vi.mock('$lib/logging/logger', async () => {
  const { spies } = await import('./file-operation-commands.test-harness')
  return { getAppLogger: () => ({ error: vi.fn(), warn: spies.logWarn, info: vi.fn(), debug: spies.logDebug }) }
})

import { createFileOperationCommands } from './file-operation-commands'
import {
  spies,
  buildAccess,
  buildDialogs,
  buildPaneRef,
  snapshot,
  snapshotEntry,
  volume,
  type DialogsStub,
} from './file-operation-commands.test-harness'

const { addToast: addToastSpy, getSnapshot: getSnapshotSpy } = spies

function create(access: ReturnType<typeof buildAccess>, dialogs: DialogsStub) {
  return createFileOperationCommands(access, dialogs as unknown as Parameters<typeof createFileOperationCommands>[1])
}

beforeEach(() => {
  vi.clearAllMocks()
})

describe('openTransferDialog on a search-results pane', () => {
  it('warns with the search-results destination toast when the opposite pane is a snapshot', async () => {
    const access = buildAccess({ focusedPane: 'left', volumeIds: { left: 'root', right: 'search-results' } })
    const dialogs = buildDialogs()

    await create(access, dialogs).openTransferDialog('copy')

    expect(addToastSpy).toHaveBeenCalledWith("Search results aren't a folder. Pick a real destination.", {
      level: 'warn',
    })
    expect(dialogs.showTransfer).not.toHaveBeenCalled()
  })

  it('does not show the search-results toast for a network destination (PR3: kind-scoped)', async () => {
    // A network dest also has `canWrite: false`, but the dest-block toast is
    // scoped to the search-results KIND. Historically a network dest fell through
    // here silently; converting the gate to `!canWrite` must not start
    // toasting it. The transfer then proceeds past the guard as before.
    const access = buildAccess({
      focusedPane: 'left',
      volumeIds: { left: 'root', right: 'network' },
      paneRefs: { left: buildPaneRef({ listingId: null }) },
    })
    const dialogs = buildDialogs()

    await create(access, dialogs).openTransferDialog('copy')

    expect(addToastSpy).not.toHaveBeenCalledWith("Search results aren't a folder. Pick a real destination.", {
      level: 'warn',
    })
  })

  it('builds snapshot transfer props for a search-results source pane', async () => {
    getSnapshotSpy.mockReturnValue(
      snapshot([snapshotEntry({ path: '/real/a.txt' }), snapshotEntry({ path: '/real/b.txt' })]),
    )
    const paneRef = buildPaneRef({ currentPath: 'search-results://sr-1', selectedIndices: [0, 1] })
    const access = buildAccess({
      focusedPane: 'left',
      paneRefs: { left: paneRef },
      volumeIds: { left: 'search-results', right: 'root' },
    })
    const dialogs = buildDialogs()

    await create(access, dialogs).openTransferDialog('move')

    expect(dialogs.showTransfer).toHaveBeenCalledTimes(1)
    expect(dialogs.showTransfer.mock.calls[0][0]).toMatchObject({
      operationType: 'move',
      sourcePaths: ['/real/a.txt', '/real/b.txt'],
    })
  })

  it('copies from the volume the rows live on, so the transfer routes through its backend', async () => {
    // `sourceVolumeId` picks the copy/move dispatch path
    // (`transfer/transfer-dispatch.ts::isVolumeMove`, `dispatchCopy`). Rows found
    // on a non-boot volume have to name it, or a move off an SMB share or an MTP
    // storage takes the local-filesystem fast path.
    getSnapshotSpy.mockReturnValue(
      snapshot([snapshotEntry({ path: '/Volumes/Stick/a.txt', parentPath: '/Volumes/Stick' })]),
    )
    const paneRef = buildPaneRef({ currentPath: 'search-results://sr-1', selectedIndices: [0] })
    const access = buildAccess({
      focusedPane: 'left',
      paneRefs: { left: paneRef },
      volumeIds: { left: 'search-results', right: 'root' },
      volumes: [volume({ id: 'root', path: '/' }), volume({ id: 'vol-stick', name: 'Stick', path: '/Volumes/Stick' })],
    })
    const dialogs = buildDialogs()

    await create(access, dialogs).openTransferDialog('move')

    expect(dialogs.showTransfer.mock.calls[0][0]).toMatchObject({ sourceVolumeId: 'vol-stick' })
  })

  it('does not open a snapshot transfer when the snapshot index is stale (out of range)', async () => {
    getSnapshotSpy.mockReturnValue(snapshot([snapshotEntry()]))
    const paneRef = buildPaneRef({ currentPath: 'search-results://sr-1', selectedIndices: [9] })
    const access = buildAccess({
      focusedPane: 'left',
      paneRefs: { left: paneRef },
      volumeIds: { left: 'search-results', right: 'root' },
    })
    const dialogs = buildDialogs()

    await create(access, dialogs).openTransferDialog('move')

    expect(dialogs.showTransfer).not.toHaveBeenCalled()
  })

  it('does not open a transfer for a snapshot with no rows at all', async () => {
    getSnapshotSpy.mockReturnValue(snapshot([]))
    const paneRef = buildPaneRef({ currentPath: 'search-results://sr-1', selectedIndices: [], cursorIndex: 0 })
    const access = buildAccess({
      focusedPane: 'left',
      paneRefs: { left: paneRef },
      volumeIds: { left: 'search-results', right: 'root' },
    })
    const dialogs = buildDialogs()

    await create(access, dialogs).openTransferDialog('copy')

    expect(dialogs.showTransfer).not.toHaveBeenCalled()
    expect(dialogs.showAlert).not.toHaveBeenCalled()
  })

  it('does not open a snapshot transfer when the snapshot is missing', async () => {
    getSnapshotSpy.mockReturnValue(undefined)
    const paneRef = buildPaneRef({ currentPath: 'search-results://sr-1', selectedIndices: [0] })
    const access = buildAccess({
      focusedPane: 'left',
      paneRefs: { left: paneRef },
      volumeIds: { left: 'search-results', right: 'root' },
    })
    const dialogs = buildDialogs()

    await create(access, dialogs).openTransferDialog('move')

    expect(dialogs.showTransfer).not.toHaveBeenCalled()
  })
})

describe('openDeleteDialog on a search-results pane', () => {
  it('reports the volume the rows actually live on, not root', async () => {
    // A search covers one volume, and it need not be the boot drive: any volume
    // with a persisted index is searchable (`src-tauri/src/search/volumes.rs`).
    // A permanent delete routes on `sourceVolumeId`, so reporting `root` for rows
    // on another volume sends the operation down the local-filesystem path.
    getSnapshotSpy.mockReturnValue(
      snapshot([
        snapshotEntry({ name: 'a.txt', path: '/Volumes/Stick/a.txt', parentPath: '/Volumes/Stick' }),
        snapshotEntry({ name: 'b.txt', path: '/Volumes/Stick/b.txt', parentPath: '/Volumes/Stick' }),
      ]),
    )
    const paneRef = buildPaneRef({ currentPath: 'search-results://sr-1', selectedIndices: [0, 1] })
    const access = buildAccess({
      paneRefs: { left: paneRef },
      volumeIds: { left: 'search-results' },
      volumes: [
        volume({ id: 'root', path: '/' }),
        volume({ id: 'vol-stick', name: 'Stick', path: '/Volumes/Stick', supportsTrash: false }),
      ],
    })
    const dialogs = buildDialogs()

    await create(access, dialogs).openDeleteDialog({ permanent: true })

    expect(dialogs.showDeleteConfirmation.mock.calls[0][0]).toMatchObject({ sourceVolumeId: 'vol-stick' })
  })

  it('drops the trash affordance when the rows sit on a volume with no trash', async () => {
    // exFAT and every network mount answer `supportsTrash: false`
    // (`volumes/fs_type.rs`). Offering "Move to trash" there gives the user a
    // button whose operation the backend can only fail.
    getSnapshotSpy.mockReturnValue(
      snapshot([snapshotEntry({ name: 'a.txt', path: '/Volumes/Stick/a.txt', parentPath: '/Volumes/Stick' })]),
    )
    const paneRef = buildPaneRef({ currentPath: 'search-results://sr-1', selectedIndices: [0] })
    const access = buildAccess({
      paneRefs: { left: paneRef },
      volumeIds: { left: 'search-results' },
      volumes: [
        volume({ id: 'root', path: '/' }),
        volume({ id: 'vol-stick', name: 'Stick', path: '/Volumes/Stick', supportsTrash: false }),
      ],
    })
    const dialogs = buildDialogs()

    await create(access, dialogs).openDeleteDialog({ permanent: false })

    expect(dialogs.showDeleteConfirmation.mock.calls[0][0]).toMatchObject({ supportsTrash: false })
  })

  it('keeps root and its trash for rows on the boot volume', async () => {
    getSnapshotSpy.mockReturnValue(
      snapshot([snapshotEntry({ name: 'a.txt', path: '/Users/me/a.txt', parentPath: '/Users/me' })]),
    )
    const paneRef = buildPaneRef({ currentPath: 'search-results://sr-1', selectedIndices: [0] })
    const access = buildAccess({
      paneRefs: { left: paneRef },
      volumeIds: { left: 'search-results' },
      volumes: [
        volume({ id: 'root', path: '/' }),
        volume({ id: 'vol-stick', name: 'Stick', path: '/Volumes/Stick', supportsTrash: false }),
      ],
    })
    const dialogs = buildDialogs()

    await create(access, dialogs).openDeleteDialog({ permanent: false })

    expect(dialogs.showDeleteConfirmation.mock.calls[0][0]).toMatchObject({
      sourceVolumeId: 'root',
      supportsTrash: true,
    })
  })

  it('falls back to root when no registered volume claims the rows', async () => {
    // The honest unknown, the same answer `transfer-entry::resolveSourceVolumeId`
    // gives a drag it cannot place. Optimistic on trash, so an unplaceable row
    // keeps the trash option and lets the backend answer for it, rather than
    // being forced into a permanent delete by a resolution miss.
    getSnapshotSpy.mockReturnValue(
      snapshot([snapshotEntry({ name: 'a.txt', path: '/nowhere/a.txt', parentPath: '/nowhere' })]),
    )
    const paneRef = buildPaneRef({ currentPath: 'search-results://sr-1', selectedIndices: [0] })
    const access = buildAccess({
      paneRefs: { left: paneRef },
      volumeIds: { left: 'search-results' },
      volumes: [volume({ id: 'vol-stick', name: 'Stick', path: '/Volumes/Stick', supportsTrash: false })],
    })
    const dialogs = buildDialogs()

    await create(access, dialogs).openDeleteDialog({ permanent: false })

    expect(dialogs.showDeleteConfirmation.mock.calls[0][0]).toMatchObject({
      sourceVolumeId: 'root',
      supportsTrash: true,
    })
  })

  it('builds the delete dialog from the snapshot cursor entry on a search-results pane', async () => {
    getSnapshotSpy.mockReturnValue(
      snapshot([snapshotEntry({ name: 'hit.md', path: '/real/hit.md', parentPath: '/real' })]),
    )
    const paneRef = buildPaneRef({ currentPath: 'search-results://sr-1', cursorIndex: 0 })
    const access = buildAccess({ paneRefs: { left: paneRef }, volumeIds: { left: 'search-results' } })
    const dialogs = buildDialogs()

    await create(access, dialogs).openDeleteDialog({ permanent: true })

    expect(dialogs.showDeleteConfirmation).toHaveBeenCalledTimes(1)
    expect(dialogs.showDeleteConfirmation.mock.calls[0][0]).toMatchObject({
      sourcePaths: ['/real/hit.md'],
      sourceFolderPath: '/real',
      isPermanent: true,
      supportsTrash: true,
      isFromCursor: true,
      sourceVolumeId: 'root',
    })
  })

  it('deletes every selected row on a search-results pane, not only the cursor row', async () => {
    // ERR-Q373S: Cmd+A then F8 in a search-results pane deleted a single file.
    // The snapshot pane shares `FilePane.selection` with normal panes, so the
    // delete opener has to honour it exactly like the copy/move opener does.
    getSnapshotSpy.mockReturnValue(
      snapshot([
        snapshotEntry({ name: 'a.txt', path: '/real/a.txt', parentPath: '/real', size: 1 }),
        snapshotEntry({ name: 'b.txt', path: '/real/b.txt', parentPath: '/real', size: 2 }),
        snapshotEntry({ name: 'c.txt', path: '/real/c.txt', parentPath: '/real', size: 3 }),
      ]),
    )
    const paneRef = buildPaneRef({ currentPath: 'search-results://sr-1', selectedIndices: [0, 2], cursorIndex: 1 })
    const access = buildAccess({ paneRefs: { left: paneRef }, volumeIds: { left: 'search-results' } })
    const dialogs = buildDialogs()

    await create(access, dialogs).openDeleteDialog({ permanent: false })

    expect(dialogs.showDeleteConfirmation).toHaveBeenCalledTimes(1)
    expect(dialogs.showDeleteConfirmation.mock.calls[0][0]).toMatchObject({
      sourcePaths: ['/real/a.txt', '/real/c.txt'],
      sourceItems: [
        { name: 'a.txt', size: 1, isDirectory: false },
        { name: 'c.txt', size: 3, isDirectory: false },
      ],
      isFromCursor: false,
    })
  })

  it('reports the common parent when a snapshot selection spans several folders', async () => {
    // The dialog's "From <path>" line and the trash toast's volume lookup both
    // read `sourceFolderPath`, so it has to stay a real directory. The common
    // ancestor is the honest one for a result set gathered from everywhere.
    getSnapshotSpy.mockReturnValue(
      snapshot([
        snapshotEntry({ name: 'a.txt', path: '/Users/me/docs/a.txt', parentPath: '/Users/me/docs' }),
        snapshotEntry({ name: 'b.txt', path: '/Users/me/photos/b.txt', parentPath: '/Users/me/photos' }),
      ]),
    )
    const paneRef = buildPaneRef({ currentPath: 'search-results://sr-1', selectedIndices: [0, 1] })
    const access = buildAccess({ paneRefs: { left: paneRef }, volumeIds: { left: 'search-results' } })
    const dialogs = buildDialogs()

    await create(access, dialogs).openDeleteDialog({ permanent: false })

    expect(dialogs.showDeleteConfirmation.mock.calls[0][0]).toMatchObject({
      sourceFolderPath: '/Users/me',
    })
  })

  it('skips a stale selected index on a search-results pane and keeps the rest', async () => {
    getSnapshotSpy.mockReturnValue(
      snapshot([snapshotEntry({ name: 'a.txt', path: '/real/a.txt', parentPath: '/real' })]),
    )
    const paneRef = buildPaneRef({ currentPath: 'search-results://sr-1', selectedIndices: [0, 9] })
    const access = buildAccess({ paneRefs: { left: paneRef }, volumeIds: { left: 'search-results' } })
    const dialogs = buildDialogs()

    await create(access, dialogs).openDeleteDialog({ permanent: false })

    expect(dialogs.showDeleteConfirmation.mock.calls[0][0]).toMatchObject({ sourcePaths: ['/real/a.txt'] })
  })

  it('bails on a search-results pane whose whole selection is stale', async () => {
    getSnapshotSpy.mockReturnValue(snapshot([snapshotEntry()]))
    const paneRef = buildPaneRef({ currentPath: 'search-results://sr-1', selectedIndices: [7, 9] })
    const access = buildAccess({ paneRefs: { left: paneRef }, volumeIds: { left: 'search-results' } })
    const dialogs = buildDialogs()

    await create(access, dialogs).openDeleteDialog({ permanent: false })

    expect(dialogs.showDeleteConfirmation).not.toHaveBeenCalled()
  })

  it('bails on a search-results pane whose cursor is out of range', async () => {
    getSnapshotSpy.mockReturnValue(snapshot([snapshotEntry()]))
    const paneRef = buildPaneRef({ currentPath: 'search-results://sr-1', cursorIndex: 9 })
    const access = buildAccess({ paneRefs: { left: paneRef }, volumeIds: { left: 'search-results' } })
    const dialogs = buildDialogs()

    await create(access, dialogs).openDeleteDialog({ permanent: false })

    expect(dialogs.showDeleteConfirmation).not.toHaveBeenCalled()
  })

  it('does nothing on a snapshot with no rows at all', async () => {
    // A search that found nothing, or one whose every row has since been purged.
    // Cmd+A leaves the selection empty and the cursor sits at 0 over nothing, so
    // F8 must be a quiet no-op rather than a dialog over an empty list.
    getSnapshotSpy.mockReturnValue(snapshot([]))
    const paneRef = buildPaneRef({ currentPath: 'search-results://sr-1', selectedIndices: [], cursorIndex: 0 })
    const access = buildAccess({ paneRefs: { left: paneRef }, volumeIds: { left: 'search-results' } })
    const dialogs = buildDialogs()

    await create(access, dialogs).openDeleteDialog({ permanent: false })

    expect(dialogs.showDeleteConfirmation).not.toHaveBeenCalled()
    expect(dialogs.showAlert).not.toHaveBeenCalled()
  })

  it('carries a path with a newline or unicode in it through untouched', async () => {
    // These reach the backend as an array of strings, never a newline-joined
    // blob, so nothing has to escape them. The common-parent walk splits on `/`
    // and must not trip over either.
    const odd = '/Users/me/dossier\nnote/\u00e9t\u00e9 \u2014 r\u00e9sum\u00e9.txt'
    getSnapshotSpy.mockReturnValue(
      snapshot([snapshotEntry({ name: 'x.txt', path: odd, parentPath: '/Users/me/dossier\nnote' })]),
    )
    const paneRef = buildPaneRef({ currentPath: 'search-results://sr-1', selectedIndices: [0] })
    const access = buildAccess({ paneRefs: { left: paneRef }, volumeIds: { left: 'search-results' } })
    const dialogs = buildDialogs()

    await create(access, dialogs).openDeleteDialog({ permanent: false })

    expect(dialogs.showDeleteConfirmation.mock.calls[0][0]).toMatchObject({
      sourcePaths: [odd],
      sourceFolderPath: '/Users/me/dossier\nnote',
    })
  })

  it('bails on a search-results pane whose snapshot is missing', async () => {
    getSnapshotSpy.mockReturnValue(undefined)
    const paneRef = buildPaneRef({ currentPath: 'search-results://sr-1', cursorIndex: 0 })
    const access = buildAccess({ paneRefs: { left: paneRef }, volumeIds: { left: 'search-results' } })
    const dialogs = buildDialogs()

    await create(access, dialogs).openDeleteDialog({ permanent: false })

    expect(dialogs.showDeleteConfirmation).not.toHaveBeenCalled()
  })
})
