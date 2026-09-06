/**
 * The main window's failure toast: the real reason, and a way to the queue.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, flushSync } from 'svelte'
import type { OperationSnapshot } from '$lib/ipc/bindings'

const openQueueWindow = vi.fn(() => Promise.resolve())
vi.mock('$lib/file-operations/queue/queue-window', () => ({
  openQueueWindow: (): Promise<void> => openQueueWindow(),
}))

const dismissToast = vi.fn<(id: string) => void>()
vi.mock('$lib/ui/toast', () => ({
  dismissToast: (id: string): void => {
    dismissToast(id)
  },
}))

import OperationFailedToastContent from './OperationFailedToastContent.svelte'

function snapshot(over: Partial<OperationSnapshot> = {}): OperationSnapshot {
  return {
    operationId: 'op-1',
    operationType: 'copy',
    status: 'failed',
    source: '/Users/me/Documents/report.pdf',
    destination: '/Volumes/Backup',
    supportsRollback: false,
    reverses: null,
    error: { type: 'read_only_device', path: '/Volumes/Backup', deviceName: 'Backup', side: 'destination' },
    ...over,
  }
}

let target: HTMLElement

function render(over: Partial<OperationSnapshot> = {}): void {
  target = document.createElement('div')
  document.body.appendChild(target)
  mount(OperationFailedToastContent, { target, props: { toastId: 'toast-1', snapshot: snapshot(over) } })
  flushSync()
}

beforeEach(() => {
  document.body.innerHTML = ''
  openQueueWindow.mockClear()
  dismissToast.mockClear()
})

describe('OperationFailedToastContent', () => {
  it('names the operation in the house wording, never "failed"', () => {
    render()
    const title = target.querySelector('.title')?.textContent.trim() ?? ''
    expect(title).toBe("Couldn't finish copying")
  })

  it('phrases the title per operation type', () => {
    render({ operationType: 'trash' })
    expect(target.querySelector('.title')?.textContent.trim()).toBe("Couldn't finish moving to trash")
  })

  it('gives the real reason, not a generic apology', () => {
    render()
    expect(target.querySelector('.reason')?.textContent).toContain('Backup is read-only')
  })

  it('opens the operation queue and gets out of the way', () => {
    render()
    const action = [...target.querySelectorAll('button')].find((b) => b.textContent.includes('Show in operation queue'))
    action?.click()
    flushSync()
    expect(openQueueWindow).toHaveBeenCalledTimes(1)
    expect(dismissToast).toHaveBeenCalledWith('toast-1')
  })
})
