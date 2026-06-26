import { describe, it, expect } from 'vitest'
import { isFileListBackgroundClick } from './pane-background-dblclick'

/**
 * Builds a Full-view-shaped DOM: a scroll surface holding a sticky header, a
 * short rows region (the `[role="listbox"]`), and the empty space below it that
 * a few-file directory leaves bare.
 */
function buildFullView() {
  const surface = document.createElement('div')
  surface.setAttribute('data-file-list-surface', '')

  const header = document.createElement('div')
  header.className = 'header-row'
  const headerCell = document.createElement('span') // a sort trigger inside the header
  header.appendChild(headerCell)

  const listbox = document.createElement('div')
  listbox.setAttribute('role', 'listbox')
  const row = document.createElement('div')
  row.className = 'file-entry'
  const cell = document.createElement('span') // a child node inside a row
  row.appendChild(cell)
  listbox.appendChild(row)

  surface.appendChild(header)
  surface.appendChild(listbox)
  // `emptyBelow` is the surface's own background, BELOW the (short) listbox — the
  // Full-mode case that the old `[role="listbox"]` gate missed.
  return { surface, header, headerCell, listbox, row, cell }
}

describe('isFileListBackgroundClick', () => {
  it('is true for the surface background (incl. empty space below a short list)', () => {
    const { surface } = buildFullView()
    expect(isFileListBackgroundClick(surface)).toBe(true)
  })

  it('is true for the listbox region background (between/around rows)', () => {
    const { listbox } = buildFullView()
    expect(isFileListBackgroundClick(listbox)).toBe(true)
  })

  it('is false on a row', () => {
    const { row } = buildFullView()
    expect(isFileListBackgroundClick(row)).toBe(false)
  })

  it('is false on a cell inside a row', () => {
    const { cell } = buildFullView()
    expect(isFileListBackgroundClick(cell)).toBe(false)
  })

  it('is false on the Full view sticky column header (it sorts)', () => {
    const { header, headerCell } = buildFullView()
    expect(isFileListBackgroundClick(header)).toBe(false)
    expect(isFileListBackgroundClick(headerCell)).toBe(false)
  })

  it('is false outside any list surface (e.g. the breadcrumb header)', () => {
    const el = document.createElement('div')
    el.className = 'header'
    expect(isFileListBackgroundClick(el)).toBe(false)
  })

  it('is false for a null / non-element target', () => {
    expect(isFileListBackgroundClick(null)).toBe(false)
  })
})
