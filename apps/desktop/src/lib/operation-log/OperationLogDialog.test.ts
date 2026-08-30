/**
 * Component + a11y tests for `OperationLogDialog.svelte`: the alpha operation-log
 * dialog renders one grouped row per operation with a client-formatted summary,
 * carries the ALPHA badge, and expands a row to its per-item detail (fetched
 * lazily over IPC). Paging state itself is covered in `operation-log-trigger.test.ts`.
 */

import { describe, it, vi, expect, beforeEach } from 'vitest'
import { mount, tick } from 'svelte'
import type { NotRollbackableReason, OperationRow } from '$lib/ipc/bindings'
import type { OperationLogDetail } from '$lib/tauri-commands'
import OperationLogDialog from './OperationLogDialog.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'
import { operationLogState, closeOperationLog } from './operation-log-trigger.svelte'
import { RollbackRefusalFailure } from './rollback-refusal'

const getOperationLogDetailMock =
  vi.fn<
    (payload: { operationId: string; itemLimit: number; itemOffset: number }) => Promise<OperationLogDetail | null>
  >()
const rollbackOperationMock = vi.fn<(operationId: string) => Promise<{ inverseOpId: string }>>()
vi.mock('$lib/tauri-commands', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
  getRecentOperationLogEntries: vi.fn(() => Promise.resolve([])),
  getOperationLogDetail: (id: string, l: number, o: number) =>
    getOperationLogDetailMock({ operationId: id, itemLimit: l, itemOffset: o }),
  rollbackOperation: (id: string) => rollbackOperationMock(id),
}))

// Avoid pulling the reactive-settings chain; a stable stamp is all the row needs.
vi.mock('$lib/settings/reactive-settings.svelte', () => ({
  getFileSizeFormat: () => 'binary',
  formatDateTime: () => '2026-07-09 12:00',
}))

vi.mock('$lib/logging/logger', () => ({
  getAppLogger: () => ({ warn: vi.fn(), info: vi.fn(), debug: vi.fn(), error: vi.fn() }),
}))

function opRow(overrides: Partial<OperationRow>): OperationRow {
  return {
    opId: 'op-1',
    kind: 'copy',
    archiveSubkind: null,
    initiator: 'user',
    executionStatus: 'done',
    rollbackState: 'rollbackable',
    notRollbackableReason: null,
    rollsBackOpId: null,
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
    ...overrides,
  }
}

function setEntries(entries: OperationRow[]): void {
  operationLogState.entries = entries
  operationLogState.loading = false
  operationLogState.loadError = false
  operationLogState.hasMore = false
  operationLogState.open = true
}

async function mountDialog(): Promise<HTMLElement> {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(OperationLogDialog, { target, props: {} })
  await tick()
  return target
}

describe('OperationLogDialog', () => {
  beforeEach(() => {
    closeOperationLog()
    document.body.innerHTML = ''
    getOperationLogDetailMock.mockReset()
    rollbackOperationMock.mockReset()
    rollbackOperationMock.mockResolvedValue({ inverseOpId: 'op-inverse' })
  })

  it('renders one grouped row per operation with a client-formatted summary', async () => {
    setEntries([
      opRow({ opId: 'op-copy', kind: 'copy', itemCount: 3 }),
      opRow({ opId: 'op-rename', kind: 'rename', itemCount: 214, initiator: 'aiClient' }),
    ])
    const target = await mountDialog()

    // Summaries are formatted client-side from kind + itemCount (ICU plural), so
    // the English catalog produces these exact strings, with a thousands separator.
    expect(target.textContent).toContain('Copied 3 items')
    expect(target.textContent).toContain('Renamed 214 items')
    // Provenance label from the typed initiator enum.
    expect(target.textContent).toContain('AI client')
    // One collapsible row per operation.
    expect(target.querySelectorAll('.op').length).toBe(2)
  })

  it('shows the ALPHA badge', async () => {
    setEntries([opRow({})])
    const target = await mountDialog()
    // StatusBadge renders the raw status text ('alpha'); CSS uppercases it visually.
    expect(target.querySelector('.feature-status-badge')?.textContent).toBe('alpha')
  })

  it('shows the empty state when no operations are logged', async () => {
    setEntries([])
    const target = await mountDialog()
    expect(target.querySelector('.notice')).not.toBeNull()
    expect(target.textContent).toContain('No operations yet')
  })

  it('expands a row to its per-item detail, fetched lazily over IPC', async () => {
    getOperationLogDetailMock.mockResolvedValue({
      operation: opRow({ opId: 'op-copy' }),
      items: [
        {
          seq: 0,
          entryType: 'file',
          rowRole: 'rollbackUnit',
          sourceVolumeId: 'root',
          sourcePath: '/left/file-a.txt',
          destVolumeId: 'root',
          destPath: '/right/file-a.txt',
          size: 10,
          mtime: null,
          outcome: 'done',
          overwrote: false,
          rollbackSkipReason: null,
        },
      ],
      totalItems: 1,
    })
    setEntries([opRow({ opId: 'op-copy' })])
    const target = await mountDialog()

    const head = target.querySelector<HTMLButtonElement>('.op-head')
    expect(head?.getAttribute('aria-expanded')).toBe('false')
    head?.click()
    // Let the click handler's await getOperationLogDetail settle.
    await vi.waitFor(() => {
      expect(target.textContent).toContain('/left/file-a.txt')
    })
    expect(getOperationLogDetailMock).toHaveBeenCalledWith({ operationId: 'op-copy', itemLimit: 200, itemOffset: 0 })
    expect(target.querySelector('.op-head')?.getAttribute('aria-expanded')).toBe('true')
    expect(target.textContent).toContain('/right/file-a.txt')
  })

  it('has no a11y violations with grouped rows rendered', async () => {
    setEntries([opRow({ opId: 'op-copy' }), opRow({ opId: 'op-del', kind: 'delete', itemCount: 5 })])
    const target = await mountDialog()
    await expectNoA11yViolations(target)
  })
})

