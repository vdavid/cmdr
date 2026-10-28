/**
 * The words a group reads out and the facts each row compares. The button has to name the action
 * the verb runs, the reversibility pill has to give the RIGHT reason (a trash on a drive with no
 * Trash must never claim one), and a row has to tell a value that moved from one that didn't.
 */

import { describe, expect, it } from 'vitest'
import type { SuggestedOpView } from '#lib/tauri-commands/index.js'
import {
  actionKey,
  approveKey,
  fileIconId,
  fileName,
  parentFolder,
  reversibilityKey,
  rowFacts,
  runsAsKeys,
  shownReversibility,
  shownVerb,
  sweepHeadingHidden,
  sweepWhen,
} from './suggested-ops-copy'

function op(overrides: Partial<SuggestedOpView> = {}): SuggestedOpView {
  return {
    opId: 1,
    sourcePath: '/Users/someone/Downloads/invoice.pdf',
    newName: null,
    status: 'pending',
    snapshotSize: 1_000,
    snapshotModified: 1_780_000_000,
    ...overrides,
  }
}

describe('the approve button', () => {
  it('names the action each verb runs', () => {
    expect(approveKey('move')).toBe('suggestedOps.approve.move')
    expect(approveKey('trash')).toBe('suggestedOps.approve.trash')
    expect(approveKey('delete')).toBe('suggestedOps.approve.delete')
    expect(approveKey('extract')).toBe('suggestedOps.approve.extract')
    expect(approveKey('addServer')).toBe('suggestedOps.approve.addServer')
    expect(approveKey('editServer')).toBe('suggestedOps.approve.editServer')
  })
})

describe('a server suggestion’s pill', () => {
  it('says where the user takes a saved server back', () => {
    expect(reversibilityKey('addServer', 'serverSettings')).toBe('suggestedOps.reversible.forgetServer')
    expect(reversibilityKey('editServer', 'serverSettings')).toBe('suggestedOps.reversible.changeServerBack')
  })
})

describe('the reversibility pill', () => {
  it('says a trash goes to the Trash, and a move can be moved back', () => {
    expect(reversibilityKey('trash', 'restoreMove')).toBe('suggestedOps.reversible.trash')
    expect(reversibilityKey('move', 'restoreMove')).toBe('suggestedOps.reversible.moveBack')
    expect(reversibilityKey('rename', 'restoreMove')).toBe('suggestedOps.reversible.renameBack')
  })

  it('says what undoing a write removes', () => {
    expect(reversibilityKey('copy', 'deleteWhatWasWritten')).toBe('suggestedOps.reversible.removeCopies')
    expect(reversibilityKey('compress', 'deleteWhatWasWritten')).toBe('suggestedOps.reversible.removeArchive')
    expect(reversibilityKey('extract', 'deleteWhatWasWritten')).toBe('suggestedOps.reversible.removeExtracted')
  })

  it('names a trash on a drive with no Trash as the permanent delete it runs as, everywhere', () => {
    expect(runsAsKeys('permanentDelete').pill).toBe('suggestedOps.noTrash')
    expect(shownVerb('trash', 'permanentDelete')).toBe('delete')
    expect(shownVerb('trash', null)).toBe('trash')
    expect(shownReversibility('restoreMove', 'permanentDelete')).toBe('irreversible')
    expect(shownReversibility('restoreMove', null)).toBe('restoreMove')
    expect(approveKey(shownVerb('trash', 'permanentDelete'))).toBe('suggestedOps.approve.delete')
    expect(actionKey(shownVerb('trash', 'permanentDelete'))).toBe('suggestedOps.action.delete')
  })

  it('leads every title with its action', () => {
    expect(actionKey('move')).toBe('suggestedOps.action.move')
    expect(actionKey('trash')).toBe('suggestedOps.action.trash')
    expect(actionKey('addServer')).toBe('suggestedOps.server.add')
  })

  it('says why a delete or an overwrite is for good', () => {
    expect(reversibilityKey('delete', 'irreversible')).toBe('suggestedOps.irreversible.delete')
    expect(reversibilityKey('compress', 'irreversible')).toBe('suggestedOps.irreversible.overwrite')
    expect(reversibilityKey('move', 'irreversible')).toBe('suggestedOps.irreversible.other')
  })
})

