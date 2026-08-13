/**
 * The pane work a settled transfer runs, on its own.
 *
 * This is the module an adopted view is deliberately built without, so it is
 * worth pinning what it does when it IS wired up: the fresh-versus-stale rule
 * that decides whether a selection may be touched, and the per-operation-type
 * refresh fan-out. Feeding it a birth-props getter and two fake panes is the
 * whole setup — no dialog, no Svelte component.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import { createTransferPaneEffects } from './transfer-pane-effects'
import { refreshListing } from '$lib/tauri-commands'
import type { FilePaneAPI } from './types'
import type { TransferProgressPropsData } from './dialog-props'

vi.mock('$lib/tauri-commands', () => ({ refreshListing: vi.fn(() => Promise.resolve()) }))

const BIRTH_FOLDER = '/Users/me/photos'

function makePaneRef(currentPath: string, listingId: string, snapshot: string[] | 'all' | null = null) {
  const spies = {
    clearSelection: vi.fn(),
    selectAll: vi.fn(),
    snapshotSelectionForOperation: vi.fn(() => Promise.resolve()),
    clearOperationSnapshot: vi.fn(() => snapshot),
    getListingId: vi.fn(() => listingId),
    getCurrentPath: vi.fn(() => currentPath),
    refreshVolumeSpace: vi.fn(() => Promise.resolve()),
  }
  return { ref: spies as unknown as FilePaneAPI, spies }
}

/** A move the RIGHT pane started, going left. */
function moveProps(): TransferProgressPropsData {
  return {
    operationType: 'move',
    sourcePaths: [`${BIRTH_FOLDER}/a.jpg`],
    sourceFolderPath: BIRTH_FOLDER,
    sourcePaneSide: 'right',
    destinationPath: '/Users/me/backup',
    direction: 'left',
    sortColumn: 'name',
    sortOrder: 'ascending',
    previewId: null,
    sourceVolumeId: 'root',
  }
}

function makeEffects(sourcePaneAt: string, snapshot: string[] | 'all' | null = null) {
  const right = makePaneRef(sourcePaneAt, 'listing-right', snapshot)
  const left = makePaneRef('/Users/me/backup', 'listing-left')
  let props: TransferProgressPropsData | null = moveProps()
  const effects = createTransferPaneEffects(
    { getLeftPaneRef: () => left.ref, getRightPaneRef: () => right.ref },
    () => props,
  )
  return { effects, right, left, settle: () => (props = null) }
}

beforeEach(() => {
  vi.clearAllMocks()
})

describe('the source pane still shows the folder the operation was born in', () => {
  it('drops the snapshot and the selection when the transfer settles', () => {
    const { effects, right } = makeEffects(BIRTH_FOLDER)

    effects.clearSourcePaneAfterTransfer()

    expect(right.spies.clearOperationSnapshot).toHaveBeenCalled()
    expect(right.spies.clearSelection).toHaveBeenCalled()
  })

  it('re-selects the survivors after a cancelled move that had selected everything', () => {
    const { effects, right } = makeEffects(BIRTH_FOLDER, 'all')

    effects.adjustSelectionAfterCancel('move')

    expect(right.spies.selectAll).toHaveBeenCalled()
  })

  it('leaves a cancelled COPY alone: the source listing never changed', () => {
    const { effects, right } = makeEffects(BIRTH_FOLDER, 'all')

    effects.adjustSelectionAfterCancel('copy')

    expect(right.spies.selectAll).not.toHaveBeenCalled()
    expect(right.spies.clearSelection).not.toHaveBeenCalled()
  })
})

describe('the source pane has navigated since', () => {
  // The selection there is one the user made somewhere else, and this operation
  // has no business clearing or restoring it.
  it('touches no selection when the transfer settles', () => {
    const { effects, right } = makeEffects('/Users/me/somewhere-else')

    effects.clearSourcePaneAfterTransfer()
    effects.adjustSelectionAfterCancel('move')

    expect(right.spies.clearOperationSnapshot).not.toHaveBeenCalled()
    expect(right.spies.clearSelection).not.toHaveBeenCalled()
    expect(right.spies.selectAll).not.toHaveBeenCalled()
  })
})

describe('refreshing panes after a transfer', () => {
  it('re-reads both listings for a move, since the source lost rows too', () => {
    const { effects } = makeEffects(BIRTH_FOLDER)

    effects.refreshPanesAfterTransfer()

    expect(refreshListing).toHaveBeenCalledWith('listing-left')
    expect(refreshListing).toHaveBeenCalledWith('listing-right')
  })

  it('is inert once the operation has settled and the slot is empty', () => {
    const { effects, right, settle } = makeEffects(BIRTH_FOLDER)
    settle()

    effects.clearSourcePaneAfterTransfer()

    expect(right.spies.clearSelection).not.toHaveBeenCalled()
  })
})