/**
 * The per-row Roll back button. The reversal itself belongs to the operation queue
 * the moment the command returns, so what's pinned here is the route into it: who
 * gets a button, that the question always comes first, and that the confirmed press
 * names the right operation.
 */
describe('OperationLogDialog rollback', () => {
  beforeEach(() => {
    closeOperationLog()
    document.body.innerHTML = ''
    rollbackOperationMock.mockReset()
    rollbackOperationMock.mockResolvedValue({ inverseOpId: 'op-inverse' })
  })

  /** The Roll back buttons on the rows, in row order. */
  function rollbackButtons(target: HTMLElement): HTMLButtonElement[] {
    return [...target.querySelectorAll<HTMLButtonElement>('.op-row button.btn')]
  }

  /** Every button in the stacked confirmation, in DOM order (safe answer first). */
  function confirmButtons(): HTMLButtonElement[] {
    return [...document.querySelectorAll<HTMLButtonElement>('[data-dialog-id="rollback-confirmation"] button.btn')]
  }

  /** Answer the confirmation, failing loudly rather than silently doing nothing. */
  function answerConfirmation(label: string): void {
    const button = confirmButtons().find((b) => b.textContent.trim() === label)
    if (button === undefined) throw new Error(`no "${label}" button in the rollback confirmation`)
    button.click()
  }

  it('offers the button on exactly the rows the backend gate would let through', async () => {
    setEntries([
      opRow({ opId: 'op-can', rollbackState: 'rollbackable' }),
      opRow({ opId: 'op-part', rollbackState: 'partiallyRolledBack' }),
      opRow({ opId: 'op-cannot', rollbackState: 'notRollbackable' }),
      opRow({ opId: 'op-done', rollbackState: 'rolledBack' }),
      opRow({ opId: 'op-busy', rollbackState: 'rollingBack' }),
    ])
    const target = await mountDialog()

    // Two, and only two: `check_rollbackable` admits `rollbackable` and
    // `partiallyRolledBack` and refuses the other three, so offering a press on one
    // of those could only ever earn a refusal.
    const buttons = rollbackButtons(target)
    expect(buttons).toHaveLength(2)
    // Each says what ITS press does. A partly-reversed operation offered "Roll back"
    // would promise a fresh reversal of work that's already half undone.
    expect(buttons[0].textContent.trim()).toBe('Roll back')
    expect(buttons[1].textContent.trim()).toBe('Finish rolling back')
    // Described by its row: buttons sharing a name would otherwise be
    // indistinguishable to a screen reader.
    expect(buttons[0].getAttribute('aria-describedby')).toBe('op-head-op-can')
    // The partly-reversed row also points at the line explaining what became of its
    // files, so the button and its context are heard together.
    expect(buttons[1].getAttribute('aria-describedby')).toBe('op-head-op-part op-reason-op-part')
  })

  it('asks before it does anything, and dispatches nothing if the answer is no', async () => {
    setEntries([opRow({ opId: 'op-copy', kind: 'copy' })])
    const target = await mountDialog()

    rollbackButtons(target)[0].click()
    await tick()

    expect(document.querySelector('[data-dialog-id="rollback-confirmation"]')).not.toBeNull()
    expect(rollbackOperationMock).not.toHaveBeenCalled()

    // The safe answer comes first and holds focus, so a reflex Enter can't reverse anything.
    expect(confirmButtons()[0].textContent.trim()).toBe('Leave it as is')
    answerConfirmation('Leave it as is')
    await tick()

    expect(document.querySelector('[data-dialog-id="rollback-confirmation"]')).toBeNull()
    expect(rollbackOperationMock).not.toHaveBeenCalled()
  })

  it('words the question by what the rollback will DO, so undoing a move never reads as a delete', async () => {
    setEntries([opRow({ opId: 'op-move', kind: 'move' })])
    const target = await mountDialog()
    rollbackButtons(target)[0].click()
    await tick()

    const body = document.querySelector('#rollback-confirmation-body')?.textContent ?? ''
    expect(body).toContain('moves the files back where they came from')
    // The reversal of a move deletes nothing, so the copy must not say it does.
    expect(body).not.toContain('deletes')
  })

  it('fires the command with the row’s own id once confirmed, and marks the row rolling back', async () => {
    setEntries([opRow({ opId: 'op-first' }), opRow({ opId: 'op-second' })])
    const target = await mountDialog()

    rollbackButtons(target)[1].click()
    await tick()
    answerConfirmation('Roll back')

    await vi.waitFor(() => {
      expect(rollbackOperationMock).toHaveBeenCalledWith('op-second')
    })
    // The badge is the whole of the feedback: the reversal is the queue's from here.
    await vi.waitFor(() => {
      expect(target.textContent).toContain('Rolling back')
    })
    expect(rollbackOperationMock).toHaveBeenCalledTimes(1)
    // The button goes with the state change, so a second press can't double-dispatch.
    expect(rollbackButtons(target)).toHaveLength(1)
  })

  it('says WHY when the reversal is refused, rather than looking like nothing happened', async () => {
    rollbackOperationMock.mockRejectedValue(new RollbackRefusalFailure({ kind: 'alreadyRollingBack' }))
    setEntries([opRow({ opId: 'op-raced' })])
    const target = await mountDialog()

    rollbackButtons(target)[0].click()
    await tick()
    answerConfirmation('Roll back')

    await vi.waitFor(() => {
      expect(target.querySelector('.op-refusal')?.textContent).toContain('already rolling back')
    })
    // A lost race leaves the row exactly as it was, so the user can act on the notice.
    expect(rollbackButtons(target)).toHaveLength(1)
  })
})