describe('a row’s facts', () => {
  it('shows the live value and flags only what moved since the suggestion', () => {
    const facts = rowFacts(op(), { kind: 'present', size: 2_000, modified: 1_780_000_000 })

    expect(facts).toMatchObject({ kind: 'live', size: 2_000, sizeChanged: true, modifiedChanged: false })
  })

  it('flags nothing when the suggestion recorded nothing to compare against', () => {
    const facts = rowFacts(op({ snapshotSize: null, snapshotModified: null }), {
      kind: 'present',
      size: 2_000,
      modified: 1_790_000_000,
    })

    expect(facts).toMatchObject({ kind: 'live', sizeChanged: false, modifiedChanged: false })
  })

  it('says a file is gone', () => {
    expect(rowFacts(op(), { kind: 'missing' })).toEqual({ kind: 'missing' })
  })

  it('falls back to the snapshot when Cmdr couldn’t look, or hasn’t yet', () => {
    expect(rowFacts(op(), { kind: 'unknown' })).toEqual({ kind: 'snapshot', size: 1_000, modified: 1_780_000_000 })
    expect(rowFacts(op(), undefined)).toEqual({ kind: 'snapshot', size: 1_000, modified: 1_780_000_000 })
  })
})

describe('paths', () => {
  it('splits a path into the name and its folder', () => {
    expect(fileName('/Users/someone/Downloads/invoice.pdf')).toBe('invoice.pdf')
    expect(parentFolder('/Users/someone/Downloads/invoice.pdf')).toBe('/Users/someone/Downloads')
    expect(parentFolder('/invoice.pdf')).toBe('/')
    expect(fileName('/Users/someone/Downloads/Old stuff/')).toBe('Old stuff')
  })

  it('keys the icon the way the backend does', () => {
    expect(fileIconId('/a/Report.PDF')).toBe('ext:pdf')
    expect(fileIconId('/a/Makefile')).toBe('file')
    expect(fileIconId('/a/.zshrc')).toBe('file')
  })
})

describe('a sweep heading', () => {
  // 2026-10-26 15:00 local time.
  const now = new Date(2026, 9, 26, 15, 0, 0).getTime()
  const at = (date: Date) => Math.floor(date.getTime() / 1000)

  it('says minutes for the first hour', () => {
    expect(sweepWhen(at(new Date(2026, 9, 26, 14, 59, 30)), now)).toEqual({ kind: 'justNow' })
    expect(sweepWhen(at(new Date(2026, 9, 26, 14, 45, 0)), now)).toEqual({ kind: 'minutesAgo', minutes: 15 })
  })

  it('names the calendar day after that, not a 24-hour window', () => {
    expect(sweepWhen(at(new Date(2026, 9, 26, 9, 0, 0)), now)).toEqual({ kind: 'today' })
    expect(sweepWhen(at(new Date(2026, 9, 25, 23, 50, 0)), now)).toEqual({ kind: 'yesterday' })
    expect(sweepWhen(at(new Date(2026, 9, 24, 23, 50, 0)), now)).toEqual({ kind: 'earlier' })
  })

  it('hides only for a lone sweep from the last ten minutes', () => {
    const recent = at(new Date(2026, 9, 26, 14, 55, 0))
    expect(sweepHeadingHidden(1, recent, now)).toBe(true)
    expect(sweepHeadingHidden(2, recent, now)).toBe(false)
    expect(sweepHeadingHidden(1, at(new Date(2026, 9, 26, 14, 40, 0)), now)).toBe(false)
  })
})
