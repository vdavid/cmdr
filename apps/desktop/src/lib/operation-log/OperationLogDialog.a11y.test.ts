/**
 * Tier 3 a11y tests for `OperationLogDialog.svelte`.
 *
 * The alpha "Operation log" dialog: a newest-first list of file operations, each an
 * expandable button (`aria-expanded` + `aria-controls`) that reveals its per-item
 * rows, and each reversible one carrying a Roll back button beside it. Covers the
 * empty state, a populated collapsed list, an expanded operation (whose revealed
 * region must satisfy the `aria-controls` reference), the confirmation stacked over
 * the log, the refusal notice a lost race leaves on a row, and the stored reason a
 * not-rollbackable row explains itself with.
 */

import { describe, it, expect, afterEach, vi } from 'vitest'
import { mount, tick } from 'svelte'
import OperationLogDialog from './OperationLogDialog.svelte'
import { operationLogState } from './operation-log-trigger.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'
import type { OperationRow, OperationItemView } from '$lib/ipc/bindings'
import { RollbackRefusalFailure } from './rollback-refusal'

const getDetailMock = vi.fn((_id: string, _limit: number, _offset: number) =>
  Promise.resolve({ operation: op('op-1', 'move'), items: [item(0), item(1)], totalItems: 2 }),
)
const rollbackMock = vi.fn((_id: string) => Promise.resolve({ inverseOpId: 'op-inverse' }))

vi.mock('$lib/tauri-commands', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
  getRecentOperationLogEntries: vi.fn(() => Promise.resolve([])),
  getOperationLogDetail: (id: string, limit: number, offset: number) => getDetailMock(id, limit, offset),
  rollbackOperation: (id: string) => rollbackMock(id),
}))

function op(opId: string, kind: OperationRow['kind']): OperationRow {
  return {
    opId,
    kind,
    archiveSubkind: null,
    initiator: 'user',
    executionStatus: 'done',
    rollbackState: 'rollbackable',
    notRollbackableReason: null,
    rollsBackOpId: null,
    inverseOpId: null,
    sourceVolumeId: 'root',
    destVolumeId: 'root',
    startedAt: 1_700_000_000_000,
    endedAt: 1_700_000_005_000,
    itemCount: 2,
    itemsDone: 2,
    bytesTotal: 4096,
    searchCoverage: 'full',
    searchCoverageReason: null,
    devSummary: null,
  }
}

function item(seq: number): OperationItemView {
  return {
    seq,
    entryType: 'file',
    rowRole: 'rollbackUnit',
    sourceVolumeId: 'root',
    sourcePath: `/Users/me/Documents/report-${String(seq)}.pdf`,
    destVolumeId: 'root',
    destPath: `/Volumes/Backup/report-${String(seq)}.pdf`,
    size: 2048,
    mtime: 1_700_000_000_000,
    outcome: 'done',
    overwrote: false,
    rollbackSkipReason: null,
  }
}

function resetState(entries: OperationRow[]): void {
  operationLogState.open = true
  operationLogState.entries = entries
  operationLogState.loading = false
  operationLogState.loadError = false
  operationLogState.hasMore = false
  operationLogState.loadingMore = false
}

afterEach(() => {
  document.body.innerHTML = ''
  resetState([])
  operationLogState.open = false
})

function mountDialog(): HTMLElement {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(OperationLogDialog, { target, props: {} })
  return target
}

describe('OperationLogDialog a11y', () => {
  it('the empty state has no a11y violations', async () => {
    resetState([])
    const target = mountDialog()
    await tick()
    await expectNoA11yViolations(target)
  })

  it('a populated, collapsed list has no a11y violations', async () => {
    resetState([op('op-1', 'move'), op('op-2', 'copy')])
    const target = mountDialog()
    await tick()
    await expectNoA11yViolations(target)
  })

  /** The row's Roll back button: the one control on a row that isn't the row itself. */
  function rollbackButton(target: HTMLElement): HTMLButtonElement | null {
    return target.querySelector<HTMLButtonElement>('.op-row button.btn')
  }

  /** Press Roll back and let the stacked confirmation mount. */
  async function openConfirmation(target: HTMLElement): Promise<void> {
    const button = rollbackButton(target)
    if (button === null) throw new Error('expected a Roll back button on a rollbackable row')
    button.click()
    await vi.waitFor(() => {
      expect(document.querySelector('[data-dialog-id="rollback-confirmation"]')).not.toBeNull()
    })
    await tick()
  }

  it('the rollback confirmation stacked over the log has no a11y violations', async () => {
    resetState([op('op-1', 'move')])
    mountDialog()
    await tick()
    // Both dialogs are in the document at once, so audit the whole body: a stacked
    // dialog is exactly where duplicate ids and orphaned `aria-describedby` show up.
    await openConfirmation(document.body)
    await expectNoA11yViolations(document.body)
  })

  it('a refusal notice on a row has no a11y violations', async () => {
    rollbackMock.mockRejectedValueOnce(new RollbackRefusalFailure({ kind: 'alreadyRollingBack' }))
    resetState([op('op-1', 'copy')])
    const target = mountDialog()
    await tick()
    await openConfirmation(document.body)

    const confirm = [...document.querySelectorAll<HTMLButtonElement>('[data-dialog-id="rollback-confirmation"] button')]
    const rollBack = confirm.find((b) => b.textContent.trim() === 'Roll back')
    if (rollBack === undefined) throw new Error('expected a Roll back button in the confirmation')
    rollBack.click()
    await vi.waitFor(() => {
      expect(target.querySelector('.op-refusal')).not.toBeNull()
    })

    await expectNoA11yViolations(target)
  })

  it('a row explaining why it can’t be rolled back has no a11y violations, and the row points at it', async () => {
    const merged = { ...op('op-merge', 'move'), rollbackState: 'notRollbackable' as const }
    resetState([{ ...merged, notRollbackableReason: 'directoryMerge' as const }])
    const target = mountDialog()
    await tick()

    // The explanation is `aria-describedby` from the row's own button, so a screen
    // reader hears "Can't roll back" and the reason together instead of meeting an
    // orphaned paragraph after the row it belongs to.
    const head = target.querySelector<HTMLButtonElement>('.op-head')
    expect(head?.getAttribute('aria-describedby')).toBe('op-reason-op-merge')
    expect(target.querySelector('#op-reason-op-merge')).not.toBeNull()
    await expectNoA11yViolations(target)
  })

  it('a row with no recorded reason leaves no dangling description behind', async () => {
    resetState([{ ...op('op-fresh', 'copy'), rollbackState: 'notRollbackable' as const }])
    const target = mountDialog()
    await tick()

    // An `aria-describedby` pointing at an element that was never rendered is exactly
    // the violation the axe run below catches; the attribute has to be absent, not empty.
    expect(target.querySelector('.op-head')?.hasAttribute('aria-describedby')).toBe(false)
    await expectNoA11yViolations(target)
  })

  it('an expanded operation (with revealed item rows) has no a11y violations', async () => {
    resetState([op('op-1', 'move')])
    const target = mountDialog()
    await tick()

    const head = target.querySelector<HTMLButtonElement>('.op-head')
    if (head === null) throw new Error('expected an .op-head button to be rendered')
    head.click()
    await vi.waitFor(() => {
      expect(target.querySelector('.item-list')).not.toBeNull()
    })
    await tick()

    await expectNoA11yViolations(target)
  })
})