describe('OperationLogDialog partly rolled back', () => {
  beforeEach(() => {
    closeOperationLog()
    document.body.innerHTML = ''
    rollbackOperationMock.mockReset()
    rollbackOperationMock.mockResolvedValue({ inverseOpId: 'op-inverse' })
  })

  function rollbackButtons(target: HTMLElement): HTMLButtonElement[] {
    return [...target.querySelectorAll<HTMLButtonElement>('.op-row button.btn')]
  }

  function confirmButtons(): HTMLButtonElement[] {
    return [...document.querySelectorAll<HTMLButtonElement>('[data-dialog-id="rollback-confirmation"] button.btn')]
  }

  /**
   * Cancelling a reversal is an ordinary thing to do, and the state it lands in used
   * to be a dead end: a badge, no button, and no words. Everything below is about
   * that person finding their way out.
   */
  it('says what became of the files, without making the user press anything', async () => {
    setEntries([opRow({ opId: 'op-part', kind: 'copy', rollbackState: 'partiallyRolledBack' })])
    const target = await mountDialog()

    expect(target.textContent).toContain('Partly rolled back')
    expect(target.querySelector('.op-reason')?.textContent.trim()).toBe(
      'Cmdr rolled back what it could and left the rest as it was. Finishing takes another pass and skips anything Cmdr still isn’t sure about.',
    )
  })

  it('frames the question as finishing the reversal, while the body still says what happens to the files', async () => {
    setEntries([opRow({ opId: 'op-part', kind: 'copy', rollbackState: 'partiallyRolledBack' })])
    const target = await mountDialog()

    rollbackButtons(target)[0].click()
    await tick()

    const title = document.querySelector('#rollback-confirmation-title')?.textContent.trim() ?? ''
    expect(title).toBe('Finish rolling this back?')
    // The kind-aware body is untouched: what this does to the files doesn't change
    // because half of it already happened.
    expect(document.querySelector('#rollback-confirmation-body')?.textContent).toContain(
      'deletes the files and folders the operation created',
    )
    // The confirming button repeats the words the row's button used, so the two
    // read as one action rather than two.
    expect(confirmButtons().map((b) => b.textContent.trim())).toEqual(['Leave it as is', 'Finish rolling back'])
  })

  it('picks the reversal back up on the same operation once confirmed', async () => {
    setEntries([
      opRow({ opId: 'op-fresh', rollbackState: 'rollbackable' }),
      opRow({ opId: 'op-part', rollbackState: 'partiallyRolledBack' }),
    ])
    const target = await mountDialog()

    rollbackButtons(target)[1].click()
    await tick()
    const confirm = confirmButtons().find((b) => b.textContent.trim() === 'Finish rolling back')
    if (confirm === undefined) throw new Error('no "Finish rolling back" button in the confirmation')
    confirm.click()

    // The same command as a first rollback: the engine re-attempts every item and
    // credits the ones already reversed, so finishing needs no separate entry point.
    await vi.waitFor(() => {
      expect(rollbackOperationMock).toHaveBeenCalledWith('op-part')
    })
    expect(rollbackOperationMock).toHaveBeenCalledTimes(1)
    await vi.waitFor(() => {
      expect(target.textContent).toContain('Rolling back')
    })
  })

  it('has no a11y violations with a partly-reversed row rendered', async () => {
    setEntries([
      opRow({ opId: 'op-part', rollbackState: 'partiallyRolledBack' }),
      opRow({ opId: 'op-can', rollbackState: 'rollbackable' }),
    ])
    const target = await mountDialog()
    await expectNoA11yViolations(target)
  })
})

