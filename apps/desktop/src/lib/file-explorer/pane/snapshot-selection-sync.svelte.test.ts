/**
 * The snapshot pane's answer to a shrinking result set.
 *
 * A search-results pane has no backend listing, so the `directory-diff` /
 * `findFileIndices` machinery that keeps a normal pane's index-based selection
 * honest never runs for it. Without this module the selection kept the indices
 * the user clicked while the rows underneath them moved up, which is a
 * delete acting on files nobody picked. Tests here drive the real store and a
 * real `createSelectionState`, so the reproduction is the production shape.
 */
import { describe, it, expect, beforeEach, afterEach } from 'vitest'
import { flushSync } from 'svelte'
import { createSelectionState } from './selection-state.svelte'
import { createSnapshotSelectionSync, remapSnapshotSelection } from './snapshot-selection-sync.svelte'
import {
  _resetForTesting,
  getMutationTick,
  getOrCreate,
  getSnapshot,
  removeEntryFromAllSnapshots,
  appendSnapshotEntries,
  type SearchSnapshot,
} from '$lib/search/snapshot-store.svelte'
import type { SearchResultEntry } from '$lib/ipc/bindings'

function makeEntry(name: string): SearchResultEntry {
  return {
    name,
    path: `/Users/test/${name}`,
    parentPath: '/Users/test',
    isDirectory: false,
    size: 1,
    modifiedAt: null,
    iconId: 'ext:txt',
  }
}

function makeSnapshot(id: string, names: string[]): SearchSnapshot {
  return {
    id,
    query: 'q',
    mode: 'filename',
    filters: {},
    scope: '',
    caseSensitive: false,
    excludeSystemDirs: true,
    entries: names.map(makeEntry),
    totalCount: names.length,
    createdAt: 0,
    label: 'q',
  }
}

describe('remapSnapshotSelection', () => {
  it('follows the selected rows to their new positions', () => {
    expect(
      remapSnapshotSelection({
        previousPaths: ['/a', '/b', '/c', '/d'],
        currentPaths: ['/b', '/d'],
        cursorIndex: 1,
        selectedIndices: [1, 3],
      }),
    ).toEqual({ cursorIndex: 0, selectedIndices: [0, 1] })
  })

  it('drops the rows that are gone, which empties a selection the delete took whole', () => {
    expect(
      remapSnapshotSelection({
        previousPaths: ['/a', '/b', '/c'],
        currentPaths: ['/a'],
        cursorIndex: 1,
        selectedIndices: [1, 2],
      }).selectedIndices,
    ).toEqual([])
  })

  it('slides the cursor to whatever took its place when its own row is gone', () => {
    // Same rule `listing-diff-sync::reconcileCursorAndSelection` applies on a
    // normal pane: a cursor whose row vanished stays where it is on screen.
    expect(
      remapSnapshotSelection({
        previousPaths: ['/a', '/b', '/c', '/d', '/e'],
        currentPaths: ['/a', '/b', '/e'],
        cursorIndex: 2,
        selectedIndices: [],
      }).cursorIndex,
    ).toBe(2)
  })

  it('clamps a cursor that the shrunk list can no longer hold', () => {
    expect(
      remapSnapshotSelection({
        previousPaths: ['/a', '/b', '/c'],
        currentPaths: ['/a'],
        cursorIndex: 2,
        selectedIndices: [],
      }).cursorIndex,
    ).toBe(0)
  })

  it('lands the cursor on 0 for an emptied list rather than -1', () => {
    expect(
      remapSnapshotSelection({
        previousPaths: ['/a'],
        currentPaths: [],
        cursorIndex: 0,
        selectedIndices: [0],
      }),
    ).toEqual({ cursorIndex: 0, selectedIndices: [] })
  })

  it('leaves everything alone when rows are only appended, as a running walk does', () => {
    expect(
      remapSnapshotSelection({
        previousPaths: ['/a', '/b'],
        currentPaths: ['/a', '/b', '/c'],
        cursorIndex: 1,
        selectedIndices: [0, 1],
      }),
    ).toEqual({ cursorIndex: 1, selectedIndices: [0, 1] })
  })
})

