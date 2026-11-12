import { describe, it, expect } from 'vitest'
import { fileRowAttributes, fileRowId, fileRowKind, fileRowName, renderedCursorRow, rowSizeText } from './file-row-a11y'
import { createParentEntry } from './file-list-utils'
import type { FileEntry } from '../types'
import { _setLocaleForTests } from '#lib/intl/locale.js'

function entry(overrides: Partial<FileEntry> & { name: string }): FileEntry {
  return {
    path: `/root/${overrides.name}`,
    isDirectory: false,
    isSymlink: false,
    permissions: 0o644,
    owner: '',
    group: '',
    iconId: 'file',
    ...overrides,
  }
}

describe('fileRowKind', () => {
  it('names folders, symlinks, packages, and archives, and gives a plain file no kind', () => {
    expect(fileRowKind(entry({ name: 'sub', isDirectory: true }))).toBe('folder')
    expect(fileRowKind(entry({ name: 'link', isSymlink: true }))).toBe('symlink')
    expect(fileRowKind(entry({ name: 'Safari.app', isDirectory: true, isPackage: true }))).toBe('package')
    expect(fileRowKind(entry({ name: 'c.zip', isArchive: true }))).toBe('archive')
    expect(fileRowKind(entry({ name: 'a.txt' }))).toBeUndefined()
  })

  it('calls a symlink to a folder a symlink, the way its badge does', () => {
    expect(fileRowKind(entry({ name: 'docs-link', isDirectory: true, isSymlink: true }))).toBe('symlink')
  })
})

// WCAG 2.5.3 (axe `label-content-name-mismatch`): the words the row shows run through the
// name in order, and the words only the name says sit in parentheses, which axe drops.
describe('fileRowName', () => {
  it('says the name, then the kind in parentheses', () => {
    expect(fileRowName({ entry: entry({ name: 'subfolder', isDirectory: true }) })).toBe('subfolder (folder)')
    expect(fileRowName({ entry: entry({ name: 'Safari.app', isDirectory: true, isPackage: true }) })).toBe(
      'Safari.app (package)',
    )
    expect(fileRowName({ entry: entry({ name: 'c.zip', isArchive: true }) })).toBe('c.zip (archive)')
    expect(fileRowName({ entry: entry({ name: 'd-link', isSymlink: true }) })).toBe('d-link (symlink)')
  })

  it('gives a plain file its name alone, since the extension is in it', () => {
    expect(fileRowName({ entry: entry({ name: 'a.txt' }) })).toBe('a.txt')
  })

  // Regression: "Parent folder, 36,86 kB" sounded like the parent's size, but the `..` row's
  // size cell shows the CURRENT folder's total (VoiceOver listening run, 2026-10-15). The
  // words in parentheses say whose it is; axe drops them, so the visible size still matches.
  it('names the `..` row "Parent folder", then says its size is this folder’s', () => {
    const parent = createParentEntry('/')
    expect(fileRowName({ entry: parent })).toBe('Parent folder')
    expect(fileRowName({ entry: parent, details: { size: '<dir>' } })).toBe('Parent folder, (this folder) <dir>')
    expect(fileRowName({ entry: parent, details: { size: '36,860' } })).toBe('Parent folder, (this folder) 36,860')
  })

  it('says what the row’s badges mean beside the kind', () => {
    expect(fileRowName({ entry: entry({ name: 'a.txt' }), syncStatus: 'online_only' })).toBe('a.txt (Online-only)')
    expect(fileRowName({ entry: entry({ name: 'Desktop', isDirectory: true }), isRestricted: true })).toBe(
      'Desktop (folder, Restricted folder)',
    )
    expect(fileRowName({ entry: entry({ name: 'cold.bin', inColdStorage: true }) })).toBe('cold.bin (In cold storage)')
    // `unknown` draws no badge, so it says nothing either.
    expect(fileRowName({ entry: entry({ name: 'a.txt' }), syncStatus: 'unknown' })).toBe('a.txt')
  })

  it('adds the Full view’s size, modified date, and git status in that order', () => {
    const details = {
      size: '1 KB',
      date: { column: 'modified', text: '2026-10-14 10:30' },
      gitStatus: 'Modified',
    } as const
    expect(fileRowName({ entry: entry({ name: 'a.txt' }), details })).toBe(
      'a.txt, 1 KB, (modified) 2026-10-14 10:30, Modified in git',
    )
    expect(fileRowName({ entry: entry({ name: 'sub', isDirectory: true }), details })).toBe(
      'sub (folder), 1 KB, (modified) 2026-10-14 10:30, Modified in git',
    )
  })

  // The date cell shows the date the rows are sorted by (`displayedDateColumn`), so the name
  // says which one it is.
  it.each([
    ['created', 'a.txt, (created) 2026-10-14 10:30'],
    ['added', 'a.txt, (added) 2026-10-14 10:30'],
    ['modified', 'a.txt, (modified) 2026-10-14 10:30'],
  ] as const)('names the %s date the date cell shows', (column, expected) => {
    const details = { date: { column, text: '2026-10-14 10:30' } }
    expect(fileRowName({ entry: entry({ name: 'a.txt' }), details })).toBe(expected)
  })

  it('leaves out a size, date, or git status it doesn’t have', () => {
    expect(fileRowName({ entry: entry({ name: 'a.txt' }), details: { size: '1 KB' } })).toBe('a.txt, 1 KB')
    const noDate = { size: '', date: { column: 'modified', text: '' } } as const
    expect(fileRowName({ entry: entry({ name: 'a.txt' }), details: noDate })).toBe('a.txt')
  })

  it('joins with no "and", in a language whose lists end in one', () => {
    _setLocaleForTests('de')
    try {
      const name = fileRowName({
        entry: entry({ name: 'sub', isDirectory: true }),
        details: { size: '1 KB', date: { column: 'modified', text: '2026-10-14 10:30' } },
      })
      expect(name).toMatch(/^sub \(.+\), 1 KB, \(.+\) 2026-10-14 10:30$/)
      expect(name).not.toMatch(/\bund\b/)
    } finally {
      _setLocaleForTests(null)
    }
  })
})

