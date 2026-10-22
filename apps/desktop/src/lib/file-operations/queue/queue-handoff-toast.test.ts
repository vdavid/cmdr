/**
 * The handoff toast is keyed to its job, so whatever later speaks for the job's
 * ending (a trash's Undo toast, the failure toast) can take its place.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import type { OperationRow } from './operations-store.svelte'
import { clearAllToasts, getToasts } from '#lib/ui/toast/index.js'

vi.mock('#lib/file-operations/queue/queue-window.js', () => ({
  openQueueWindow: () => Promise.resolve(),
}))

let rows: OperationRow[] = []
vi.mock('#lib/file-operations/queue/main-window-operations.svelte.js', () => ({
  getMainWindowOperationRows: () => rows,
}))

import { dismissQueueHandoffToast, queueHandoffToastId, showQueueHandoffToast } from './queue-handoff-toast'

function row(operationId: string, status: OperationRow['snapshot']['status']): OperationRow {
  return {
    snapshot: {
      operationId,
      operationType: 'copy',
      status,
      source: '/Users/me/a.jpg',
      destination: '/Users/me/backup',
      rollbackSupport: 'archiveRewrite',
      reverses: null,
      error: null,
      canRetry: false,
      conflictPolicy: null,
      items: null,
    },
    progress: null,
  }
}

beforeEach(() => {
  clearAllToasts()
  rows = []
})

afterEach(() => {
  clearAllToasts()
})

describe('showQueueHandoffToast', () => {
  it('raises one toast per job, keyed to it', () => {
    showQueueHandoffToast('Sent to the background.', 'op-1')
    showQueueHandoffToast('Sent to the background.', 'op-2')

    expect(getToasts().map((t) => t.id)).toEqual([queueHandoffToastId('op-1'), queueHandoffToastId('op-2')])
  })

  // The failure toast already spoke for it, so a handoff toast arriving after
  // would stack a stale "Sent to the background" beside it.
  it('stays out of the way of a job that already failed', () => {
    rows = [row('op-1', 'failed')]

    showQueueHandoffToast('Sent to the background.', 'op-1')

    expect(getToasts()).toEqual([])
  })

  it('still shows for a job that already finished, the only word a quick copy gets', () => {
    rows = [row('op-1', 'done')]

    showQueueHandoffToast('Sent to the background.', 'op-1')

    expect(getToasts().map((t) => t.id)).toEqual([queueHandoffToastId('op-1')])
  })
})

describe('dismissQueueHandoffToast', () => {
  it('takes down only that job’s toast', () => {
    showQueueHandoffToast('Sent to the background.', 'op-1')
    showQueueHandoffToast('Sent to the background.', 'op-2')

    dismissQueueHandoffToast('op-1')

    expect(getToasts().map((t) => t.id)).toEqual([queueHandoffToastId('op-2')])
  })
})
