/**
 * Unit tests for the pure operation-log label mapping. Every enum variant maps to
 * a resolved English catalog string, and the summary formats from kind + count via
 * ICU plural (thousands separator on the count). Exhaustive by construction — a new
 * enum variant would fail to compile in the source switch, and any missing case
 * here would surface as a wrong string.
 */

import { describe, it, expect } from 'vitest'
import { tString } from '#lib/intl/messages.svelte.js'
import type { MessageKey } from '#lib/intl/keys.gen.js'
import type {
  ArchiveSubkind,
  ExecutionStatus,
  Initiator,
  NotRollbackableReason,
  OpKind,
  RollbackRefusal,
  RollbackState,
} from '#lib/ipc/bindings.js'
import {
  operationSummary,
  initiatorLabel,
  executionStatusLabel,
  rollbackStateLabel,
  rollbackRefusalNotice,
  rowRollbackAction,
  rowRollbackActionLabel,
  rowStandingNotice,
  rollbackAction,
  rollbackActionLabel,
  selectedCountText,
  partiallyRolledBackSummary,
  coverageNotice,
} from './operation-log-labels'

/** A finished operation that did everything it planned. */
function summaryOf(kind: OpKind, archiveSubkind: ArchiveSubkind | null, itemCount: number): string {
  return operationSummary({ kind, archiveSubkind, itemCount, itemsDone: itemCount, executionStatus: 'done' })
}

describe('operationSummary', () => {
  it('reports what a canceled copy actually copied, and that it was canceled', () => {
    // Regression: a copy canceled about 30 items in read "Copied 1,200 items", the planned count.
    const summary = operationSummary({
      kind: 'copy',
      archiveSubkind: null,
      itemCount: 1_200,
      itemsDone: 30,
      executionStatus: 'canceled',
    })
    expect(summary).toBe('Copied 30 items, then canceled (1,200 planned)')
  })

  it('reports what an operation that stopped on a problem got done', () => {
    const summary = operationSummary({
      kind: 'move',
      archiveSubkind: null,
      itemCount: 1_200,
      itemsDone: 1,
      executionStatus: 'failed',
    })
    expect(summary).toBe('Moved 1 item, then stopped (1,200 planned)')
  })

  it('keeps the planned count for an operation still under way', () => {
    for (const executionStatus of ['queued', 'running'] as const) {
      const summary = operationSummary({
        kind: 'copy',
        archiveSubkind: null,
        itemCount: 5,
        itemsDone: 2,
        executionStatus,
      })
      expect(summary).toBe('Copied 5 items')
    }
  })

  it('formats each op kind from the typed kind + count', () => {
    expect(summaryOf('copy', null, 3)).toBe('Copied 3 items')
    expect(summaryOf('move', null, 3)).toBe('Moved 3 items')
    expect(summaryOf('delete', null, 3)).toBe('Deleted 3 items')
    expect(summaryOf('trash', null, 3)).toBe('Moved 3 items to the trash')
    expect(summaryOf('rename', null, 3)).toBe('Renamed 3 items')
    expect(summaryOf('createFolder', null, 1)).toBe('Created 1 folder')
    expect(summaryOf('createFile', null, 1)).toBe('Created 1 file')
  })

  it('resolves the archive_edit subkind (compress vs edit vs extract)', () => {
    expect(summaryOf('archiveEdit', 'compress', 5)).toBe('Compressed 5 items')
    expect(summaryOf('archiveEdit', 'edit', 1)).toBe('Edited an archive')
    expect(summaryOf('archiveEdit', 'extract', 1)).toBe('Extracted an archive')
    // A missing/unknown subkind falls back to the generic archive-edit label.
    expect(summaryOf('archiveEdit', null, 1)).toBe('Edited an archive')
  })

  it('uses the singular plural branch and a thousands separator', () => {
    expect(summaryOf('copy', null, 1)).toBe('Copied 1 item')
    expect(summaryOf('delete', null, 1_234)).toBe('Deleted 1,234 items')
    expect(summaryOf('tag', null, 1)).toBe('Changed the tags on 1 item')
  })

  it('covers every OpKind (no unmapped kind)', () => {
    const kinds: OpKind[] = [
      'copy',
      'move',
      'delete',
      'trash',
      'rename',
      'createFolder',
      'createFile',
      'archiveEdit',
      'tag',
    ]
    for (const kind of kinds) expect(summaryOf(kind, null, 2)).toBeTruthy()
    const subkinds: ArchiveSubkind[] = ['compress', 'edit', 'extract']
    for (const sub of subkinds) expect(summaryOf('archiveEdit', sub, 2)).toBeTruthy()
  })
})

