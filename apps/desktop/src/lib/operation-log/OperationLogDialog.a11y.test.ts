/**
 * Tier 3 a11y tests for `OperationLogDialog.svelte`.
 *
 * The alpha "Operation log" dialog: a newest-first list of file operations, each an
 * expandable button (`aria-expanded` + `aria-controls`) that reveals its per-item
 * rows. Covers the empty state, a populated collapsed list, and an expanded operation
 * (whose revealed region must satisfy the `aria-controls` reference).
 */

import { describe, it, expect, afterEach, vi } from 'vitest'
import { mount, tick } from 'svelte'
import OperationLogDialog from './OperationLogDialog.svelte'
import { operationLogState } from './operation-log-trigger.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'
import type { OperationRow, OperationItemView } from '$lib/ipc/bindings'

const getDetailMock = vi.fn((_id: string, _limit: number, _offset: number) =>
  Promise.resolve({ operation: op('op-1', 'move'), items: [item(0), item(1)], totalItems: 2 }),
)

vi.mock('$lib/tauri-commands', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
  getRecentOperationLogEntries: vi.fn(() => Promise.resolve([])),
  getOperationLogDetail: (id: string, limit: number, offset: number) => getDetailMock(id, limit, offset),
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