describe('createSnapshotSelectionSync', () => {
  let dispose: (() => void) | undefined

  beforeEach(() => {
    _resetForTesting()
  })

  afterEach(() => {
    dispose?.()
    dispose = undefined
  })

  /** Wires the sync to a real selection state over the stored snapshot `id`. */
  function wire(id: string | null) {
    const selection = createSelectionState()
    let cursorIndex = $state(0)
    let paneId = $state(id)
    dispose = $effect.root(() => {
      createSnapshotSelectionSync({
        getSnapshotEntries: () => {
          void getMutationTick()
          return paneId === null ? undefined : getSnapshot(paneId)?.entries
        },
        getCursorIndex: () => cursorIndex,
        getSelectedIndices: () => selection.getSelectedIndices(),
        setSelectedIndices: (indices: number[]) => {
          selection.setSelectedIndices(indices)
        },
        applyCursorIndex: (index: number) => {
          cursorIndex = index
        },
      })
    })
    flushSync()
    return {
      selection,
      getCursor: () => cursorIndex,
      setCursor: (index: number) => {
        cursorIndex = index
        flushSync()
      },
      leaveSnapshotPane: () => {
        paneId = null
        flushSync()
      },
    }
  }

  it('drops the deleted rows from the selection instead of leaving it on their neighbours', () => {
    // The user selects rows 2 and 3 of 5 and deletes them. Left alone, the
    // selection still reads {2, 3}, which now names the fifth row and nothing.
    // A second F8 would delete a file the user never picked.
    getOrCreate('sr-1', makeSnapshot('sr-1', ['a.txt', 'b.txt', 'c.txt', 'd.txt', 'e.txt']))
    const pane = wire('sr-1')
    pane.selection.setSelectedIndices([2, 3])
    pane.setCursor(2)

    removeEntryFromAllSnapshots('/Users/test/c.txt')
    removeEntryFromAllSnapshots('/Users/test/d.txt')
    flushSync()

    expect(pane.selection.getSelectedIndices()).toEqual([])
    expect(pane.getCursor()).toBe(2)
  })

  it('keeps a survivor selected at its new index when only part of the selection went', () => {
    getOrCreate('sr-2', makeSnapshot('sr-2', ['a.txt', 'b.txt', 'c.txt']))
    const pane = wire('sr-2')
    pane.selection.setSelectedIndices([0, 2])

    removeEntryFromAllSnapshots('/Users/test/a.txt')
    flushSync()

    expect(pane.selection.getSelectedIndices()).toEqual([1])
  })

  it("leaves a PARTIAL delete's survivors selected, unlike a normal pane, which clears outright", () => {
    // Decision, not an oversight. A normal pane's `clearSourcePaneAfterTransfer`
    // clears indices that stopped describing anything; here the remap has already
    // dropped every row the operation took, so what is left is exactly the rows
    // the user picked that are STILL THERE (a permission-denied one, say). That
    // is a retry, and it can never act on a file nobody chose. The birth-folder
    // gate can't reach this pane anyway (`search-results://<id>` is never an
    // operation's `sourceFolderPath`); ❌ don't widen it to match.
    getOrCreate('sr-partial', makeSnapshot('sr-partial', ['a.txt', 'b.txt', 'c.txt', 'd.txt']))
    const pane = wire('sr-partial')
    pane.selection.setSelectedIndices([0, 1, 2])
    pane.setCursor(0)

    // The delete took a.txt and b.txt; c.txt was refused and keeps its row.
    removeEntryFromAllSnapshots('/Users/test/a.txt')
    removeEntryFromAllSnapshots('/Users/test/b.txt')
    flushSync()

    expect(pane.selection.getSelectedIndices()).toEqual([0])
  })

  it('clamps the cursor into the list when the last row is purged out from under it', () => {
    getOrCreate('sr-last', makeSnapshot('sr-last', ['a.txt', 'b.txt', 'c.txt']))
    const pane = wire('sr-last')
    pane.setCursor(2)

    removeEntryFromAllSnapshots('/Users/test/c.txt')
    flushSync()

    expect(pane.getCursor()).toBe(1)
  })

  it('leaves nothing selected when a purge takes both rows of a duplicated path', () => {
    // The live walk dedups against the indexed half, so two rows for one file is
    // a race the store defends against rather than a normal state. The purge
    // filters by path, so it takes both; the remap must not leave either index
    // behind pointing at some other file.
    getOrCreate('sr-dup', makeSnapshot('sr-dup', ['a.txt', 'a.txt', 'b.txt']))
    const pane = wire('sr-dup')
    pane.selection.setSelectedIndices([0, 1, 2])

    removeEntryFromAllSnapshots('/Users/test/a.txt')
    flushSync()

    expect(getSnapshot('sr-dup')?.entries).toHaveLength(1)
    expect(pane.selection.getSelectedIndices()).toEqual([0])
  })

  it('leaves a selection alone while a running walk appends rows', () => {
    getOrCreate('sr-3', makeSnapshot('sr-3', ['a.txt', 'b.txt']))
    const pane = wire('sr-3')
    pane.selection.setSelectedIndices([0, 1])

    appendSnapshotEntries('sr-3', [makeEntry('c.txt')], 3)
    flushSync()

    expect(pane.selection.getSelectedIndices()).toEqual([0, 1])
  })

  it('touches nothing on a pane that is not showing a snapshot', () => {
    const pane = wire(null)
    pane.selection.setSelectedIndices([4, 7])

    getOrCreate('sr-4', makeSnapshot('sr-4', ['a.txt']))
    removeEntryFromAllSnapshots('/Users/test/a.txt')
    flushSync()

    expect(pane.selection.getSelectedIndices()).toEqual([4, 7])
  })
})
