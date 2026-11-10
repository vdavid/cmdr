/**
 * Tier 3 a11y tests for `OperationLogDialog.svelte`.
 *
 * The master–detail Operation log: a `grid` of operations on the left, the picked one's
 * header and `treegrid` of items on the right, and the action bar. Covers the empty
 * state, a populated list with its tree, a tree with folder units and skipped rows (row
 * descriptions must point at rendered elements), a read-only tree, the confirmation
 * stacked over the log, the refusal notice a lost race leaves, and the stored reason a
 * not-rollbackable operation explains itself with.
 */

import { describe, it, expect, afterEach, beforeEach, vi } from 'vitest'
import { mount, tick } from 'svelte'
import OperationLogDialog from './OperationLogDialog.svelte'
import OperationList from './OperationList.svelte'
import OperationDetail from './OperationDetail.svelte'
import OperationTree from './OperationTree.svelte'
import { createOperationTreeSource } from './operation-tree-source.svelte'
import { operationLogState } from './operation-log-trigger.svelte'
import { expectNoA11yViolations } from '#lib/test-a11y.js'
import type { OperationRow, OperationTreeView } from '#lib/ipc/bindings.js'
import { RollbackRefusalFailure } from './rollback-refusal'
import { fileRow, folderRow, treeView } from './operation-tree-test-fixtures'

const rollbackMock = vi.fn((_id: string, _initiator: string, _selection: unknown) =>
  Promise.resolve({ inverseOpId: 'op-inverse' }),
)
const treeMock = vi.fn((_id: string, _request: unknown): Promise<OperationTreeView | null> =>
  Promise.resolve(treeView([fileRow(1)])),
)

vi.mock('#lib/tauri-commands/index.js', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
  getRecentOperationLogEntries: vi.fn(() => Promise.resolve([])),
  getOperationLogDetail: vi.fn(() => Promise.resolve(null)),
  rollbackOperation: (id: string, initiator: string, selection: unknown) => rollbackMock(id, initiator, selection),
}))