describe('enum labels', () => {
  it('maps every initiator', () => {
    const cases: Record<Initiator, string> = {
      user: 'You',
      aiClient: 'AI client',
      agent: 'Agent',
      // Mixed provenance: the agent proposed the batch, the user retyped a name in the review.
      // It has to read differently from plain `agent`, or the log credits the agent either way.
      agentEdited: 'Agent, with your edits',
    }
    for (const [value, label] of Object.entries(cases)) {
      expect(initiatorLabel(value as Initiator)).toBe(label)
    }
  })

  it('maps every execution status, avoiding "failed"', () => {
    const cases: Record<ExecutionStatus, string> = {
      queued: 'Queued',
      running: 'Running',
      done: 'Finished',
      failed: 'Didn’t finish',
      canceled: 'Canceled',
    }
    for (const [value, label] of Object.entries(cases)) {
      expect(executionStatusLabel(value as ExecutionStatus)).toBe(label)
    }
  })

  it('maps every rollback state', () => {
    const cases: Record<RollbackState, string> = {
      notRollbackable: 'Can’t roll back',
      rollbackable: 'Can roll back',
      rollingBack: 'Rolling back',
      rolledBack: 'Rolled back',
      partiallyRolledBack: 'Partly rolled back',
    }
    for (const [value, label] of Object.entries(cases)) {
      expect(rollbackStateLabel(value as RollbackState)).toBe(label)
    }
  })
})

describe('rollbackRefusalNotice', () => {
  it('gives every refusal its own sentence', () => {
    const cases: [RollbackRefusal, string][] = [
      [{ kind: 'unknownOperation' }, 'This operation isn’t in your history anymore.'],
      [{ kind: 'alreadyRollingBack' }, 'This one is already rolling back. Watch it in the queue.'],
      [{ kind: 'alreadyRolledBack' }, 'This one is already back the way it was.'],
      [
        { kind: 'isARollback' },
        'This one undid another operation, and an undo doesn’t get undone in turn. The operation it undid is still in the log.',
      ],
      [
        { kind: 'volumeUnavailable', detail: { volumeId: 'smb-nas' } },
        'Connect the drive this operation used, then try again.',
      ],
      [{ kind: 'selection', detail: 'nothingSelected' }, 'Check at least one item to roll back.'],
      [
        { kind: 'selection', detail: 'summaryNotBuilt' },
        'Cmdr is still reading this operation’s items. Try again in a moment.',
      ],
      [
        { kind: 'selection', detail: 'tooManyToggles' },
        'That’s more separate picks than Cmdr can hold. Pick a folder instead.',
      ],
    ]
    for (const [refusal, notice] of cases) {
      expect(tString(rollbackRefusalNotice(refusal))).toBe(notice)
    }
  })

  it('falls back to a reason-free line when the press never reached the backend', () => {
    expect(tString(rollbackRefusalNotice(null))).toBe('Cmdr couldn’t start the rollback. Try again in a moment.')
  })

  it('says WHY an operation is beyond reversing, one reason at a time', () => {
    // The whole point of the split: a merge, an overwrite, and a permanent delete
    // are three different situations, and one shared sentence left the user guessing
    // whether they had done something wrong.
    const cases: [NotRollbackableReason, string][] = [
      [
        'overwrote',
        'This operation replaced files that were already there. Cmdr doesn’t keep copies of what it replaces, so the originals can’t come back.',
      ],
      ['permanentDelete', 'A permanent delete leaves nothing to put back.'],
      [
        'archiveOverwrite',
        'This archive replaced an older one with the same name. Cmdr doesn’t keep copies of what it replaces, so the older one can’t come back.',
      ],
      ['zipEditUnsupported', 'Cmdr can’t undo changes made inside an archive yet.'],
      [
        'journalIncomplete',
        'Cmdr’s record of this operation isn’t complete, so rolling it back could touch the wrong files.',
      ],
      [
        'directoryMerge',
        'This move merged the folder into one that was already there. Cmdr can’t tell which files came along and which were already inside, so there’s no safe way back.',
      ],
      [
        'stagedConflictResolved',
        'This move ran into a name that was already taken and asked what to do about it. That answer is part of the result now, so there’s no single way back.',
      ],
    ]
    for (const [reason, notice] of cases) {
      expect(tString(rollbackRefusalNotice({ kind: 'notRollbackable', detail: reason }))).toBe(notice)
    }
  })

  it('never blames the user or reaches for the words this app doesn’t use', () => {
    const reasons: NotRollbackableReason[] = [
      'overwrote',
      'permanentDelete',
      'archiveOverwrite',
      'zipEditUnsupported',
      'journalIncomplete',
      'directoryMerge',
      'stagedConflictResolved',
    ]
    for (const reason of reasons) {
      const sentence = tString(rollbackRefusalNotice({ kind: 'notRollbackable', detail: reason })).toLowerCase()
      for (const banned of ['error', 'failed', 'invalid', 'you should have']) {
        expect(sentence).not.toContain(banned)
      }
    }
  })
})

