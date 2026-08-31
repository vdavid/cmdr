/**
 * The Pause / Resume and Cancel a history row offers while its reversal runs.
 *
 * Mounted through `OperationLogDialog` rather than the control on its own,
 * because the questions worth pinning are about the ROW: that the buttons show
 * up on exactly the row whose reversal is live, that they command the INVERSE
 * operation and never the row's own finished one (the bug this whole surface
 * would have if it read `opId`), and that they go away when the reversal does.
 *
 * The session registry is real, fed the same way the backend feeds it. That's
 * the point: these presses have to be the queue window's presses, against the
 * same guards, not a second path to the same IPC.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount, unmount, flushSync, tick } from 'svelte'
import type { OperationRow as JournalRow, OperationSnapshot } from '$lib/ipc/bindings'
import type { OperationLogDetail } from '$lib/tauri-commands'

const { commandMocks } = vi.hoisted(() => ({
  commandMocks: {
    pauseOperation: vi.fn<(id: string) => Promise<void>>(() => Promise.resolve()),
    resumeOperation: vi.fn<(id: string) => Promise<void>>(() => Promise.resolve()),
    cancelOperation: vi.fn<(id: string) => Promise<void>>(() => Promise.resolve()),
    cancelWriteOperation: vi.fn<(id: string, rollback: boolean) => Promise<void>>(() => Promise.resolve()),
    resolveWriteConflict: vi.fn(() => Promise.resolve('resolved')),
    rollbackOperation: vi.fn<(id: string) => Promise<{ inverseOpId: string }>>(() =>
      Promise.resolve({ inverseOpId: 'inv-dispatched' }),
    ),
    getOperationLogDetail: vi.fn<() => Promise<OperationLogDetail | null>>(() => Promise.resolve(null)),
  },
}))

vi.mock('$lib/tauri-commands', () => ({
  // The honest double for `list_operations`: a session seeds from the live
  // registry, and an id it can't find there is one that has already ended.
  listOperations: vi.fn(() => Promise.resolve(liveOperations)),
  getRecentOperationLogEntries: vi.fn(() => Promise.resolve([])),
  ...commandMocks,
  onOperationsChanged: vi.fn(() => Promise.resolve(() => {})),
  onWriteProgress: vi.fn(() => Promise.resolve(() => {})),
  onWriteComplete: vi.fn(() => Promise.resolve(() => {})),
  onWriteError: vi.fn(() => Promise.resolve(() => {})),
  onWriteCancelled: vi.fn(() => Promise.resolve(() => {})),
  onWriteSettled: vi.fn(() => Promise.resolve(() => {})),
  onWriteConflict: vi.fn(() => Promise.resolve(() => {})),
  onWriteConflictResolved: vi.fn(() => Promise.resolve(() => {})),
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
}))

vi.mock('$lib/settings/reactive-settings.svelte', () => ({
  getFileSizeFormat: () => 'binary',
  formatDateTime: () => '2026-07-09 12:00',
}))

vi.mock('$lib/logging/logger', () => ({
  getAppLogger: () => ({ warn: vi.fn(), info: vi.fn(), debug: vi.fn(), error: vi.fn() }),
}))

import OperationLogDialog from './OperationLogDialog.svelte'
import { operationLogState, closeOperationLog } from './operation-log-trigger.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'
import {
  destroyOperationSessions,
  getOperationSessions,
  initOperationSessions,
} from '$lib/file-operations/operation-session/window-operation-sessions.svelte'

/** A finished copy in the history feed, mid-reversal unless a test says otherwise. */
function journalRow(over: Partial<JournalRow> = {}): JournalRow {
  return {
    opId: 'op-copy',
    kind: 'copy',
    archiveSubkind: null,
    initiator: 'user',
    executionStatus: 'done',
    rollbackState: 'rollingBack',
    notRollbackableReason: null,
    rollsBackOpId: null,
    inverseOpId: 'inv-1',
    sourceVolumeId: 'root',
    destVolumeId: null,
    startedAt: 1_700_000_000,
    endedAt: 1_700_000_010,
    itemCount: 3,
    itemsDone: 3,
    bytesTotal: 0,
    searchCoverage: 'full',
    searchCoverageReason: null,
    devSummary: null,
    ...over,
  }
}

/** The registry row for the reversal itself: a real managed operation that
 *  reverses the copy above. */
function reversalSnapshot(operationId: string, status: OperationSnapshot['status'] = 'running'): OperationSnapshot {
  return {
    operationId,
    operationType: 'delete',
    status,
    source: '/Volumes/Backup',
    destination: null,
    supportsRollback: false,
    reverses: 'copy',
    error: null,
  }
}

/** What `list_operations` would answer right now. A session claimed after the
 *  last event seeds from this, so a test never has to race the seed. */
