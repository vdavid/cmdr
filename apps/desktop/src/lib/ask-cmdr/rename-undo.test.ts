/**
 * The one display judgment an undo makes: how loud to be about what came back.
 *
 * Undo never forces, so a partial result is the NORMAL outcome whenever a file
 * changed after the rename or its old name got taken again. Reporting a partial as
 * a clean success would tell the user their files are back when some are not, which
 * is the failure this whole surface exists to prevent (invariant 9).
 */

import { describe, expect, it } from 'vitest'

import { undoStateFromReport } from './rename-undo'
import type { OperationUndoOutcome, SkipBreakdown, UndoReport } from '$lib/tauri-commands'

function outcome(overrides: Partial<OperationUndoOutcome> = {}): OperationUndoOutcome {
  return {
    operationId: 'op-1',
    restored: 0,
    skipped: 0,
    skips: [],
    finalState: 'rolledBack',
    refusal: null,
    ...overrides,
  }
}

function report(overrides: Partial<UndoReport> = {}): UndoReport {
  return { operations: [outcome()], restored: 0, skipped: 0, ...overrides }
}

function group(overrides: Partial<SkipBreakdown> = {}): SkipBreakdown {
  return { reason: 'drift', count: 1, exampleName: 'a.pdf', ...overrides }
}

describe('undoStateFromReport', () => {
  it('reports a clean reversal as undone', () => {
    expect(undoStateFromReport(report({ restored: 23, operations: [outcome({ restored: 23 })] }))).toEqual({
      status: 'undone',
      restored: 23,
    })
  })

  it('reports a skipped file as partial, never as a clean success', () => {
    const state = undoStateFromReport(
      report({ restored: 19, skipped: 4, operations: [outcome({ restored: 19, skipped: 4 })] }),
    )

    expect(state).toEqual({ status: 'partial', restored: 19, skipped: 4, refusedBatches: 0, skips: [] })
  })

  it('counts a refused batch separately, since it reports no per-file numbers', () => {
    const state = undoStateFromReport(
      report({
        restored: 12,
        operations: [
          outcome({ operationId: 'op-2', restored: 12 }),
          outcome({ operationId: 'op-1', finalState: null, refusal: { kind: 'alreadyRolledBack' } }),
        ],
      }),
    )

    // Restored 12, yet a whole batch was missed: partial, not undone.
    expect(state).toEqual({ status: 'partial', restored: 12, skipped: 0, refusedBatches: 1, skips: [] })
  })

  it('reports nothing-happened as unavailable rather than as an undo of zero files', () => {
    const state = undoStateFromReport(
      report({ operations: [outcome({ finalState: null, refusal: { kind: 'alreadyRolledBack' } })] }),
    )

    expect(state).toEqual({ status: 'unavailable' })
  })

  it('carries each reason through, so the line can name a file instead of a class', () => {
    const state = undoStateFromReport(
      report({
        restored: 5,
        skipped: 2,
        operations: [
          outcome({
            restored: 5,
            skipped: 2,
            skips: [
              group({ reason: 'drift', count: 1, exampleName: 'invoice-2026.pdf' }),
              group({ reason: 'restoreTargetOccupied', count: 1, exampleName: 'receipt-2026.pdf' }),
            ],
          }),
        ],
      }),
    )

    expect(state).toEqual({
      status: 'partial',
      restored: 5,
      skipped: 2,
      refusedBatches: 0,
      skips: [
        { reason: 'drift', count: 1, exampleName: 'invoice-2026.pdf' },
        { reason: 'restoreTargetOccupied', count: 1, exampleName: 'receipt-2026.pdf' },
      ],
    })
  })

  it('merges the same reason across batches into one group, keeping the counts complete', () => {
    // A job-wide undo reverses several batches; the same reason hitting two of them is
    // one thing to tell the user, and the count has to be the sum or the report
    // understates what stayed behind.
    const state = undoStateFromReport(
      report({
        restored: 0,
        skipped: 5,
        operations: [
          outcome({
            operationId: 'op-2',
            skipped: 3,
            skips: [group({ reason: 'drift', count: 3, exampleName: 'newest.pdf' })],
          }),
          outcome({
            operationId: 'op-1',
            skipped: 2,
            skips: [
              group({ reason: 'drift', count: 1, exampleName: 'older.pdf' }),
              group({ reason: 'failed', count: 1, exampleName: 'locked.pdf' }),
            ],
          }),
        ],
      }),
    )

    expect(state).toEqual({
      status: 'partial',
      restored: 0,
      skipped: 5,
      refusedBatches: 0,
      skips: [
        // First seen wins the example: the operations arrive newest-batch-first.
        { reason: 'drift', count: 4, exampleName: 'newest.pdf' },
        { reason: 'failed', count: 1, exampleName: 'locked.pdf' },
      ],
    })
  })

  it('treats an already-restored batch (every item an idempotent no-op) as undone', () => {
    // The engine counts an already-gone item as reversed, so a re-issued undo lands
    // here with a real `restored` count and no skips.
    expect(undoStateFromReport(report({ restored: 5, operations: [outcome({ restored: 5 })] }))).toEqual({
      status: 'undone',
      restored: 5,
    })
  })
})
