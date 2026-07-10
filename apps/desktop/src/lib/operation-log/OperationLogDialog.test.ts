/**
 * Component + a11y tests for `OperationLogDialog.svelte`: the alpha operation-log
 * dialog renders one grouped row per operation with a client-formatted summary,
 * carries the ALPHA badge, and expands a row to its per-item detail (fetched
 * lazily over IPC). Paging state itself is covered in `operation-log-trigger.test.ts`.
 */

import { describe, it, vi, expect, beforeEach } from 'vitest'
import { mount, tick } from 'svelte'
import type { OperationRow } from '$lib/ipc/bindings'
import type { OperationLogDetail } from '$lib/tauri-commands'
import OperationLogDialog from './OperationLogDialog.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'
import { operationLogState, closeOperationLog } from './operation-log-trigger.svelte'

const getOperationLogDetailMock = vi.fn<(id: string, l: number, o: number) => Promise<OperationLogDetail | null>>()
vi.mock('$lib/tauri-commands', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
  getRecentOperationLogEntries: vi.fn(() => Promise.resolve([])),
  getOperationLogDetail: (id: string, l: number, o: number) => getOperationLogDetailMock(id, l, o),
}))

// Avoid pulling the reactive-settings chain; a stable stamp is all the row needs.
vi.mock('$lib/settings/reactive-settings.svelte', () => ({
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
    getOperationLogDetailMock.mockReset()
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
    expect(getOperationLogDetailMock).toHaveBeenCalledWith('op-copy', 200, 0)
    expect(target.querySelector('.op-head')?.getAttribute('aria-expanded')).toBe('true')
    expect(target.textContent).toContain('/right/file-a.txt')
  })

  it('has no a11y violations with grouped rows rendered', async () => {
    setEntries([opRow({ opId: 'op-copy' }), opRow({ opId: 'op-del', kind: 'delete', itemCount: 5 })])
    const target = await mountDialog()
    await expectNoA11yViolations(target)
  })
})
