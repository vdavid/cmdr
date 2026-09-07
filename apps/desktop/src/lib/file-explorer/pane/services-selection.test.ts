import { describe, expect, it } from 'vitest'
import { servicesSelectionForPane } from './services-selection'

const listingRows = { kind: 'listing', listingId: 'listing-1', includeHidden: false, hasParent: true } as const
const SNAPSHOT_PATHS = ['/a/one.txt', '/b/two.txt', '/c/three.txt']
const snapshotRows = { kind: 'snapshot', pathAt: (index: number) => SNAPSHOT_PATHS[index] } as const

describe('servicesSelectionForPane', () => {
  it('sends the selection as listing indices, never as resolved paths', () => {
    // The whole point of the indices shape: a select-all over a 500k folder is
    // four numbers here, and the backend reads the paths only when a service asks.
    expect(
      servicesSelectionForPane({
        rowsAreOsVisible: true,
        cursorPath: '/dir/cursor.txt',
        selectedIndices: [3, 7],
        rows: listingRows,
      }),
    ).toEqual({
      cursorPath: '/dir/cursor.txt',
      rows: { kind: 'listing', listingId: 'listing-1', indices: [3, 7], includeHidden: false, hasParent: true },
    })
  })

  it('falls back to the cursor row when nothing is selected', () => {
    // Finder's rule, the same one `snapshotContextMenuPaths` follows.
    expect(
      servicesSelectionForPane({
        rowsAreOsVisible: true,
        cursorPath: '/dir/cursor.txt',
        selectedIndices: [],
        rows: listingRows,
      }),
    ).toEqual({ cursorPath: '/dir/cursor.txt', rows: null })
  })

  it('offers nothing at all from a pane whose rows have no file behind them', () => {
    // A phone, an archive's insides, the host list. Handing those to a service
    // would produce file URLs pointing at nothing, so the Services menu has to
    // look exactly as it did before this feature.
    expect(
      servicesSelectionForPane({
        rowsAreOsVisible: false,
        cursorPath: '/mtp/DCIM/photo.jpg',
        selectedIndices: [1, 2],
        rows: listingRows,
      }),
    ).toEqual({ cursorPath: '', rows: null })
  })

  it('carries a snapshot pane by value, since it has no listing to index into', () => {
    expect(
      servicesSelectionForPane({
        rowsAreOsVisible: true,
        cursorPath: '/a/one.txt',
        selectedIndices: [0, 2],
        rows: snapshotRows,
      }),
    ).toEqual({
      cursorPath: '/a/one.txt',
      rows: { kind: 'paths', paths: ['/a/one.txt', '/c/three.txt'] },
    })
  })

  it('drops a snapshot index the rows no longer reach', () => {
    // A selection left over from a longer result set must not smuggle
    // `undefined` into the payload, where it would become the string "undefined".
    expect(
      servicesSelectionForPane({
        rowsAreOsVisible: true,
        cursorPath: '/a/one.txt',
        selectedIndices: [1, 99],
        rows: snapshotRows,
      }),
    ).toEqual({ cursorPath: '/a/one.txt', rows: { kind: 'paths', paths: ['/b/two.txt'] } })
  })

  it('treats a pane with no listing id yet as having no selection', () => {
    // Mid-navigation the pane holds indices but nothing to resolve them against.
    expect(
      servicesSelectionForPane({
        rowsAreOsVisible: true,
        cursorPath: '/dir/cursor.txt',
        selectedIndices: [1],
        rows: { kind: 'listing', listingId: '', includeHidden: false, hasParent: false },
      }),
    ).toEqual({ cursorPath: '/dir/cursor.txt', rows: null })
  })

  it('reports an empty cursor path when the cursor has no row to offer', () => {
    expect(
      servicesSelectionForPane({
        rowsAreOsVisible: true,
        cursorPath: null,
        selectedIndices: [],
        rows: listingRows,
      }),
    ).toEqual({ cursorPath: '', rows: null })
  })
})
