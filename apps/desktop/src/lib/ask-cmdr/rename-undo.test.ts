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
import type { OperationUndoOutcome, UndoReport } from '$lib/tauri-commands'

function outcome(overrides: Partial<OperationUndoOutcome> = {}): OperationUndoOutcome {
  return {
    operationId: 'op-1',
    restored: 0,
    skipped: 0,
    finalState: 'rolledBack',
    refusal: null,
    ...overrides,
  }
}

function report(overrides: Partial<UndoReport> = {}): UndoReport {
  return { operations: [outcome()], restored: 0, skipped: 0, ...overrides }
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

    expect(state).toEqual({ status: 'partial', restored: 19, skipped: 4, refusedBatches: 0 })
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
    expect(state).toEqual({ status: 'partial', restored: 12, skipped: 0, refusedBatches: 1 })
  })

  it('reports nothing-happened as unavailable rather than as an undo of zero files', () => {
    const state = undoStateFromReport(
      report({ operations: [outcome({ finalState: null, refusal: { kind: 'alreadyRolledBack' } })] }),
    )

    expect(state).toEqual({ status: 'unavailable' })
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
