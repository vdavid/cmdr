/**
 * Unit coverage for the MCP adapter's validating parsers.
 *
 * The adapter never `as`-casts a raw event payload into a typed `CommandArgs`: it
 * whitelist-parses every discriminant string, and a malformed value collapses to
 * `undefined` so the listener skips the dispatch (a malformed payload must not
 * reach a handler). These pure parsers carry that contract; the listener wiring
 * itself (a routes module) has no coverage gate, so this pins the load-bearing
 * part — the parsers.
 */
import { describe, it, expect } from 'vitest'
import {
  parsePane,
  parseSortColumn,
  parseSortOrder,
  parseSelectMode,
  parseTabAction,
  parseViewMode,
  parseConfirmDialogType,
  parseSelectCount,
  parseCursorTarget,
} from './mcp-listeners'

describe('parsePane', () => {
  it('accepts left/right', () => {
    expect(parsePane('left')).toBe('left')
    expect(parsePane('right')).toBe('right')
  })
  it('rejects anything else', () => {
    for (const bad of ['both', '', 'Left', 0, null, undefined, {}]) {
      expect(parsePane(bad)).toBeUndefined()
    }
  })
})

describe('parseSortColumn', () => {
  it('accepts the canonical columns', () => {
    for (const col of ['name', 'extension', 'size', 'modified', 'created'] as const) {
      expect(parseSortColumn(col)).toBe(col)
    }
  })
  it('maps the MCP `ext` alias to `extension`', () => {
    expect(parseSortColumn('ext')).toBe('extension')
  })
  it('rejects unknown columns', () => {
    for (const bad of ['date', '', 'NAME', 42, null]) {
      expect(parseSortColumn(bad)).toBeUndefined()
    }
  })
})

describe('parseSortOrder', () => {
  it('accepts asc/desc', () => {
    expect(parseSortOrder('asc')).toBe('asc')
    expect(parseSortOrder('desc')).toBe('desc')
  })
  it('rejects toggle and unknowns (the MCP tool never emits toggle)', () => {
    for (const bad of ['toggle', 'ascending', '', null]) {
      expect(parseSortOrder(bad)).toBeUndefined()
    }
  })
})

describe('parseSelectMode', () => {
  it('accepts replace/add/subtract', () => {
    for (const mode of ['replace', 'add', 'subtract'] as const) {
      expect(parseSelectMode(mode)).toBe(mode)
    }
  })
  it('rejects unknowns', () => {
    for (const bad of ['remove', '', 'Add', null]) {
      expect(parseSelectMode(bad)).toBeUndefined()
    }
  })
})

describe('parseTabAction', () => {
  it('accepts every tab action', () => {
    for (const action of ['new', 'close', 'close_others', 'activate', 'reopen', 'set_pinned'] as const) {
      expect(parseTabAction(action)).toBe(action)
    }
  })
  it('rejects unknowns', () => {
    for (const bad of ['open', 'closeOthers', '', null]) {
      expect(parseTabAction(bad)).toBeUndefined()
    }
  })
})

describe('parseViewMode', () => {
  it('accepts full/brief', () => {
    expect(parseViewMode('full')).toBe('full')
    expect(parseViewMode('brief')).toBe('brief')
  })
  it('rejects unknowns', () => {
    for (const bad of ['list', '', 'Full', null]) {
      expect(parseViewMode(bad)).toBeUndefined()
    }
  })
})

describe('parseConfirmDialogType', () => {
  it('accepts the two dialog kinds', () => {
    expect(parseConfirmDialogType('transfer-confirmation')).toBe('transfer-confirmation')
    expect(parseConfirmDialogType('delete-confirmation')).toBe('delete-confirmation')
  })
  it('rejects unknowns (including the bare `transfer` short form)', () => {
    for (const bad of ['transfer', 'delete', '', null]) {
      expect(parseConfirmDialogType(bad)).toBeUndefined()
    }
  })
})

describe('parseSelectCount', () => {
  it('accepts the `all` sentinel and any number (including 0)', () => {
    expect(parseSelectCount('all')).toBe('all')
    expect(parseSelectCount(0)).toBe(0)
    expect(parseSelectCount(7)).toBe(7)
  })
  it('rejects non-number, non-`all` values', () => {
    for (const bad of ['7', '', null, undefined, {}]) {
      expect(parseSelectCount(bad)).toBeUndefined()
    }
  })
})

describe('parseCursorTarget', () => {
  it('accepts a numeric index or a name string', () => {
    expect(parseCursorTarget(3)).toBe(3)
    expect(parseCursorTarget('README.md')).toBe('README.md')
    expect(parseCursorTarget('')).toBe('')
  })
  it('rejects other types', () => {
    for (const bad of [null, undefined, {}, true]) {
      expect(parseCursorTarget(bad)).toBeUndefined()
    }
  })
})