describe('rowRollbackAction', () => {
  it('offers a press on exactly the states the backend gate admits', () => {
    // Mirrors `check_rollbackable` (`crates/cmdr-operation-log/src/rollback.rs`) arm for arm. Offering
    // one on any other state would be a button whose only possible answer is a refusal.
    const cases: Record<RollbackState, 'start' | 'finish' | null> = {
      rollbackable: 'start',
      partiallyRolledBack: 'finish',
      notRollbackable: null,
      rollingBack: null,
      rolledBack: null,
    }
    for (const [state, action] of Object.entries(cases)) {
      expect(rowRollbackAction(state as RollbackState)).toBe(action)
    }
  })

  it('names what THIS press does, so a half-undone operation never promises a fresh reversal', () => {
    expect(rowRollbackActionLabel('start')).toBe('Roll back')
    expect(rowRollbackActionLabel('finish')).toBe('Roll back the rest')
  })
})

describe('rollbackAction', () => {
  const tally = (selected: number, selectable: number, housekeepingWaiting = 0) => ({
    selected,
    selectable,
    housekeepingWaiting,
  })

  it('offers nothing where the backend gate offers nothing, whatever the tree says', () => {
    for (const state of ['notRollbackable', 'rollingBack', 'rolledBack'] as const) {
      expect(rollbackAction(state, tally(3, 3), true)).toBeNull()
    }
  })

  it('counts the checked items on a fresh rollback, untouched or narrowed', () => {
    expect(rollbackAction('rollbackable', tally(12, 12), false)).toEqual({
      action: { kind: 'selected', count: 12, finishing: false, subset: false },
      disabled: false,
    })
    expect(rollbackAction('rollbackable', tally(3, 12), true)).toEqual({
      action: { kind: 'selected', count: 3, finishing: false, subset: true },
      disabled: false,
    })
  })

  it('rolls back "the rest" of a partly reversed operation until the person narrows it', () => {
    expect(rollbackAction('partiallyRolledBack', tally(4, 4), false)).toEqual({
      action: { kind: 'rest' },
      disabled: false,
    })
    expect(rollbackAction('partiallyRolledBack', tally(1, 4), true)).toEqual({
      action: { kind: 'selected', count: 1, finishing: true, subset: true },
      disabled: false,
    })
  })

  it('disables the press at 0 of M, the one state it can’t help', () => {
    expect(rollbackAction('rollbackable', tally(0, 5), true)?.disabled).toBe(true)
    expect(rollbackAction('partiallyRolledBack', tally(0, 5), true)?.disabled).toBe(true)
  })

  it('never strands an operation at 0 of 0: the whole-operation press sweeps what’s left', () => {
    // A copied folder held back while a file sat in it, now empty: nothing to check, but a
    // pass with no selection removes it.
    expect(rollbackAction('rollbackable', tally(0, 0, 1), false)).toEqual({ action: { kind: 'all' }, disabled: false })
    expect(rollbackAction('partiallyRolledBack', tally(0, 0, 1), true)).toEqual({
      action: { kind: 'rest' },
      disabled: false,
    })
  })

  it('works as it always did while the tree can’t be read', () => {
    expect(rollbackAction('rollbackable', null, false)).toEqual({ action: { kind: 'all' }, disabled: false })
    expect(rollbackAction('partiallyRolledBack', null, false)).toEqual({ action: { kind: 'rest' }, disabled: false })
  })
})

describe('rollbackActionLabel and selectedCountText', () => {
  it('words each press, with thousands separators on the count', () => {
    expect(rollbackActionLabel({ kind: 'all' })).toBe('Roll back')
    expect(rollbackActionLabel({ kind: 'rest' })).toBe('Roll back the rest')
    expect(rollbackActionLabel({ kind: 'selected', count: 1_234, finishing: false, subset: true })).toBe(
      'Roll back selected (1,234)',
    )
  })

  it('says how many of the reversible items are checked', () => {
    expect(selectedCountText({ selected: 3, selectable: 12 })).toBe('3 of 12 selected')
    expect(selectedCountText({ selected: 1_000, selectable: 2_000_000 })).toBe('1,000 of 2,000,000 selected')
  })
})

