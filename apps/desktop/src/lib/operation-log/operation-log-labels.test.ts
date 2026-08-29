/**
 * Unit tests for the pure operation-log label mapping. Every enum variant maps to
 * a resolved English catalog string, and the summary formats from kind + count via
 * ICU plural (thousands separator on the count). Exhaustive by construction — a new
 * enum variant would fail to compile in the source switch, and any missing case
 * here would surface as a wrong string.
 */

import { describe, it, expect } from 'vitest'
import { tString } from '$lib/intl/messages.svelte'
import type {
  ArchiveSubkind,
  ExecutionStatus,
  Initiator,
  ItemOutcome,
  NotRollbackableReason,
  OpKind,
  RollbackRefusal,
  RollbackState,
} from '$lib/ipc/bindings'
import {
  operationSummary,
  initiatorLabel,
  executionStatusLabel,
  rollbackStateLabel,
  itemOutcomeLabel,
  rollbackRefusalNotice,
} from './operation-log-labels'

describe('operationSummary', () => {
  it('formats each op kind from the typed kind + count', () => {
    expect(operationSummary('copy', null, 3)).toBe('Copied 3 items')
    expect(operationSummary('move', null, 3)).toBe('Moved 3 items')
    expect(operationSummary('delete', null, 3)).toBe('Deleted 3 items')
    expect(operationSummary('trash', null, 3)).toBe('Moved 3 items to the trash')
    expect(operationSummary('rename', null, 3)).toBe('Renamed 3 items')
    expect(operationSummary('createFolder', null, 1)).toBe('Created 1 folder')
    expect(operationSummary('createFile', null, 1)).toBe('Created 1 file')
  })

  it('resolves the archive_edit subkind (compress vs edit vs extract)', () => {
    expect(operationSummary('archiveEdit', 'compress', 5)).toBe('Compressed 5 items')
    expect(operationSummary('archiveEdit', 'edit', 1)).toBe('Edited an archive')
    expect(operationSummary('archiveEdit', 'extract', 1)).toBe('Extracted an archive')
    // A missing/unknown subkind falls back to the generic archive-edit label.
    expect(operationSummary('archiveEdit', null, 1)).toBe('Edited an archive')
  })

  it('uses the singular plural branch and a thousands separator', () => {
    expect(operationSummary('copy', null, 1)).toBe('Copied 1 item')
    expect(operationSummary('delete', null, 1_234)).toBe('Deleted 1,234 items')
  })

  it('covers every OpKind (no unmapped kind)', () => {
    const kinds: OpKind[] = ['copy', 'move', 'delete', 'trash', 'rename', 'createFolder', 'createFile', 'archiveEdit']
    for (const kind of kinds) expect(operationSummary(kind, null, 2)).toBeTruthy()
    const subkinds: ArchiveSubkind[] = ['compress', 'edit', 'extract']
    for (const sub of subkinds) expect(operationSummary('archiveEdit', sub, 2)).toBeTruthy()
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
      done: 'Done',
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

  it('maps every item outcome', () => {
    const cases: Record<ItemOutcome, string> = {
      done: 'Done',
      skipped: 'Skipped',
      failed: 'Didn’t finish',
      rolledBack: 'Rolled back',
    }
    for (const [value, label] of Object.entries(cases)) {
      expect(itemOutcomeLabel(value as ItemOutcome)).toBe(label)
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
        { kind: 'volumeUnavailable', detail: { volumeId: 'smb-nas' } },
        'Connect the drive this operation used, then try again.',
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
