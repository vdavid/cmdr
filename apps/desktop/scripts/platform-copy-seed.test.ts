/**
 * Tests for the resolved-copy file: its row validator and English block builder
 * (`platform-copy-rows.ts`), its seed generator (`platform-copy-seed.ts`), and
 * the committed `docs/notes/windows-port/copy-final.json` itself.
 */
import { describe, expect, it } from 'vitest'
import { parsePlatformSelect } from '../src/lib/intl/platform-select.ts'
import {
  copyFinalIssues,
  englishValueIssues,
  englishValueOf,
  isApplyRow,
  matchesKeyPattern,
  readCopyFinal,
  rowIssues,
} from './platform-copy-rows.ts'
import type { ApplyRow, CopyRow } from './platform-copy-rows.ts'
import { addMissingRows, seedCopyFinal } from './platform-copy-seed.ts'
import type { Proposal, ProposalRow } from './platform-copy-seed.ts'

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

  it('lets a pending row leave branches unresolved', () => {
    expect(rowIssues(applyRow({ status: 'pending', branches: { macos: 'A', windows: null, other: null } }))).toEqual([])
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

function proposalRow(overrides: Partial<ProposalRow>): ProposalRow {
  return {
    key: 'area.thing.label',
    keys: ['area.thing.label'],
    file: 'area.json',
    source: 'catalog',
    mechanism: 'select',
    glossaryTerms: [],
    whereInUi: 'A dialog',
    macos: 'Mac text',
    windows: 'Windows text',
    linux: 'Linux text',
    ...overrides,
  }
}

const proposal: Proposal = {
  strings: [
    proposalRow({}),
    proposalRow({ key: 'area.macOnly', keys: ['area.macOnly'], mechanism: 'none', windows: null, linux: null }),
    proposalRow({ key: 'ai.local.notInstalled', keys: ['ai.local.notInstalled'], mechanism: 'split-feature' }),
    proposalRow({ key: 'area.neutral', keys: ['area.neutral'], mechanism: 'neutral' }),
    proposalRow({ key: 'viewer.copyDialog.confirmBody', keys: ['viewer.copyDialog.confirmBody'], mechanism: 'code' }),
    proposalRow({
      key: 'errors.provider.appBased.serious',
      keys: ['errors.provider.appBased.serious'],
      file: 'errors.json',
      mechanism: 'none',
      windows: null,
      linux: null,
    }),
    proposalRow({
      key: 'area.trash.toast',
      keys: ['area.trash.toast'],
      glossaryTerms: ['trash'],
      macos: 'Moved to Trash',
    }),
    proposalRow({ key: 'commands.appQuit.label', keys: ['commands.appQuit.label'], file: 'commands.json' }),
    proposalRow({
      key: 'main.upgradeNudge.mac + main.upgradeNudge.other',
      keys: ['main.upgradeNudge.mac', 'main.upgradeNudge.other'],
      file: 'main.json',
    }),
    proposalRow({
      key: 'ai.secretError.genericBody',
      keys: ['ai.secretError.genericBody'],
      file: 'ai.json',
      macosChanged: true,
    }),
  ],
  reviewedNoChange: [
    {
      key: 'commands.navGoHome.label',
      file: 'commands.json',
      reason: 'reads fine',
      currentEnglish: 'Go to home folder',
    },
    { key: 'area.reviewed', file: 'area.json', reason: 'shared word', currentEnglish: 'Eject' },
  ],
}

const english = {
  'commands.navGoHome.label': 'Go to home folder',
  'main.quit.countdown': 'Quitting in {seconds} seconds',
}

describe('seedCopyFinal', () => {
  const { file, missing } = seedCopyFinal(proposal, english)
  const row = (key: string): CopyRow => {
    const found = file.rows.find((candidate) => candidate.key === key)
    if (!found) throw new Error(`no row ${key}`)
    return found
  }

  it('produces a file the validator accepts', () => {
    expect(copyFinalIssues(file)).toEqual([])
  })

  it('seeds a select row as pending, with the proposal’s three texts', () => {
    expect(row('area.thing.label')).toMatchObject({
      status: 'pending',
      branches: { macos: 'Mac text', windows: 'Windows text', other: 'Linux text' },
      whereInUi: 'A dialog',
    })
  })

  it('keeps a macOS-only row and a neutral row as no change', () => {
    expect(row('area.macOnly').status).toBe('none')
    expect(row('area.neutral').status).toBe('none')
  })

  it('marks a row behind a capability as gated, whatever its mechanism', () => {
    expect(row('ai.local.notInstalled').status).toBe('gated')
    expect(row('ai.local.notInstalled').sources.row).toContain('localAi')
  })

  it('parks the key-placeholder rows until the keymap work', () => {
    expect(row('viewer.copyDialog.confirmBody').status).toBe('none')
    expect(row('viewer.copyDialog.confirmBody').note).toContain('#1102')
  })

  it('promotes a none row a verdict wrote text for', () => {
    expect(row('errors.provider.appBased.serious').status).toBe('pending')
  })

  it('seeds the register of a Trash or Quit row', () => {
    expect(row('commands.appQuit.label').register).toBe('action')
    expect(row('area.trash.toast').register).toBe('sentence')
  })

  it('resolves a pair into its survivor, with both old keys on record', () => {
    const pair = row('main.upgradeNudge')
    expect(pair).toMatchObject({
      status: 'apply',
      proposalKey: 'main.upgradeNudge.mac + main.upgradeNudge.other',
      collapseFrom: { mac: 'main.upgradeNudge.mac', other: 'main.upgradeNudge.other' },
    })
    expect(isApplyRow(pair) && englishValueOf(pair)).toMatch(/^\{platform, select, macos \{.*\} other \{.*\}\}$/)
  })

  it('resolves the accepted macOS change, keeping the old English for Linux', () => {
    expect(row('ai.secretError.genericBody')).toMatchObject({
      status: 'apply',
      macosChanged: true,
      branches: {
        other: 'Check that your system keyring is available, then try again. If it persists, restart the app.',
      },
    })
  })

  it('turns an overturned reviewed row into pending work, and keeps the rest as no change', () => {
    expect(row('commands.navGoHome.label')).toMatchObject({
      status: 'pending',
      branches: { macos: 'Go to home folder', windows: null, other: null },
    })
    expect(row('area.reviewed').status).toBe('none')
    expect(row('area.reviewed').note).toContain('shared word')
  })

  it('adds a value the proposal never scanned from today’s English, and reports the ones it could not find', () => {
    expect(row('main.quit.countdown')).toMatchObject({ status: 'pending', register: 'narration', file: 'main.json' })
    expect(missing).toContain('askCmdr.tool.quit.doing')
    expect(file.rows.some((candidate) => candidate.key === 'askCmdr.tool.quit.doing')).toBe(false)
  })
})

describe('addMissingRows', () => {
  it('keeps every existing row byte for byte and appends only the new ones', () => {
    const { file: seeded } = seedCopyFinal(proposal, english)
    const resolved: CopyRow = { ...seeded.rows[0], status: 'none', note: 'resolved by hand' }
    const existing = { ...seeded, rows: [resolved, ...seeded.rows.slice(1, 3)] }

    const { file, added } = addMissingRows(existing, seeded)

    expect(file.rows.slice(0, 3)).toEqual(existing.rows)
    expect(file.rows.length).toBe(seeded.rows.length)
    expect(added.length).toBe(seeded.rows.length - 3)
    expect(added).not.toContain(resolved.key)
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
