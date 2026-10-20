/**
 * The press-reading rules the callers' specs don't each repeat: a `gone` listing
 * refuses at once, a press that changes shape while it waits is refused, and an
 * error that isn't a listing refusal is the caller's. The waiting itself, and the
 * re-read after the pane applies a diff, are pinned at every caller over
 * `moved-listing.test-fixtures.ts`.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest'

const { announceSpy } = vi.hoisted(() => ({ announceSpy: vi.fn() }))
vi.mock('./operation-start-gate', () => ({ announceRowsStillSettling: announceSpy }))
vi.mock('#lib/tauri-commands/index.js', async () => ({
  asListingLookupError: (
    await vi.importActual<typeof import('#lib/tauri-commands/listing-gone.js')>('#lib/tauri-commands/listing-gone.js')
  ).asListingLookupError,
}))

import { readCursorRow, readPressedRows, type PressedPane } from './pressed-rows'
import { ListingLookupFailure } from '#lib/tauri-commands/listing-gone.js'

function paneOf(state: { selected: number[]; cursor?: number }): PressedPane {
  return {
    getListingId: () => 'listing-1',
    getSelectedIndices: () => state.selected,
    getCursorIndex: () => state.cursor ?? 0,
    hasParentEntry: () => false,
    getLastSequence: () => 0,
  }
}

const changed = () => Promise.reject(new ListingLookupFailure({ type: 'changed', listingId: 'listing-1' }))

beforeEach(() => {
  announceSpy.mockClear()
})

describe('readPressedRows', () => {
  it('refuses a gone listing at once, without waiting', async () => {
    const read = vi.fn(() => Promise.reject(new ListingLookupFailure({ type: 'gone', listingId: 'listing-1' })))

    expect(await readPressedRows(paneOf({ selected: [1] }), read, { cursorFallback: true, mcpRequestId: 'r' })).toEqual(
      {
        kind: 'refused',
      },
    )
    expect(read).toHaveBeenCalledTimes(1)
    expect(announceSpy).toHaveBeenCalledWith('r')
  })

  it('refuses a cursor press under which a selection appeared while it waited', async () => {
    const state = { selected: [] as number[], cursor: 3 }
    const read = vi.fn(() => {
      state.selected = [1]
      return changed()
    })

    expect(await readPressedRows(paneOf(state), read, { cursorFallback: true })).toEqual({ kind: 'refused' })
    expect(read).toHaveBeenCalledTimes(1)
  })

  it('stays quiet when the caller words its own refusal', async () => {
    const read = vi.fn(() => Promise.reject(new ListingLookupFailure({ type: 'gone', listingId: 'listing-1' })))

    await readPressedRows(paneOf({ selected: [1] }), read, { cursorFallback: false, announce: false })

    expect(announceSpy).not.toHaveBeenCalled()
  })

  it('hands an error that is not a listing refusal to the caller', async () => {
    const read = () => Promise.reject(new Error('ipc down'))

    await expect(readPressedRows(paneOf({ selected: [1] }), read, { cursorFallback: false })).rejects.toThrow(
      'ipc down',
    )
  })

  it('answers none with nothing selected and no cursor fallback, without reading', async () => {
    const read = vi.fn()

    expect(await readPressedRows(paneOf({ selected: [] }), read, { cursorFallback: false })).toEqual({ kind: 'none' })
    expect(read).not.toHaveBeenCalled()
  })
})

describe('readCursorRow', () => {
  it('reads the cursor row alone, whatever is selected', async () => {
    const read = vi.fn(({ backendRows }: { backendRows: number[] }) => Promise.resolve(backendRows))

    expect(await readCursorRow(paneOf({ selected: [1, 2], cursor: 4 }), read)).toEqual({
      kind: 'read',
      value: [4],
      onSelection: false,
    })
  })

  it('answers none on the `..` row, without reading', async () => {
    const read = vi.fn()
    const pane = { ...paneOf({ selected: [], cursor: 0 }), hasParentEntry: () => true }

    expect(await readCursorRow(pane, read)).toEqual({ kind: 'none' })
    expect(read).not.toHaveBeenCalled()
  })

  it('refuses once the pane switched listings while it waited', async () => {
    let listingId = 'listing-1'
    const pane = { ...paneOf({ selected: [] }), getListingId: () => listingId }
    const read = vi.fn(() => {
      listingId = 'listing-2'
      return changed()
    })

    expect(await readCursorRow(pane, read, { mcpRequestId: 'r' })).toEqual({ kind: 'refused' })
    expect(read).toHaveBeenCalledTimes(1)
    expect(announceSpy).toHaveBeenCalledWith('r')
  })
})