let liveOperations: OperationSnapshot[] = []

/** One `operations-changed`, delivered the way the backend delivers it.
 *
 *  Called BEFORE the view that binds, the way the backend orders it: the manager
 *  registers the reversal before the dispatch returns. A session claimed with the
 *  registry empty seeds as `gone` and stays settled for good (write-once), which
 *  is correct behavior and would make these tests measure the wrong thing. */
function emitSnapshot(operations: OperationSnapshot[]): void {
  liveOperations = operations
  getOperationSessions()?._testEmit({ kind: 'snapshot', operations })
  flushSync()
}

let target: HTMLElement
let view: ReturnType<typeof mount> | null = null

async function mountDialog(entries: JournalRow[]): Promise<void> {
  operationLogState.entries = entries
  operationLogState.loading = false
  operationLogState.loadError = false
  operationLogState.hasMore = false
  operationLogState.open = true
  view = mount(OperationLogDialog, { target, props: {} })
  await tick()
  flushSync()
}

/** A control by its visible words, scoped to the row's action area. */
function control(label: string): HTMLButtonElement | null {
  return (
    [...target.querySelectorAll<HTMLButtonElement>('.op-row button')].find((b) => b.textContent.trim() === label) ??
    null
  )
}

function requireControl(label: string): HTMLButtonElement {
  const found = control(label)
  if (!found) throw new Error(`No "${label}" control on the row`)
  return found
}

beforeEach(async () => {
  closeOperationLog()
  liveOperations = []
  document.body.innerHTML = ''
  target = document.createElement('div')
  document.body.appendChild(target)
  await initOperationSessions()
  for (const mock of Object.values(commandMocks)) mock.mockClear()
  commandMocks.rollbackOperation.mockResolvedValue({ inverseOpId: 'inv-dispatched' })
  commandMocks.getOperationLogDetail.mockResolvedValue(null)
})

afterEach(() => {
  if (view) void unmount(view)
  view = null
  destroyOperationSessions()
})

describe('a rolling-back row offers to pause and to stop', () => {
  it('shows Pause and Cancel while the reversal runs', async () => {
    emitSnapshot([reversalSnapshot('inv-1')])
    await mountDialog([journalRow()])

    expect(control('Pause')).not.toBeNull()
    expect(control('Cancel')).not.toBeNull()
    // ❌ Never both at once: one button, two words.
    expect(control('Resume')).toBeNull()
    // A reversal is under way, so the row's own Roll back is gone (the gate
    // refuses a second one) and can't be confused with these.
    expect(control('Roll back')).toBeNull()
  })

  it('offers nothing on a row whose reversal this window never saw', async () => {
    await mountDialog([journalRow()])
    // No snapshot: the reversal is over, or belongs to a window that isn't here.
    expect(control('Pause')).toBeNull()
    expect(control('Cancel')).toBeNull()
  })

  it('offers nothing on a row that is not rolling back', async () => {
    emitSnapshot([reversalSnapshot('inv-1')])
    await mountDialog([journalRow({ rollbackState: 'rollbackable', inverseOpId: null })])

    expect(control('Pause')).toBeNull()
    expect(control('Cancel')).toBeNull()
    expect(control('Roll back')).not.toBeNull()
  })

  it('drops the controls once the reversal is over', async () => {
    emitSnapshot([reversalSnapshot('inv-1')])
    await mountDialog([journalRow()])
    expect(control('Pause')).not.toBeNull()

    // The reversal was stopped and wound down. The journal row this dialog read
    // on open still says `rolling_back` (it re-reads on reopen, not on settle),
    // so the live outcome is the only thing that can take a dead press away —
    // and it comes from the terminal EVENT, never from leaving the snapshot.
    getOperationSessions()?._testEmit({
      kind: 'cancelled',
      event: {
        operationId: 'inv-1',
        operationType: 'delete',
        filesProcessed: 2,
        rollback: { outcome: 'notRolledBack', reversed: 0, skips: [] },
      },
    })
    emitSnapshot([])

    expect(control('Pause')).toBeNull()
    expect(control('Cancel')).toBeNull()
  })

  it('only offers Cancel while the reversal waits its turn on the drive', async () => {
    emitSnapshot([reversalSnapshot('inv-1', 'queued')])
    await mountDialog([journalRow()])

    // Nothing to park yet; cancelling drops it before it ever spawns.
    expect(control('Pause')).toBeNull()
    expect(control('Cancel')).not.toBeNull()
  })
})

