/**
 * The one question the progress dialog's background/queue button asks: is there
 * anything in the queue BESIDES the operation this dialog is showing?
 *
 * Each gate gets its own case, because each one is a wrong label if it's missing:
 * counting the dialog's own operation reads "Queue" forever, counting a rename
 * flips the word for an operation that's already gone, and counting a retained
 * failure calls a notice "work".
 */

import { describe, it, expect } from 'vitest'
import type { OperationSnapshot } from '$lib/ipc/bindings'
import type { OperationRow } from './operations-store.svelte'
import { hasOtherQueuedWork } from './queue-backlog'

function row(
  operationId: string,
  status: OperationSnapshot['status'],
  operationType: OperationSnapshot['operationType'] = 'copy',
): OperationRow {
  return {
    snapshot: {
      operationId,
      operationType,
      status,
      source: '/src/file',
      destination: '/dst/file',
      supportsRollback: true,
      error: null,
    },
    progress: null,
    etaSecondsDisplay: null,
  }
}

describe('hasOtherQueuedWork', () => {
  it('an empty queue holds nothing', () => {
    expect(hasOtherQueuedWork([], 'op-self')).toBe(false)
  })

  it('another running operation is work to queue behind', () => {
    expect(hasOtherQueuedWork([row('op-other', 'running')], 'op-self')).toBe(true)
  })

  it("the dialog's OWN operation doesn't count", () => {
    // Without this the label would read "Queue" every single time: the dialog's
    // operation is always in the queue while the dialog is up.
    expect(hasOtherQueuedWork([row('op-self', 'running')], 'op-self')).toBe(false)
  })

  it('a paused or waiting operation still counts: it is work, parked', () => {
    expect(hasOtherQueuedWork([row('op-other', 'paused')], 'op-self')).toBe(true)
    expect(hasOtherQueuedWork([row('op-other', 'queued')], 'op-self')).toBe(true)
  })

  it('instant operations never count: a rename is gone before the word changes', () => {
    expect(hasOtherQueuedWork([row('op-r', 'running', 'rename')], 'op-self')).toBe(false)
    expect(hasOtherQueuedWork([row('op-f', 'running', 'create_folder')], 'op-self')).toBe(false)
    expect(hasOtherQueuedWork([row('op-n', 'running', 'create_file')], 'op-self')).toBe(false)
  })

  it("a retained failure doesn't count: it's a notice, not work you'd wait behind", () => {
    expect(hasOtherQueuedWork([row('op-dead', 'failed')], 'op-self')).toBe(false)
  })

  it('a settled operation on its way out of the list doesn’t count', () => {
    expect(hasOtherQueuedWork([row('op-done', 'done')], 'op-self')).toBe(false)
    expect(hasOtherQueuedWork([row('op-gone', 'cancelled')], 'op-self')).toBe(false)
  })

  it('one real operation among the ones that never count is enough', () => {
    const rows = [
      row('op-self', 'running'),
      row('op-r', 'running', 'rename'),
      row('op-dead', 'failed'),
      row('op-other', 'queued'),
    ]
    expect(hasOtherQueuedWork(rows, 'op-self')).toBe(true)
  })

  it('with no id of its own yet, it counts every live row', () => {
    // The button is gated on a known `operationId`, so this is defensive: with
    // nothing to exclude, an operation that IS ours would count. Nothing to
    // exclude also means nothing to hide.
    expect(hasOtherQueuedWork([row('op-1', 'running')], null)).toBe(true)
    expect(hasOtherQueuedWork([], null)).toBe(false)
  })
})
