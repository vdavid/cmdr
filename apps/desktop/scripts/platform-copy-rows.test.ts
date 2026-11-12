/**
 * Tests for the resolved-copy file: its row validator and English block builder
 * (`platform-copy-rows.ts`), and the committed
 * `docs/notes/windows-port/copy-final.json` itself.
 */
import { describe, expect, it } from 'vitest'
import { parsePlatformSelect } from '../src/lib/intl/platform-select.ts'
import {
  englishValueIssues,
  englishValueOf,
  isApplyRow,
  matchesKeyPattern,
  readCopyFinal,
  rowIssues,
} from './platform-copy-rows.ts'
import type { ApplyRow, CopyRow } from './platform-copy-rows.ts'

function applyRow(overrides: Partial<CopyRow> = {}): ApplyRow {
  return {
    key: 'area.thing.label',
    file: 'area.json',
    source: 'catalog',
    mechanism: 'select',
    status: 'apply',
    branches: { macos: 'Open in Finder', windows: 'Open in File Explorer', other: 'Open in your file manager' },
    sources: { windows: 'proposal', other: 'proposal' },
    ...overrides,
  } as ApplyRow
}

describe('englishValueOf', () => {
  it('writes three different branches as one canonical block', () => {
    expect(englishValueOf(applyRow())).toBe(
      '{platform, select, macos {Open in Finder} windows {Open in File Explorer} other {Open in your file manager}}',
    )
  })

  it('folds a branch equal to other, so a Windows-only change is a {windows, other} block', () => {
    const row = applyRow({ branches: { macos: 'Quit now', windows: 'Exit now', other: 'Quit now' } })
    expect(englishValueOf(row)).toBe('{platform, select, windows {Exit now} other {Quit now}}')
  })

  it('folds windows into other for a Linux-only change, and macOS keeps its own branch', () => {
    const row = applyRow({ branches: { macos: 'Show in Finder', windows: 'Reveal', other: 'Reveal' } })
    expect(englishValueOf(row)).toBe('{platform, select, macos {Show in Finder} other {Reveal}}')
  })

  it('keeps macos and windows as two equal branches when only Linux differs', () => {
    const row = applyRow({ branches: { macos: 'Hidden files', windows: 'Hidden files', other: 'Dotfiles' } })
    const value = englishValueOf(row)
    expect(value).toBe('{platform, select, macos {Hidden files} windows {Hidden files} other {Dotfiles}}')
    expect(parsePlatformSelect(value)?.windows).toBe('Hidden files')
  })

  it('writes three equal branches as a plain value', () => {
    const row = applyRow({ branches: { macos: 'Same', windows: 'Same', other: 'Same' } })
    expect(englishValueOf(row)).toBe('Same')
  })

  it('keeps a typographic apostrophe and a nested plural inside a branch', () => {
    const plural = '{count, plural, one {Don’t move # file} other {Don’t move # files}}'
    const row = applyRow({ branches: { macos: plural, windows: plural.replaceAll('move', 'delete'), other: plural } })
    expect(englishValueIssues(row)).toEqual([])
    expect(parsePlatformSelect(englishValueOf(row))?.other).toBe(plural)
  })
})

describe('englishValueIssues', () => {
  it('refuses a raw branch with an apostrophe directly before a brace', () => {
    const row = applyRow({
      key: 'errors.listing.thing.title',
      branches: { macos: 'Open {system_settings}', windows: "Open l'{name}", other: 'Open your settings' },
    })
    expect(englishValueIssues(row).join('\n')).toMatch(/apostrophe/)
  })

  it('refuses an ICU branch that does not parse', () => {
    const row = applyRow({ branches: { macos: 'Fine', windows: '{count, plural, one {x}', other: 'Also fine' } })
    expect(englishValueIssues(row)).not.toEqual([])
  })

  it('accepts a raw row whose {system_settings} token sits in macos only', () => {
    const row = applyRow({
      key: 'errors.listing.thing.suggestion',
      branches: {
        macos: 'Open {system_settings} > {privacy_and_security}',
        windows: 'Open Settings > Privacy & security',
        other: 'Check your system’s privacy settings',
      },
    })
    expect(englishValueIssues(row)).toEqual([])
  })
})

describe('rowIssues', () => {
  it('accepts a complete apply row', () => {
    expect(rowIssues(applyRow())).toEqual([])
  })

  it('refuses an apply row with no windows branch: Windows reading Linux words is always a decision', () => {
    const row = applyRow({ branches: { macos: 'A', windows: null, other: 'B' } })
    expect(rowIssues(row).join('\n')).toMatch(/all three branches.*no windows/)
    expect(isApplyRow(row)).toBe(false)
  })

  it('refuses a status outside the four it knows', () => {
    expect(rowIssues(applyRow({ status: 'pending' as CopyRow['status'] })).join('\n')).toMatch(
      /`status` must be one of/,
    )
  })

  it('wants a source for each branch an apply row decides', () => {
    expect(rowIssues(applyRow({ sources: { windows: 'proposal' } })).join('\n')).toMatch(/sources\.other/)
  })

  it('wants a register on an apply row that names the Trash or quitting', () => {
    const trash = applyRow({ branches: { macos: 'Move to Trash', windows: 'Delete', other: 'Move to Trash' } })
    expect(rowIssues(trash).join('\n')).toMatch(/register/)
    expect(rowIssues({ ...trash, register: 'label' })).toEqual([])
  })

  it('does not read a select arm named trash as the word', () => {
    const selector = '{kind, select, trash {Removing} other {Copying}}'
    const row = applyRow({
      branches: { macos: selector, windows: selector.replace('Removing', 'Deleting'), other: selector },
    })
    expect(rowIssues(row)).toEqual([])
  })

  it('holds an open branch to the macOS text', () => {
    const row = applyRow({ openBranches: ['windows'] })
    expect(rowIssues(row).join('\n')).toMatch(/open windows branch must hold the macOS text/)
  })

  it('wants a reason on every lint exception', () => {
    expect(rowIssues(applyRow({ lintExceptions: [{ branch: 'windows', reason: ' ' }] }))).not.toEqual([])
  })
})

describe('matchesKeyPattern', () => {
  it('matches an exact key or a prefix ending in *', () => {
    expect(matchesKeyPattern('ai.local.*', 'ai.local.notInstalled')).toBe(true)
    expect(matchesKeyPattern('ai.local.*', 'ai.localOnly')).toBe(false)
    expect(matchesKeyPattern('menu.file.getInfo', 'menu.file.getInfo')).toBe(true)
    expect(matchesKeyPattern('menu.file.getInfo', 'menu.file.getInfoMore')).toBe(false)
  })
})

describe('the committed copy-final.json', () => {
  const file = readCopyFinal()

  it('holds every proposal row and the rows beyond it', () => {
    expect(file.rows.length).toBeGreaterThan(590)
  })

  it('builds a legal English value for every apply row', () => {
    const issues = file.rows
      .filter(isApplyRow)
      .flatMap((row) => englishValueIssues(row).map((issue) => `${row.key}: ${issue}`))
    expect(issues).toEqual([])
  })

  it('resolves the 14 pairs and the two accepted macOS changes', () => {
    expect(file.rows.filter((row) => row.collapseFrom !== undefined && row.status === 'apply').length).toBe(14)
    expect(
      file.rows
        .filter((row) => row.macosChanged === true)
        .map((row) => row.key)
        .sort(),
    ).toEqual(['ai.secretError.genericBody', 'goToPath.dialog.inputPlaceholder'])
  })
})
