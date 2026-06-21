import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, unmount, flushSync, type ComponentProps } from 'svelte'
import QueueRow from './QueueRow.svelte'
import type { OperationRow } from './operations-store.svelte'
import type { OperationSnapshot, WriteProgressEvent } from '$lib/ipc/bindings'

// The component reads reactive settings (file-size format) deep in `<Size>`. The
// real path needs the settings store; stub the format getter to keep the unit
// test isolated.
vi.mock('$lib/settings/reactive-settings.svelte', () => ({
  getFileSizeFormat: () => 'decimal',
}))

function buildRow(
  status: OperationSnapshot['status'],
  opType: OperationSnapshot['operationType'] = 'copy',
  progress: WriteProgressEvent | null = null,
): OperationRow {
  return {
    snapshot: {
      operationId: 'op-1',
      operationType: opType,
      status,
      source: '/Users/me/Documents/report.pdf',
      destination: '/Volumes/Backup/report.pdf',
    },
    progress,
  }
}

let target: HTMLElement
let instance: ReturnType<typeof mount> | undefined

function render(props: ComponentProps<typeof QueueRow>) {
  target = document.createElement('ul')
  document.body.appendChild(target)
  instance = mount(QueueRow, { target, props })
  flushSync()
}

beforeEach(() => {
  document.body.innerHTML = ''
  instance = undefined
})

describe('QueueRow', () => {
  it('shows Pause for a running op and Resume for a paused op', () => {
    const onPauseResume = vi.fn()
    render({ row: buildRow('running'), selected: false, onToggleSelect: () => {}, onPauseResume, onCancel: () => {} })
    expect(target.querySelector('[aria-label="Pause this transfer"]')).not.toBeNull()
    expect(target.querySelector('[aria-label="Resume this transfer"]')).toBeNull()
    if (instance) void unmount(instance)

    render({ row: buildRow('paused'), selected: false, onToggleSelect: () => {}, onPauseResume, onCancel: () => {} })
    expect(target.querySelector('[aria-label="Resume this transfer"]')).not.toBeNull()
    expect(target.querySelector('[aria-label="Pause this transfer"]')).toBeNull()
  })

  it('a queued op has Cancel but no Pause/Resume', () => {
    render({
      row: buildRow('queued'),
      selected: false,
      onToggleSelect: () => {},
      onPauseResume: () => {},
      onCancel: () => {},
    })
    expect(target.querySelector('[aria-label="Cancel this transfer"]')).not.toBeNull()
    expect(target.querySelector('[aria-label="Pause this transfer"]')).toBeNull()
    expect(target.querySelector('[aria-label="Resume this transfer"]')).toBeNull()
  })

  it('clicking Pause fires onPauseResume; clicking Cancel fires onCancel', () => {
    const onPauseResume = vi.fn()
    const onCancel = vi.fn()
    render({ row: buildRow('running'), selected: false, onToggleSelect: () => {}, onPauseResume, onCancel })

    const pauseBtn = target.querySelector<HTMLButtonElement>('[aria-label="Pause this transfer"]')
    pauseBtn?.click()
    expect(onPauseResume).toHaveBeenCalledOnce()

    const cancelBtn = target.querySelector<HTMLButtonElement>('[aria-label="Cancel this transfer"]')
    cancelBtn?.click()
    expect(onCancel).toHaveBeenCalledOnce()
  })

  it('the select checkbox reflects `selected` and fires onToggleSelect', () => {
    const onToggleSelect = vi.fn()
    render({ row: buildRow('running'), selected: true, onToggleSelect, onPauseResume: () => {}, onCancel: () => {} })
    const checkbox = target.querySelector<HTMLInputElement>('input[type="checkbox"]')
    expect(checkbox?.checked).toBe(true)
    checkbox?.click()
    expect(onToggleSelect).toHaveBeenCalledOnce()
  })

  it('renders a progress bar from a live write-progress event for a running op', () => {
    const progress: WriteProgressEvent = {
      operationId: 'op-1',
      operationType: 'copy',
      phase: 'copying',
      currentFile: 'report.pdf',
      filesDone: 1,
      filesTotal: 4,
      bytesDone: 25,
      bytesTotal: 100,
    }
    render({
      row: buildRow('running', 'copy', progress),
      selected: false,
      onToggleSelect: () => {},
      onPauseResume: () => {},
      onCancel: () => {},
    })
    const bar = target.querySelector('[role="progressbar"]')
    expect(bar).not.toBeNull()
    expect(bar?.getAttribute('aria-valuenow')).toBe('25')
  })

  it('exposes the lifecycle status as a data attribute for E2E', () => {
    render({
      row: buildRow('queued'),
      selected: false,
      onToggleSelect: () => {},
      onPauseResume: () => {},
      onCancel: () => {},
    })
    expect(target.querySelector('[data-status="queued"]')).not.toBeNull()
    expect(target.querySelector('[data-operation-id="op-1"]')).not.toBeNull()
  })
})
