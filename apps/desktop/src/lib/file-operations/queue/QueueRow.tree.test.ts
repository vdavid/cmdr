/**
 * The live transfer tree inline in a queue row: which rows show it, the collapse
 * toggle that appears once two rows do, and how keyboard focus moves between the
 * row and its tree. The tree's own behavior is `../transfer-tree/`'s to test;
 * that it pulls on each tick needs a session, so it's in `queue-row-session.svelte.test.ts`.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount, unmount, flushSync, tick, type ComponentProps } from 'svelte'
import QueueRow from './QueueRow.svelte'
import type { OperationRow } from './operations-store.svelte'
import type { OperationSnapshot, WriteProgressEvent } from '#lib/ipc/bindings.js'
import type { TransferTreeAnswer, TransferTreeViewRequest } from '#lib/tauri-commands/index.js'
import { oneCopyingFile } from '../transfer-tree/fixtures/one-copying-file'

const getTransferTreeView = vi.hoisted(() =>
  vi.fn<(operationId: string, request: TransferTreeViewRequest) => Promise<TransferTreeAnswer>>(),
)
vi.mock('#lib/tauri-commands/index.js', () => ({
  requestForegroundOperation: vi.fn(() => Promise.resolve()),
  retryFailedOperation: vi.fn(() => Promise.resolve({ kind: 'started', operationId: 'op-2' })),
  getTransferTreeView,
}))
vi.mock('#lib/settings/reactive-settings.svelte.js', () => ({ getFileSizeFormat: () => 'decimal' }))
vi.mock('#lib/error-reporter/breadcrumbs.js', () => ({ recordBreadcrumb: vi.fn() }))

function progress(phase: WriteProgressEvent['phase']): WriteProgressEvent {
  return {
    operationId: 'op-1',
    operationType: 'copy',
    phase,
    currentFile: 'holiday.mov',
    filesDone: 1,
    filesTotal: 4,
    bytesDone: 1_000,
    bytesTotal: 16_000,
  }
}

function row(
  over: Partial<OperationSnapshot> = {},
  phase: WriteProgressEvent['phase'] | null = 'copying',
): OperationRow {
  return {
    snapshot: {
      operationId: 'op-1',
      operationType: 'copy',
      status: 'running',
      source: '/Users/me/Movies',
      destination: '/Volumes/Backup',
      rollbackSupport: 'supported',
      reverses: null,
      error: null,
      canRetry: false,
      conflictPolicy: null,
      startedAt: null,
      items: null,
      ...over,
    },
    progress: phase === null ? null : progress(phase),
  }
}

let target: HTMLElement
let instance: ReturnType<typeof mount> | undefined

async function flushPromises(): Promise<void> {
  for (let i = 0; i < 5; i++) {
    await new Promise<void>((resolve) => {
      setTimeout(resolve, 0)
    })
    await tick()
  }
}

/** Mounted in a grid, as the page does, with the row as the grid's Tab stop unless told otherwise. */
async function render(props: Partial<ComponentProps<typeof QueueRow>> & { row: OperationRow }) {
  target = document.createElement('div')
  target.setAttribute('role', 'grid')
  document.body.appendChild(target)
  instance = mount(QueueRow, {
    target,
    props: { selected: false, tabbable: true, onToggleSelect: () => {}, onDismiss: () => {}, ...props },
  })
  flushSync()
  await flushPromises()
}

const treeCell = (): HTMLElement | null => target.querySelector('[data-queue-tree]')
const toggle = (): HTMLButtonElement | null =>
  target.querySelector<HTMLButtonElement>('[aria-label="Files in this operation"]')

beforeEach(() => {
  document.body.innerHTML = ''
  getTransferTreeView.mockReset()
  getTransferTreeView.mockResolvedValue(oneCopyingFile)
})

afterEach(() => {
  if (instance) void unmount(instance)
  instance = undefined
})

