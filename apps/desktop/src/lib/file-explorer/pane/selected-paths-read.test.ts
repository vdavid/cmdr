import { describe, it, expect, vi, beforeEach } from 'vitest'
import type { PaneAccess } from './pane-access'
import type { FilePaneAPI } from './types'

const { getSelectionSnapshotSpy, resolveSnapshotPathsSpy } = vi.hoisted(() => ({
  getSelectionSnapshotSpy:
    vi.fn<
      (
        listingId: string,
        includeHidden: boolean,
        indices: number[],
        expectedSequence: number,
      ) => Promise<{ paths: string[]; fileCount: number; folderCount: number }>
    >(),
  resolveSnapshotPathsSpy: vi.fn<(snapshotId: string, selected: number[], cursor: number) => string[]>(),
}))

vi.mock('$lib/tauri-commands', () => ({ getSelectionSnapshot: getSelectionSnapshotSpy }))
vi.mock('$lib/search/snapshot-store.svelte', () => ({
  resolveSnapshotPaths: resolveSnapshotPathsSpy,
  snapshotIdFromPanePath: (path: string) =>
    path.startsWith('search-results://') ? path.slice('search-results://'.length) : null,
}))
// Empty store: a real id ('root') classifies as the listable `local` kind.
vi.mock('$lib/stores/volume-store.svelte', () => ({ getVolumes: () => [] }))

import { readSelectedPathsForCopy } from './selected-paths-read'

interface PaneStubConfig {
  selectedIndices?: number[]
  hasParent?: boolean
  listingId?: string
  sequence?: number
  rowStateReady?: boolean
  currentPath?: string
}

function paneStub(config: PaneStubConfig = {}): FilePaneAPI {
  return {
    getSelectedIndices: () => config.selectedIndices ?? [],
    hasParentEntry: () => config.hasParent ?? false,
    getListingId: () => config.listingId ?? 'listing-1',
    getLastSequence: () => config.sequence ?? 7,
    isRowStateReady: () => config.rowStateReady ?? true,
    getCurrentPath: () => config.currentPath ?? '/Users/x/dir',
  } as unknown as FilePaneAPI
}

function access(pane: FilePaneAPI | undefined, volumeId = 'root', showHidden = true): PaneAccess {
  return {
    getPaneRef: () => pane,
    getPanePath: () => '/Users/x/dir',
    getPaneVolumeId: () => volumeId,
    getFocusedPane: () => 'left',
    getShowHiddenFiles: () => showHidden,
  } as unknown as PaneAccess
}

beforeEach(() => {
  vi.clearAllMocks()
})

describe('readSelectedPathsForCopy', () => {
  it('reports no selection when nothing is selected, without any IPC', async () => {
    const read = await readSelectedPathsForCopy(access(paneStub()))
    expect(read).toEqual({ kind: 'noSelection' })
    expect(getSelectionSnapshotSpy).not.toHaveBeenCalled()
  })

  it('reports no selection when the focused pane is missing', async () => {
    expect(await readSelectedPathsForCopy(access(undefined))).toEqual({ kind: 'noSelection' })
  })

  it('resolves every selected row through the revision-checked snapshot, in pane order', async () => {
    getSelectionSnapshotSpy.mockResolvedValue({ paths: ['/d/a', '/d/b', '/d/c'], fileCount: 3, folderCount: 0 })
    const pane = paneStub({ selectedIndices: [1, 3, 4], hasParent: true, sequence: 42 })
    const read = await readSelectedPathsForCopy(access(pane, 'root', false))
    expect(read).toEqual({ kind: 'paths', paths: ['/d/a', '/d/b', '/d/c'] })
    // Frontend indices shifted past the `..` row, and the pane's sequence pinned.
    expect(getSelectionSnapshotSpy).toHaveBeenCalledExactlyOnceWith('listing-1', false, [0, 2, 3], 42)
  })

  it('reads the selection synchronously, before the IPC resolves', async () => {
    let selected = [0, 1]
    const pane: FilePaneAPI = { ...paneStub(), getSelectedIndices: () => selected }
    getSelectionSnapshotSpy.mockResolvedValue({ paths: ['/d/a', '/d/b'], fileCount: 2, folderCount: 0 })
    const pending = readSelectedPathsForCopy(access(pane))
    selected = [5]
    await pending
    expect(getSelectionSnapshotSpy).toHaveBeenCalledWith('listing-1', true, [0, 1], 7)
  })

  it('refuses rather than reinterpret indices when the listing moved on', async () => {
    getSelectionSnapshotSpy.mockRejectedValue(Object.assign(new Error('x'), { failure: { type: 'changed' } }))
    const read = await readSelectedPathsForCopy(access(paneStub({ selectedIndices: [0] })))
    expect(read).toEqual({ kind: 'changed' })
  })

  it('refuses when the listing is gone', async () => {
    getSelectionSnapshotSpy.mockRejectedValue(Object.assign(new Error('x'), { failure: { type: 'gone' } }))
    expect(await readSelectedPathsForCopy(access(paneStub({ selectedIndices: [0] })))).toEqual({ kind: 'changed' })
  })

  it('refuses while the row state is still settling, without any IPC', async () => {
    const read = await readSelectedPathsForCopy(access(paneStub({ selectedIndices: [0], rowStateReady: false })))
    expect(read).toEqual({ kind: 'changed' })
    expect(getSelectionSnapshotSpy).not.toHaveBeenCalled()
  })

  it('rethrows an unexpected failure instead of swallowing it', async () => {
    getSelectionSnapshotSpy.mockRejectedValue(new Error('boom'))
    await expect(readSelectedPathsForCopy(access(paneStub({ selectedIndices: [0] })))).rejects.toThrow('boom')
  })

  it('resolves a search-results selection from the snapshot, not a listing', async () => {
    resolveSnapshotPathsSpy.mockReturnValue(['/a/one.md', '/b/two.md'])
    const pane = paneStub({ selectedIndices: [0, 2], currentPath: 'search-results://sr-3' })
    const read = await readSelectedPathsForCopy(access(pane, 'search-results'))
    expect(read).toEqual({ kind: 'paths', paths: ['/a/one.md', '/b/two.md'] })
    expect(resolveSnapshotPathsSpy).toHaveBeenCalledWith('sr-3', [0, 2], -1)
    expect(getSelectionSnapshotSpy).not.toHaveBeenCalled()
  })

  it('reports no selection on a pane with no listing and no snapshot (the Servers hub)', async () => {
    const read = await readSelectedPathsForCopy(access(paneStub({ selectedIndices: [0] }), 'network'))
    expect(read).toEqual({ kind: 'noSelection' })
    expect(getSelectionSnapshotSpy).not.toHaveBeenCalled()
  })
})
