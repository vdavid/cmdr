/**
 * What a mousedown on a Full-view row plans to do. These pin the three rules that
 * are otherwise only observable by dragging files around: the `..` row never drags,
 * an empty selection defers the select until the drag threshold decides, and a
 * non-empty one always drags the WHOLE selection with the first row's icon.
 */

import { describe, it, expect } from 'vitest'
import { planRowMouseDown, type RowMouseDownInput } from './full-list-mouse'
import type { FileEntry } from '../types'

function entry(name: string, overrides: Partial<FileEntry> = {}): FileEntry {
  return {
    name,
    path: `/dir/${name}`,
    isDirectory: false,
    isSymlink: false,
    permissions: 0o644,
    owner: 'me',
    group: 'staff',
    iconId: `icon-${name}`,
    extendedMetadataLoaded: false,
    ...overrides,
  }
}

const ROWS: Record<number, FileEntry> = {
  0: entry('..', { path: '/', isDirectory: true, iconId: 'icon-parent' }),
  1: entry('a.txt'),
  2: entry('b.txt'),
  3: entry('sub', { isDirectory: true }),
}

function mouseDown(init: MouseEventInit = {}, target?: HTMLElement): MouseEvent {
  const event = new MouseEvent('mousedown', { button: 0, ...init })
  Object.defineProperty(event, 'target', { value: target ?? document.createElement('div') })
  return event
}

function plan(overrides: Partial<RowMouseDownInput> = {}) {
  return planRowMouseDown({
    event: mouseDown(),
    index: 1,
    cursorIndex: 1,
    selectedIndices: new Set<number>(),
    getEntryAt: (index) => ROWS[index],
    listingId: 'listing-1',
    volumeId: 'root',
    includeHidden: false,
    hasParent: true,
    usingStaticEntries: false,
    isRenaming: false,
    canStartRename: true,
    ...overrides,
  })
}

describe('rows we do not own', () => {
  it.each([1, 2])('ignores mouse button %i', (button) => {
    expect(plan({ event: mouseDown({ button }) })).toEqual({ kind: 'ignore' })
  })

  it('ignores a click inside the inline rename input', () => {
    const input = document.createElement('input')
    input.className = 'rename-input'
    const wrapper = document.createElement('span')
    wrapper.append(input)

    expect(plan({ event: mouseDown({}, input) })).toEqual({ kind: 'ignore' })
  })

  it('ignores a row that has not been fetched yet', () => {
    expect(plan({ index: 99 })).toEqual({ kind: 'ignore' })
  })

  it('only moves the cursor on the ".." row: it is not a real entry to drag', () => {
    expect(plan({ index: 0 })).toEqual({ kind: 'select' })
  })
})

describe('click-to-rename arming', () => {
  it('arms on the row already under the cursor', () => {
    const result = plan({ index: 1, cursorIndex: 1 })

    expect(result.kind === 'drag' && result.startClickToRename).toBe(true)
  })

  it.each([
    ['a different row', { index: 2, cursorIndex: 1 }],
    ['shift held', { event: mouseDown({ shiftKey: true }) }],
    ['cmd held', { event: mouseDown({ metaKey: true }) }],
    ['a rename already open', { isRenaming: true }],
    ['no rename callback wired', { canStartRename: false }],
  ])('does not arm for %s', (_label, overrides) => {
    const result = plan(overrides)

    expect(result.kind === 'drag' && result.startClickToRename).toBe(false)
  })
})

describe('with nothing selected', () => {
  it('defers the selection and drags the single row', () => {
    const result = plan({ index: 2 })

    expect(result).toEqual({
      kind: 'drag',
      startClickToRename: false,
      selectNow: false,
      context: {
        type: 'single',
        path: '/dir/b.txt',
        iconId: 'icon-b.txt',
        index: 2,
        sourceVolumeId: 'root',
        fileInfo: { name: 'b.txt', isDirectory: false, iconId: 'icon-b.txt' },
      },
    })
  })
})

describe('with a selection', () => {
  it('selects on press and drags the whole selection, whichever row was pressed', () => {
    const result = plan({ index: 3, selectedIndices: new Set([1, 2]) })

    expect(result).toEqual({
      kind: 'drag',
      startClickToRename: false,
      selectNow: true,
      context: {
        type: 'selection',
        listingId: 'listing-1',
        indices: [1, 2],
        includeHidden: false,
        hasParent: true,
        sourceVolumeId: 'root',
        // The preview icon comes from the FIRST selected row, not the pressed one.
        iconId: 'icon-a.txt',
        fileInfos: [
          { name: 'a.txt', isDirectory: false, iconId: 'icon-a.txt' },
          { name: 'b.txt', isDirectory: false, iconId: 'icon-b.txt' },
        ],
      },
    })
  })

  it('falls back to the pressed row for the icon when the first selected row is not cached', () => {
    const result = plan({ index: 2, selectedIndices: new Set([50, 2]) })

    expect(result.kind === 'drag' && result.context).toMatchObject({
      iconId: 'icon-b.txt',
      // Uncached rows contribute nothing to the drag preview.
      fileInfos: [{ name: 'b.txt', isDirectory: false, iconId: 'icon-b.txt' }],
    })
  })

  it('drags a search-results pane by PATH, since it has no backend listing to index into', () => {
    const result = plan({ index: 2, selectedIndices: new Set([1, 2]), usingStaticEntries: true })

    expect(result).toEqual({
      kind: 'drag',
      startClickToRename: false,
      selectNow: true,
      context: {
        type: 'paths',
        paths: ['/dir/a.txt', '/dir/b.txt'],
        sourceVolumeId: 'root',
        iconId: 'icon-a.txt',
        fileInfos: [
          { name: 'a.txt', isDirectory: false, iconId: 'icon-a.txt' },
          { name: 'b.txt', isDirectory: false, iconId: 'icon-b.txt' },
        ],
      },
    })
  })
})