vi.mock('#lib/tauri-commands/operation-log.js', () => ({
  getOperationLogTree: (id: string, request: unknown) => treeMock(id, request),
  startOperationLogTreeBackfill: vi.fn(() => Promise.resolve({ kind: 'started' })),
  cancelOperationLogTreeBackfill: vi.fn(() => Promise.resolve()),
  onOperationLogTreeBackfill: vi.fn(() => Promise.resolve(() => {})),
  onOperationLogTreeIndexesSettled: vi.fn(() => Promise.resolve(() => {})),
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

function resetState(entries: OperationRow[]): void {
  operationLogState.open = true
  operationLogState.entries = entries
  operationLogState.loading = false
  operationLogState.loadError = false
  operationLogState.hasMore = false
  operationLogState.loadingMore = false
}

beforeEach(() => {
  treeMock.mockReset()
  treeMock.mockResolvedValue(treeView([fileRow(1)]))
})

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

async function mountWithTree(entries: OperationRow[]): Promise<HTMLElement> {
  resetState(entries)
  const target = mountDialog()
  await vi.waitFor(() => {
    expect(target.querySelector('.tree .column-list-row.is-data')).not.toBeNull()
  })
  await tick()
  return target
}

/** The action bar's rollback button. */
function rollbackButton(): HTMLButtonElement | null {
  return (
    [...document.querySelectorAll<HTMLButtonElement>('.op-actions button.btn')].find((b) =>
      b.textContent.trim().startsWith('Roll back'),
    ) ?? null
  )
}

async function openConfirmation(): Promise<void> {
  const button = rollbackButton()
  if (button === null) throw new Error('expected a Roll back button on a rollbackable operation')
  button.click()
  await vi.waitFor(() => {
    expect(document.querySelector('[data-dialog-id="rollback-confirmation"]')).not.toBeNull()
  })
  await tick()
}

describe('OperationLogDialog a11y', () => {
  it('the empty state has no a11y violations', async () => {
    resetState([])
    const target = mountDialog()
    await tick()
    await expectNoA11yViolations(target)
  })

  it('tabs through the list, Load more, the tree, and the action bar, in that order', async () => {
    resetState([op('op-1', 'copy')])
    operationLogState.hasMore = true
    const target = mountDialog()
    await vi.waitFor(() => {
      expect(target.querySelector('.tree .column-list-row.is-data')).not.toBeNull()
    })
    await tick()
    const dialog = document.querySelector('[data-dialog-id="operation-log"]')
    if (dialog === null) throw new Error('no dialog')
    // The Tab order is the DOM order of what takes focus: no stop sets a positive tabindex.
    const stops = [
      ...dialog.querySelectorAll<HTMLElement>('button:not([disabled]), input:not([disabled]), [tabindex]'),
    ].filter((element) => element.tabIndex >= 0)
    expect(stops.every((element) => element.tabIndex === 0)).toBe(true)
    const names = stops.map((element) => {
      const role = element.getAttribute('role')
      if (role === 'grid' || role === 'treegrid') return role
      return (element.querySelector('.btn-label') ?? element).textContent.trim() || element.getAttribute('aria-label')
    })
    expect(names).toEqual(['grid', 'Load 50 more', 'treegrid', 'Close', 'Roll back selected (1)'])
  })

  it('a populated list with its item tree has no a11y violations', async () => {
    const target = await mountWithTree([op('op-1', 'move'), op('op-2', 'copy')])
    expect(target.querySelector('[role="grid"]')).not.toBeNull()
    expect(target.querySelector('[role="treegrid"]')).not.toBeNull()
    await expectNoA11yViolations(target)
  })

  it('a tree with folder units, contents, and skipped rows has no a11y violations', async () => {
    treeMock.mockResolvedValue(
      treeView([
        folderRow(1, { label: 'Invoices', role: 'selectable', expanded: true }),
        fileRow(10, { level: 2, role: 'contents' }),
        fileRow(11, { level: 2, role: 'contents' }),
        fileRow(12, { outcome: 'skipped', skipReason: 'drift' }),
        folderRow(2, { expandable: false, unexpandable: 'capped', role: 'selectable' }),
      ]),
    )
    const target = await mountWithTree([op('op-trash', 'trash')])
    // Every row description names an element that's really there.
    for (const row of target.querySelectorAll('.tree [aria-describedby]')) {
      const id = row.getAttribute('aria-describedby') ?? ''
      for (const part of id.split(' ')) expect(document.getElementById(part)).not.toBeNull()
    }
    await expectNoA11yViolations(target)
  })

  it('a read-only tree has no a11y violations', async () => {
    treeMock.mockResolvedValue(treeView([fileRow(1, { checkbox: 'none' })], { readOnly: true }))
    const target = await mountWithTree([
      { ...op('op-del', 'delete'), rollbackState: 'notRollbackable', notRollbackableReason: 'permanentDelete' },
    ])
    await expectNoA11yViolations(target)
  })

  it('the rollback confirmation stacked over the log has no a11y violations', async () => {
    await mountWithTree([op('op-1', 'move')])
    // Both dialogs are in the document at once, so audit the whole body: a stacked
    // dialog is exactly where duplicate ids and orphaned `aria-describedby` show up.
    await openConfirmation()
    await expectNoA11yViolations(document.body)
  })

  it('a refusal notice has no a11y violations', async () => {
    rollbackMock.mockRejectedValueOnce(new RollbackRefusalFailure({ kind: 'alreadyRollingBack' }))
    const target = await mountWithTree([op('op-1', 'copy')])
    await openConfirmation()

    const confirm = [...document.querySelectorAll<HTMLButtonElement>('[data-dialog-id="rollback-confirmation"] button')]
    const rollBack = confirm.find((b) => b.textContent.trim() === 'Roll back')
    if (rollBack === undefined) throw new Error('expected a Roll back button in the confirmation')
    rollBack.click()
    await vi.waitFor(() => {
      expect(target.querySelector('.op-refusal')).not.toBeNull()
    })

    await expectNoA11yViolations(target)
  })

  it('an operation explaining why it can’t be rolled back has no a11y violations', async () => {
    const merged = { ...op('op-merge', 'move'), rollbackState: 'notRollbackable' as const }
    treeMock.mockResolvedValue(treeView([fileRow(1, { checkbox: 'none' })], { readOnly: true }))
    const target = await mountWithTree([{ ...merged, notRollbackableReason: 'directoryMerge' as const }])
    expect(target.querySelector('#op-reason-op-merge')).not.toBeNull()
    await expectNoA11yViolations(target)
  })

  it('a rollback button names its operation, and the sentence beside it when there is one', async () => {
    await mountWithTree([op('op-copy', 'copy')])
    // The rollbackable notice says what the press does; the button is heard with it.
    expect(rollbackButton()?.getAttribute('aria-describedby')).toBe('op-head-op-copy op-reason-op-copy')
  })
})

/** The panes on their own, as a host other than this dialog would mount them. */
describe('the Operation log panes a11y', () => {
  function host(): HTMLElement {
    const target = document.createElement('div')
    document.body.appendChild(target)
    return target
  }

  it('the operation list alone has no a11y violations', async () => {
    const target = host()
    mount(OperationList, {
      target,
      props: {
        entries: [op('op-1', 'copy'), { ...op('op-2', 'move'), executionStatus: 'canceled' }],
        cursorIndex: 0,
        onCursorChange: () => {},
        hasMore: true,
        loadingMore: false,
        onLoadMore: () => {},
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('the detail pane and its tree alone have no a11y violations', async () => {
    const source = createOperationTreeSource({ settleMs: 0 })
    const row = op('op-1', 'copy')
    source.show(row)
    const target = host()
    mount(OperationDetail, { target, props: { op: row, source, refusal: null } })
    await vi.waitFor(() => {
      expect(target.querySelector('.tree .column-list-row.is-data')).not.toBeNull()
    })
    await expectNoA11yViolations(target)
    source.dispose()
  })

  it('the tree alone, still loading, has no a11y violations', async () => {
    treeMock.mockReturnValue(new Promise(() => {}))
    const source = createOperationTreeSource({ settleMs: 0, spinnerDelayMs: 0 })
    source.show(op('op-1', 'copy'))
    const target = host()
    mount(OperationTree, { target, props: { source, kind: 'copy' } })
    await vi.waitFor(() => {
      expect(target.querySelector('.tree-blank .spinner')).not.toBeNull()
    })
    await expectNoA11yViolations(target)
    source.dispose()
  })
})
