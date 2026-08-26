/**
 * `trash-undo.ts`: reading an undo's report honestly, and saying it in en.
 *
 * The honesty rule is the point of these: an undo that left anything in the trash
 * must never render as a clean success. The rendered-string assertions double as
 * the en parity net for the `fileOperations.trash.*` keys, so a copy edit lands in
 * the catalog AND here together.
 */

import { describe, it, expect, beforeAll, afterAll, beforeEach, vi } from 'vitest'
import { _setLocaleForTests } from '$lib/intl/locale'
import { trashUndoOutcome, trashUndoMessage, trashUndoLevel, runTrashUndo } from './trash-undo'
import type { UndoReport, OperationUndoOutcome } from '$lib/tauri-commands'
import type { RollbackRefusal } from '$lib/ipc/bindings'
import type { ToastOptions } from '$lib/ui/toast/toast-store.svelte'

const { undoOperations, addToast, dismissToast } = vi.hoisted(() => ({
  undoOperations: vi.fn<(ids: string[]) => Promise<UndoReport>>(),
  addToast: vi.fn<(message: string, options?: ToastOptions) => string>(),
  dismissToast: vi.fn<(id: string) => void>(),
}))

vi.mock('$lib/tauri-commands', () => ({ undoOperations }))
vi.mock('$lib/ui/toast', () => ({ addToast, dismissToast }))

beforeAll(() => {
  _setLocaleForTests('en-US')
})
afterAll(() => {
  _setLocaleForTests(null)
})

function operation(overrides: Partial<OperationUndoOutcome> = {}): OperationUndoOutcome {
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
  return { operations: [operation()], restored: 0, skipped: 0, ...overrides }
}

describe('trashUndoOutcome', () => {
  it('reads a complete restore as restored', () => {
    expect(trashUndoOutcome(report({ restored: 3 }))).toEqual({ status: 'restored', restored: 3 })
  })

  it('reads anything left in the trash as partial, never as a success', () => {
    expect(trashUndoOutcome(report({ restored: 2, skipped: 1 }))).toEqual({
      status: 'partial',
      restored: 2,
      skipped: 1,
    })
  })

  it('treats a refusal as partial even when no item was skipped', () => {
    // A refusal carries no per-item numbers at all, so folding it into `skipped`
    // would understate what was missed: 2 came back, the rest never got a chance.
    const refusal: RollbackRefusal = { kind: 'volumeUnavailable', detail: { volumeId: 'usb-1' } }
    const outcome = trashUndoOutcome(
      report({ restored: 2, skipped: 0, operations: [operation({ restored: 2, refusal })] }),
    )
    expect(outcome).toEqual({ status: 'partial', restored: 2, skipped: 0 })
  })

  it('reads nothing-reversed-and-nothing-attempted as unavailable', () => {
    expect(trashUndoOutcome(report())).toEqual({ status: 'unavailable' })
  })

  it('does not call an all-skipped undo a success', () => {
    expect(trashUndoOutcome(report({ restored: 0, skipped: 4 })).status).toBe('partial')
  })
})

describe('trashUndoMessage (en)', () => {
  it('words a single restored file in the singular', () => {
    expect(trashUndoMessage({ status: 'restored', restored: 1 })).toBe('Put back 1 file.')
  })

  it('words several restored files in the plural, with thousands separators', () => {
    expect(trashUndoMessage({ status: 'restored', restored: 1234 })).toBe('Put back 1,234 files.')
  })

  it('names both halves of a partial result', () => {
    expect(trashUndoMessage({ status: 'partial', restored: 2, skipped: 1 })).toBe(
      'Put back 2 files; 1 item stayed in the trash.',
    )
  })

  it('inflects each half on its own count', () => {
    // Both clauses carry a plural driver on purpose: a bare preformatted number
    // gives a translator nothing to agree with, which is what sent three
    // languages hunting for a workaround (`docs/i18n/*/style.md` § Plurals).
    expect(trashUndoMessage({ status: 'partial', restored: 1, skipped: 3 })).toBe(
      'Put back 1 file; 3 items stayed in the trash.',
    )
  })

  it('explains an undo that had nothing to reverse', () => {
    expect(trashUndoMessage({ status: 'unavailable' })).toBe(
      "Nothing to put back. These items may already be back, or their drive isn't connected.",
    )
  })
})

describe('trashUndoLevel', () => {
  it('only a complete restore reads as a success', () => {
    expect(trashUndoLevel({ status: 'restored', restored: 1 })).toBe('success')
    expect(trashUndoLevel({ status: 'partial', restored: 1, skipped: 1 })).toBe('info')
    expect(trashUndoLevel({ status: 'unavailable' })).toBe('info')
  })
})

describe('runTrashUndo', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  /** Every `addToast` call, as (message, options) pairs. */
  function raised(): { message: string; options: ToastOptions }[] {
    return addToast.mock.calls.map((call) => ({ message: call[0], options: call[1] ?? {} }))
  }

  it('holds a persistent progress toast while the restore runs', async () => {
    undoOperations.mockResolvedValue(report({ restored: 2 }))
    await runTrashUndo('op-1')

    // A restore is a queued operation and waits out the volume's lane, so it can
    // outlive any transient timeout. Nothing must be able to time it out.
    const progress = raised()[0]
    expect(progress.message).toBe('Putting them back...')
    expect(progress.options.dismissal).toBe('persistent')
  })

  it('undoes exactly the operation it was given', async () => {
    undoOperations.mockResolvedValue(report({ restored: 2 }))
    await runTrashUndo('op-1')
    expect(undoOperations).toHaveBeenCalledWith(['op-1'])
  })

  it('takes the progress toast down and reports what came back', async () => {
    undoOperations.mockResolvedValue(report({ restored: 2 }))
    await runTrashUndo('op-1')

    // Replacing in place can't work here: `addToast` replaces content and level
    // but never dismissal, so the persistent one has to go and a transient one
    // takes its place.
    expect(dismissToast).toHaveBeenCalledWith('trash-undo')
    const result = raised()[1]
    expect(result.message).toBe('Put back 2 files.')
    expect(result.options.level).toBe('success')
    expect(result.options.dismissal).toBeUndefined()
  })

  it('reports a partial restore as info, not success', async () => {
    undoOperations.mockResolvedValue(report({ restored: 2, skipped: 1 }))
    await runTrashUndo('op-1')

    const result = raised()[1]
    expect(result.message).toBe('Put back 2 files; 1 item stayed in the trash.')
    expect(result.options.level).toBe('info')
  })

  it('still clears the progress toast when the undo never runs', async () => {
    // A wedged volume or a dead journal rejects the IPC. Leaving a persistent
    // "Putting them back..." on screen forever is the one outcome that must not
    // happen, whatever else does.
    undoOperations.mockRejectedValue(new Error('the volume went away'))
    await runTrashUndo('op-1')

    expect(dismissToast).toHaveBeenCalledWith('trash-undo')
    expect(raised()[1].message).toBe(
      "Nothing to put back. These items may already be back, or their drive isn't connected.",
    )
  })

  it('does not let a refused undo reject into the click handler', async () => {
    undoOperations.mockRejectedValue(new Error('nope'))
    await expect(runTrashUndo('op-1')).resolves.toBeUndefined()
  })
})