describe('the presses command the reversal, not the operation being reversed', () => {
  it('pauses the inverse operation', async () => {
    emitSnapshot([reversalSnapshot('inv-1')])
    await mountDialog([journalRow()])

    requireControl('Pause').click()
    await tick()

    expect(commandMocks.pauseOperation).toHaveBeenCalledWith('inv-1')
    // ❌ The row's own operation is finished; pausing it would be a no-op at best.
    expect(commandMocks.pauseOperation).not.toHaveBeenCalledWith('op-copy')
  })

  it('cancels the inverse operation through the manager, keeping what came back', async () => {
    emitSnapshot([reversalSnapshot('inv-1')])
    await mountDialog([journalRow()])

    requireControl('Cancel').click()
    await tick()

    expect(commandMocks.cancelOperation).toHaveBeenCalledWith('inv-1')
    // ❌ Not `cancelWriteOperation(id, true)`: that's Rollback, which would ask
    // the reversal to undo itself.
    expect(commandMocks.cancelWriteOperation).not.toHaveBeenCalled()
  })

  it('resumes a parked reversal, and says it is parked rather than finished', async () => {
    emitSnapshot([reversalSnapshot('inv-1', 'paused')])
    await mountDialog([journalRow()])

    // The badge still reads "Rolling back" (journal truth), so the row has to say
    // out loud that nothing is moving.
    expect(target.textContent).toContain('Rolling back')
    expect(target.textContent).toContain('Paused')
    expect(control('Pause')).toBeNull()

    requireControl('Resume').click()
    await tick()

    expect(commandMocks.resumeOperation).toHaveBeenCalledWith('inv-1')
  })

  it('leaves the dialog clean for a screen reader', async () => {
    emitSnapshot([reversalSnapshot('inv-1', 'paused')])
    await mountDialog([journalRow()])

    await expectNoA11yViolations(target)
  })

  it('names the row it belongs to for a screen reader', async () => {
    emitSnapshot([reversalSnapshot('inv-1')])
    await mountDialog([journalRow()])

    // Without this a screen reader announces a bare "Pause" with no idea which of
    // several history rows it acts on.
    expect(requireControl('Pause').getAttribute('aria-describedby')).toBe('op-head-op-copy')
    expect(requireControl('Cancel').getAttribute('aria-describedby')).toBe('op-head-op-copy')
  })
})

describe('a press in flight cannot be sent twice', () => {
  it('holds the pause button while the request is out', async () => {
    let release: () => void = () => {}
    commandMocks.pauseOperation.mockImplementation(
      () =>
        new Promise<void>((resolve) => {
          release = resolve
        }),
    )
    emitSnapshot([reversalSnapshot('inv-1')])
    await mountDialog([journalRow()])

    requireControl('Pause').click()
    flushSync()
    expect(requireControl('Pause').disabled).toBe(true)

    release()
    await tick()
    flushSync()
    expect(requireControl('Pause').disabled).toBe(false)
  })

  it('holds the cancel button once a cancel has landed', async () => {
    emitSnapshot([reversalSnapshot('inv-1')])
    await mountDialog([journalRow()])

    requireControl('Cancel').click()
    await tick()
    flushSync()

    // The guard is held until the operation is gone, not until the IPC returns:
    // a second click has nothing left to ask for.
    expect(requireControl('Cancel').disabled).toBe(true)
    requireControl('Cancel').click()
    await tick()
    expect(commandMocks.cancelOperation).toHaveBeenCalledTimes(1)
  })
})

describe('a rollback started from this dialog gets the controls immediately', () => {
  it('takes the inverse id off the dispatch, with no re-read', async () => {
    // The manager registers the inverse before the dispatch returns, so the
    // registry already knows it by the time the row binds.
    emitSnapshot([reversalSnapshot('inv-dispatched')])
    await mountDialog([journalRow({ rollbackState: 'rollbackable', inverseOpId: null })])

    requireControl('Roll back').click()
    await tick()
    flushSync()
    // The confirmation stacks over the log; answering it dispatches.
    const confirmation = target.querySelector('[data-dialog-id="rollback-confirmation"]')
    if (!confirmation) throw new Error('the rollback confirmation never came up')
    const confirm = [...confirmation.querySelectorAll<HTMLButtonElement>('button')].find(
      (b) => b.textContent.trim() === 'Roll back',
    )
    if (!confirm) throw new Error('the confirmation has no confirming button')
    confirm.click()
    await vi.waitFor(() => {
      expect(commandMocks.rollbackOperation).toHaveBeenCalledWith('op-copy')
    })
    flushSync()

    // The dispatch's answer is the same journal fact a fresh read would carry, so
    // the row can command the reversal without waiting for one.
    expect(operationLogState.entries[0]?.inverseOpId).toBe('inv-dispatched')
    expect(operationLogState.entries[0]?.rollbackState).toBe('rollingBack')

    requireControl('Pause').click()
    await tick()
    expect(commandMocks.pauseOperation).toHaveBeenCalledWith('inv-dispatched')
  })
})