describe('fileRowName for a restic snapshot row', () => {
  // The Ext cell shows the snapshot's hostname between the name and the size, so the name
  // says it there too (WCAG 2.5.3), with the words around it in parentheses.
  it('says the host after the name, before the size', () => {
    const snapshot = entry({ name: '2026-10-12 21.00.03 (a1b2c3d4)', isDirectory: true })
    expect(fileRowName({ entry: snapshot, details: { host: 'testnas', size: '1 KB' } })).toBe(
      '2026-10-12 21.00.03 (a1b2c3d4) (folder), (Backed up from) testnas, 1 KB',
    )
  })
})

describe('rowSizeText', () => {
  const triads = [
    { value: '1', tierClass: 'size-kb' },
    { value: ' 024', tierClass: 'size-b' },
  ]

  it('says the size the cell shows, joined', () => {
    expect(rowSizeText({ triads })).toBe('1 024')
    expect(rowSizeText({ triads, dirState: 'size' })).toBe('1 024')
    expect(rowSizeText({ triads, dirState: 'lower-bound' })).toBe('≥1 024')
  })

  it('says the cell’s override word when it shows one', () => {
    expect(rowSizeText({ override: 'No access', triads })).toBe('No access')
  })

  it('says the `<dir>` placeholder while the cell shows it, and nothing for an empty cell', () => {
    expect(rowSizeText({ triads, dirState: 'dir' })).toBe('<dir>')
    expect(rowSizeText({ triads, dirState: 'scanning' })).toBe('<dir>')
    expect(rowSizeText({})).toBeUndefined()
  })
})

describe('fileRowAttributes', () => {
  it('gives every option its id, name, marked state, and place in the whole list', () => {
    expect(fileRowAttributes({ listId: 's1', globalIndex: 2, totalCount: 8, isMarked: true, name: 'a.txt' })).toEqual({
      id: 's1-row-2',
      'aria-label': 'a.txt',
      'aria-selected': true,
      'aria-posinset': 3,
      'aria-setsize': 8,
    })
  })

  // ❗ VoiceOver invents a position from the RENDERED siblings for an option without one
  // ("Parent folder (1 of 8)" beside "subfolder (1 of 7)"), so the `..` row counts too.
  it('counts the `..` row as position 1, and marks an unmarked row `aria-selected="false"`', () => {
    expect(
      fileRowAttributes({ listId: 's1', globalIndex: 0, totalCount: 8, isMarked: false, name: 'Parent folder' }),
    ).toMatchObject({ 'aria-selected': false, 'aria-posinset': 1, 'aria-setsize': 8 })
  })
})

describe('renderedCursorRow', () => {
  const rows = [
    { file: entry({ name: 'a.txt' }), globalIndex: 4 },
    { file: entry({ name: 'b.txt' }), globalIndex: 5 },
  ]

  it('names the cursor row by its id and path when it’s rendered', () => {
    expect(renderedCursorRow('s1', 5, rows)).toEqual({ id: fileRowId('s1', 5), key: '/root/b.txt' })
  })

  // A dangling `aria-activedescendant` is an axe critical (`aria-valid-attr-value`) and
  // leaves VoiceOver reading a stale row.
  it('names nothing when the cursor row is outside the virtual window', () => {
    expect(renderedCursorRow('s1', 9, rows)).toBeUndefined()
    expect(renderedCursorRow('s1', 0, [])).toBeUndefined()
  })
})