describe('QueueRow: the inline tree', () => {
  it('shows a running copy’s tree inside its own row, under the readout', async () => {
    await render({ row: row() })
    const cell = treeCell()
    const rowEl = target.querySelector('[role="row"]')
    expect(cell?.getAttribute('role')).toBe('gridcell')
    expect(cell?.parentElement).toBe(rowEl)
    // After the readout, so it reads as this operation's detail.
    const readout = rowEl?.querySelector('.readout-cell')
    expect(readout?.compareDocumentPosition(cell as Node)).toBe(Node.DOCUMENT_POSITION_FOLLOWING)
    expect(cell?.querySelector('[role="tree"]')).not.toBeNull()
    expect(getTransferTreeView.mock.calls[0]?.[0]).toBe('op-1')
  })

  it('shows it on a paused row too: the work is still there to look at', async () => {
    await render({ row: row({ status: 'paused' }) })
    expect(treeCell()).not.toBeNull()
  })

  it('shows none on a waiting, failed, scanning, delete, or reversing row', async () => {
    const cases: OperationRow[] = [
      row({ status: 'queued' }, null),
      row({ status: 'failed', error: { type: 'io_error', path: '/Users/me/Movies/a.mov', message: 'gone' } }, null),
      row({}, 'scanning'),
      row({ operationType: 'delete' }, 'deleting'),
      row({}, 'rolling_back'),
    ]
    for (const r of cases) {
      await render({ row: r })
      expect(treeCell()).toBeNull()
      if (instance) void unmount(instance)
      instance = undefined
    }
    expect(getTransferTreeView).not.toHaveBeenCalled()
  })

  it('offers no toggle on a lone tree: it’s always open', async () => {
    await render({ row: row() })
    expect(toggle()).toBeNull()
  })

  it('with two trees in the list, a toggle shows and hides this one, open by default', async () => {
    await render({ row: row(), treeCollapsible: true })
    const button = toggle()
    expect(button?.getAttribute('aria-expanded')).toBe('true')
    expect(button?.getAttribute('aria-controls')).toBe(treeCell()?.id)
    // Joins the row's ← / → walk, out of the Tab order like every row button.
    expect(button?.closest('.actions-cell')).not.toBeNull()
    expect(button?.tabIndex).toBe(-1)

    button?.click()
    flushSync()
    expect(treeCell()).toBeNull()
    expect(button?.getAttribute('aria-expanded')).toBe('false')

    button?.click()
    await flushPromises()
    expect(treeCell()).not.toBeNull()
  })

  it('opens a shut tree again once it’s the only one left', async () => {
    await render({ row: row(), treeCollapsible: true })
    toggle()?.click()
    flushSync()
    expect(treeCell()).toBeNull()

    await render({ row: row(), treeCollapsible: false })
    expect(treeCell()).not.toBeNull()
  })
})

describe('QueueRow: focus between the row and its tree', () => {
  it('lets Tab into the tree from the grid’s cursor row only', async () => {
    await render({ row: row(), tabbable: true })
    expect(treeCell()?.querySelector('[role="treeitem"]')?.getAttribute('tabindex')).toBe('0')
    if (instance) void unmount(instance)

    await render({ row: row(), tabbable: false })
    expect(treeCell()?.querySelectorAll('[tabindex="0"]')).toHaveLength(0)
  })

  it('Escape inside the tree hands focus back to the row, and keeps the window open', async () => {
    await render({ row: row() })
    const item = treeCell()?.querySelector<HTMLElement>('[role="treeitem"]')
    item?.focus()
    const escape = new KeyboardEvent('keydown', { key: 'Escape', code: 'Escape', bubbles: true, cancelable: true })
    item?.dispatchEvent(escape)
    expect(escape.defaultPrevented).toBe(true)
    expect(document.activeElement).toBe(target.querySelector('[role="row"]'))
  })

  it('resumes a paused tree on F from the row itself, where a wheel leaves focus', async () => {
    await render({ row: row() })
    const press = (on: Element, init: KeyboardEventInit) => {
      on.dispatchEvent(new KeyboardEvent('keydown', { bubbles: true, cancelable: true, ...init }))
    }
    const item = treeCell()?.querySelector<HTMLElement>('[role="treeitem"]')
    if (!item) throw new Error('no tree row')
    item.focus()
    press(item, { key: 'ArrowDown', code: 'ArrowDown' })
    await flushPromises()
    expect(treeCell()?.querySelector('.follow button')).not.toBeNull()

    const rowEl = target.querySelector<HTMLElement>('[role="row"]')
    if (!rowEl) throw new Error('no row')
    rowEl.focus()
    press(rowEl, { key: 'f', code: 'KeyF' })
    await flushPromises()
    expect(treeCell()?.querySelector('.follow button')).toBeNull()
  })

  it('leaves other keys inside the tree to the tree', async () => {
    await render({ row: row() })
    const item = treeCell()?.querySelector<HTMLElement>('[role="treeitem"]')
    item?.focus()
    const space = new KeyboardEvent('keydown', { key: ' ', code: 'Space', bubbles: true, cancelable: true })
    item?.dispatchEvent(space)
    expect(space.defaultPrevented).toBe(false)
    expect(document.activeElement).toBe(item)
  })
})
