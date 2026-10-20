/**
 * "Ask about selection" reads the WHOLE selection from the backend listing, not the
 * pane's loaded window: rows far past it attach too, a capped selection says how many
 * it left out, nothing selected falls back to the cursor row, and rows that never
 * settle attach nothing (with the toast). A search-results pane reads its snapshot.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest'
import type { PaneAccess } from './pane-access'
import type { FilePaneAPI } from './types'
import type { FileEntry } from '../types'

const { getSelectionEntriesSpy, resolveSnapshotEntriesSpy, announceSpy } = vi.hoisted(() => ({
  getSelectionEntriesSpy:
    vi.fn<
      (listingId: string, includeHidden: boolean, indices: number[], expectedSequence: number) => Promise<FileEntry[]>
    >(),
  resolveSnapshotEntriesSpy:
    vi.fn<(id: string, selected: number[], cursor: number) => { path: string; isDirectory: boolean }[]>(),
  announceSpy: vi.fn(),
}))

vi.mock('#lib/tauri-commands/index.js', async () => ({
  getSelectionEntries: getSelectionEntriesSpy,
  asListingLookupError: (
    await vi.importActual<typeof import('#lib/tauri-commands/listing-gone.js')>('#lib/tauri-commands/listing-gone.js')
  ).asListingLookupError,
}))
vi.mock('#lib/search/snapshot-store.svelte.js', () => ({
  resolveSnapshotEntries: resolveSnapshotEntriesSpy,
  snapshotIdFromPanePath: (path: string) =>
    path.startsWith('search-results://') ? path.slice('search-results://'.length) : null,
}))
vi.mock('./operation-start-gate', () => ({ announceRowsStillSettling: announceSpy }))
// Empty store: a real id ('root') classifies as the listable `local` kind.
vi.mock('#lib/stores/volume-store.svelte.js', () => ({ getVolumes: () => [] }))

import { readSelectionAttachments, SELECTION_ATTACHMENT_CAP } from './selection-attachments-read'
import { ROW_SETTLE_BUDGET_MS } from './pressed-rows'

/** A backend listing of `count` rows, `row<n>`, every fifth a folder. */
function backendRows(indices: number[]): Promise<FileEntry[]> {
  return Promise.resolve(
    indices.map(
      (i) => ({ name: `row${String(i)}`, path: `/d/row${String(i)}`, isDirectory: i % 5 === 0 }) as FileEntry,
    ),
  )
}

function paneStub(config: { selected?: number[]; cursor?: number; hasParent?: boolean; path?: string } = {}) {
  return {
    getSelectedIndices: () => config.selected ?? [],
    getCursorIndex: () => config.cursor ?? 0,
    hasParentEntry: () => config.hasParent ?? false,
    getListingId: () => 'listing-1',
    getLastSequence: () => 7,
    isRowStateReady: () => true,
    getCurrentPath: () => config.path ?? '/d',
  } as unknown as FilePaneAPI
}

function access(pane: FilePaneAPI | undefined, volumeId = 'root'): PaneAccess {
  return {
    getPaneRef: () => pane,
    getPaneVolumeId: () => volumeId,
    getFocusedPane: () => 'left',
    getShowHiddenFiles: () => true,
  } as unknown as PaneAccess
}

beforeEach(() => {
  vi.clearAllMocks()
  getSelectionEntriesSpy.mockImplementation((_id, _hidden, indices) => backendRows(indices))
})

describe('readSelectionAttachments', () => {
  it('attaches every selected row, however far past the loaded window, with its kind', async () => {
    const read = await readSelectionAttachments(access(paneStub({ selected: [4_999, 3, 500] })))

    expect(read).toEqual({
      kind: 'refs',
      refs: [
        { path: '/d/row3', kind: 'file' },
        { path: '/d/row500', kind: 'folder' },
        { path: '/d/row4999', kind: 'file' },
      ],
      leftOut: 0,
    })
    expect(getSelectionEntriesSpy).toHaveBeenCalledWith('listing-1', true, [3, 500, 4_999], 7)
  })

  it('caps a huge selection at its first rows in pane order, and counts the rest', async () => {
    const selected = Array.from({ length: SELECTION_ATTACHMENT_CAP + 25 }, (_, i) => SELECTION_ATTACHMENT_CAP + 24 - i)

    const read = await readSelectionAttachments(access(paneStub({ selected })))

    if (read.kind !== 'refs') throw new Error(`expected refs, got ${read.kind}`)
    expect(read.refs).toHaveLength(SELECTION_ATTACHMENT_CAP)
    expect(read.refs[0].path).toBe('/d/row0')
    expect(read.leftOut).toBe(25)
  })

  it('falls back to the cursor row with nothing selected, and to nothing on `..`', async () => {
    expect(await readSelectionAttachments(access(paneStub({ cursor: 3, hasParent: true })))).toEqual({
      kind: 'refs',
      refs: [{ path: '/d/row2', kind: 'file' }],
      leftOut: 0,
    })
    expect(await readSelectionAttachments(access(paneStub({ cursor: 0, hasParent: true })))).toEqual({
      kind: 'refs',
      refs: [],
      leftOut: 0,
    })
  })

  it('attaches nothing and says why when the rows never settle', async () => {
    vi.useFakeTimers()
    try {
      const { ListingLookupFailure } = await import('#lib/tauri-commands/listing-gone.js')
      getSelectionEntriesSpy.mockRejectedValue(new ListingLookupFailure({ type: 'changed', listingId: 'listing-1' }))

      const pending = readSelectionAttachments(access(paneStub({ selected: [1] })))
      await vi.advanceTimersByTimeAsync(ROW_SETTLE_BUDGET_MS + 200)

      expect(await pending).toEqual({ kind: 'changed' })
      expect(announceSpy).toHaveBeenCalledTimes(1)
    } finally {
      vi.useRealTimers()
    }
  })

  it('reads a search-results pane from its snapshot, without a listing read', async () => {
    resolveSnapshotEntriesSpy.mockReturnValue([
      { path: '/far/a.txt', isDirectory: false },
      { path: '/far/b', isDirectory: true },
    ])

    const read = await readSelectionAttachments(
      access(paneStub({ selected: [1, 0], path: 'search-results://sr-1' }), 'search-results'),
    )

    expect(read).toEqual({
      kind: 'refs',
      refs: [
        { path: '/far/a.txt', kind: 'file' },
        { path: '/far/b', kind: 'folder' },
      ],
      leftOut: 0,
    })
    expect(resolveSnapshotEntriesSpy).toHaveBeenCalledWith('sr-1', [0, 1], 0)
    expect(getSelectionEntriesSpy).not.toHaveBeenCalled()
  })

  it('attaches nothing without a focused pane', async () => {
    expect(await readSelectionAttachments(access(undefined))).toEqual({ kind: 'refs', refs: [], leftOut: 0 })
  })
})