describe('rowStandingNotice', () => {
  it('explains the states whose badge leaves a person guessing, and stays quiet on the rest', () => {
    // A partly-reversed operation is the one someone lands on after cancelling a reversal:
    // some files came back, some didn't, and the badge alone says neither.
    expect(tString(rowStandingNotice('partiallyRolledBack', null, 'copy') as MessageKey)).toBe(
      'Cmdr rolled back what it could and left the rest as it was. Rolling back the rest takes another pass and skips anything Cmdr still isn’t sure about.',
    )
    expect(tString(rowStandingNotice('notRollbackable', 'permanentDelete', 'delete') as MessageKey)).toBe(
      'A permanent delete leaves nothing to put back.',
    )
    // A not-rollbackable operation with no recorded reason is one still running; a
    // dangling label would be worse than silence.
    expect(rowStandingNotice('notRollbackable', null, 'copy')).toBeNull()
    expect(rowStandingNotice('rollingBack', null, 'copy')).toBeNull()
    expect(rowStandingNotice('rolledBack', null, 'copy')).toBeNull()
  })

  it('says what rolling back does to the selected items, by what the reversal does', () => {
    const cases: [OpKind, string][] = [
      ['copy', 'Rolling back deletes the selected copies. The originals stay where they are.'],
      ['createFolder', 'Rolling back deletes the selected items this operation created.'],
      ['move', 'Rolling back moves the selected items back where they came from.'],
      ['trash', 'Rolling back moves the selected items back where they came from.'],
      ['rename', 'Rolling back gives the selected items their old names back.'],
      ['tag', 'Rolling back gives the selected items the tags they had before.'],
    ]
    for (const [kind, sentence] of cases) {
      expect(tString(rowStandingNotice('rollbackable', null, kind) as MessageKey)).toBe(sentence)
    }
    // Undoing a move takes nothing away, so its sentence must not say it does.
    expect(tString(rowStandingNotice('rollbackable', null, 'move') as MessageKey)).not.toContain('delete')
  })

  it('promises no more than the engine delivers: another pass may skip again', () => {
    // A reversal lands partial for two reasons the UI can't tell apart: a cancel
    // midway, and items the engine couldn't verify (which a second pass skips
    // again). Copy that promised completion would be wrong half the time.
    const sentence = tString(rowStandingNotice('partiallyRolledBack', null, 'move') as MessageKey).toLowerCase()
    expect(sentence).toContain('skips')
    for (const banned of ['error', 'failed', 'will finish', 'completely']) {
      expect(sentence).not.toContain(banned)
    }
  })
})

describe('coverageNotice', () => {
  it('says an interrupted operation may not be fully recorded, and stays quiet on search-only gaps', () => {
    expect(tString(coverageNotice('interrupted') as MessageKey)).toBe(
      'Cmdr quit before this operation finished, so it may not have recorded everything the operation did. Rolling back reaches only what it recorded.',
    )
    for (const reason of [
      'capped',
      'indexAbsent',
      'indexStale',
      'volumeNotLive',
      'searchRowIncomplete',
      null,
    ] as const) {
      expect(coverageNotice(reason)).toBeNull()
    }
  })
})

describe('partiallyRolledBackSummary', () => {
  it('says what came back, what was skipped, and what still waits, from the counts', () => {
    // `selectable` counts held-back items too (they still await reversal), so what waits
    // untouched is the difference.
    expect(partiallyRolledBackSummary({ rolledBack: 4, heldBack: 2, selectable: 4 })).toBe(
      '4 items are back the way they were. Cmdr skipped 2 items it wasn’t sure about. 2 items haven’t been rolled back yet.',
    )
  })

  it('leaves out a sentence with nothing to count, and the singular reads as one', () => {
    expect(partiallyRolledBackSummary({ rolledBack: 1, heldBack: 0, selectable: 1 })).toBe(
      '1 item is back the way it was. 1 item hasn’t been rolled back yet.',
    )
    expect(partiallyRolledBackSummary({ rolledBack: 1_500, heldBack: 1, selectable: 1 })).toBe(
      '1,500 items are back the way they were. Cmdr skipped 1 item it wasn’t sure about.',
    )
  })

  it('gives way to the fixed sentence when there’s nothing to count', () => {
    expect(partiallyRolledBackSummary({ rolledBack: 0, heldBack: 0, selectable: 0 })).toBeNull()
  })
})