describe('OperationLogDialog not-rollbackable reasons', () => {
  beforeEach(() => {
    closeOperationLog()
    document.body.innerHTML = ''
    rollbackOperationMock.mockReset()
  })

  /** The quiet explanatory lines under the rows, in list order. */
  function reasonLines(target: HTMLElement): string[] {
    return [...target.querySelectorAll('.op-reason')].map((el) => el.textContent.trim())
  }

  it('tells the user WHY a row can’t be rolled back, without making them press anything', async () => {
    // The button never appears on these rows, so the press-then-refuse path can't
    // reach them: the stored reason is the only way the user learns what happened.
    setEntries([
      opRow({
        opId: 'op-merge',
        kind: 'move',
        rollbackState: 'notRollbackable',
        notRollbackableReason: 'directoryMerge',
      }),
      opRow({
        opId: 'op-gone',
        kind: 'delete',
        rollbackState: 'notRollbackable',
        notRollbackableReason: 'permanentDelete',
      }),
    ])
    const target = await mountDialog()

    const lines = reasonLines(target)
    expect(lines).toHaveLength(2)
    expect(lines[0]).toBe(
      'This move merged the folder into one that was already there. Cmdr can’t tell which files came along and which were already inside, so there’s no safe way back.',
    )
    expect(lines[1]).toBe('A permanent delete leaves nothing to put back.')
  })

  it('gives every stored reason its own line', async () => {
    const reasons: NotRollbackableReason[] = [
      'overwrote',
      'permanentDelete',
      'archiveOverwrite',
      'zipEditUnsupported',
      'journalIncomplete',
      'directoryMerge',
      'stagedConflictResolved',
    ]
    setEntries(
      reasons.map((reason) =>
        opRow({ opId: `op-${reason}`, rollbackState: 'notRollbackable', notRollbackableReason: reason }),
      ),
    )
    const target = await mountDialog()

    const lines = reasonLines(target)
    expect(lines).toHaveLength(reasons.length)
    expect(new Set(lines).size).toBe(reasons.length)
    for (const line of lines) expect(line.length).toBeGreaterThan(0)
  })

  it('stays silent when no reason was recorded, rather than inventing one', async () => {
    // A fresh operation sits at `notRollbackable` with a NULL reason until finalize
    // decides. The badge already says "Can't roll back"; a dangling label would be worse
    // than nothing.
    setEntries([opRow({ opId: 'op-fresh', rollbackState: 'notRollbackable', notRollbackableReason: null })])
    const target = await mountDialog()

    expect(reasonLines(target)).toHaveLength(0)
    expect(target.textContent).toContain('Can’t roll back')
  })

  it('explains nothing on a row that CAN be rolled back', async () => {
    setEntries([
      opRow({ opId: 'op-can', rollbackState: 'rollbackable' }),
      opRow({ opId: 'op-done', rollbackState: 'rolledBack' }),
    ])
    const target = await mountDialog()

    expect(reasonLines(target)).toHaveLength(0)
  })
})
