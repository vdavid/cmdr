/**
 * The main window's failure notice: who gets a toast, who deliberately doesn't,
 * and what a burst collapses into.
 *
 * The watcher is driven directly (`announceFailures`) rather than through its
 * `$effect` root, so each case is one call with one snapshot — the effect only
 * decides WHEN this runs, and `startOperationFailureWatch` is covered by the
 * main page's lifecycle test.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import type { OperationSnapshot } from '#lib/ipc/bindings.js'
import type { OperationRow } from '#lib/file-operations/queue/operations-store.svelte.js'
import { clearAllToasts, getToasts } from '#lib/ui/toast/index.js'

let foregroundOperationId: string | null = null
let foregroundFailureId: string | null = null
let claimPending = false
vi.mock('#lib/file-operations/foreground-operation.svelte.js', () => ({
  getForegroundOperationId: () => foregroundOperationId,
  getForegroundFailureId: () => foregroundFailureId,
  isForegroundClaimPending: () => claimPending,
}))

// The queue-window opener drags in `@tauri-apps/api/webviewWindow`; the toast
// content only needs it to exist for a click that these tests don't make.
vi.mock('#lib/file-operations/queue/queue-window.js', () => ({
  openQueueWindow: () => Promise.resolve(),
}))

vi.mock('#lib/file-operations/queue/main-window-operations.svelte.js', () => ({
  getMainWindowOperationRows: () => [] as OperationRow[],
}))

import { queueHandoffToastId, showQueueHandoffToast } from '#lib/file-operations/queue/queue-handoff-toast.js'
import {
  announceFailures,
  FAILURE_SUMMARY_TOAST_ID,
  FAILURE_TOAST_GROUP,
  MAX_FAILURE_TOASTS,
  resetAnnouncedFailures,
} from './operation-failure-watch.svelte'

function failedRow(operationId: string): OperationRow {
  const snapshot: OperationSnapshot = {
    operationId,
    operationType: 'copy',
    status: 'failed',
    source: '/Users/me/Documents/report.pdf',
    destination: '/Volumes/Backup',
    rollbackSupport: 'settled',
    reverses: null,
    error: { type: 'source_not_found', path: '/Users/me/Documents/report.pdf' },
    canRetry: false,
    conflictPolicy: null,
    items: null,
  }
  return { snapshot, progress: null }
}

function runningRow(operationId: string): OperationRow {
  const failed = failedRow(operationId)
  return { ...failed, snapshot: { ...failed.snapshot, status: 'running', error: null } }
}

function failureToasts() {
  return getToasts().filter((t) => t.toastGroup === FAILURE_TOAST_GROUP)
}

beforeEach(() => {
  clearAllToasts()
  resetAnnouncedFailures()
  foregroundOperationId = null
  foregroundFailureId = null
  claimPending = false
})

afterEach(() => {
  clearAllToasts()
})

describe('announceFailures', () => {
  // The failure is the job's last word, so "Sent to the background" beside it
  // is stale: a background job that fails fast leaves one toast, not two.
  it('replaces the failed job’s handoff toast, and leaves other jobs’ alone', () => {
    showQueueHandoffToast('Sent to the background.', 'a')
    showQueueHandoffToast('Sent to the background.', 'b')

    announceFailures([failedRow('a'), runningRow('b')])

    expect(getToasts().map((t) => t.id)).toEqual([queueHandoffToastId('b'), 'operation-failure:a'])
  })

  it('raises one persistent, grouped toast for a failure', () => {
    announceFailures([runningRow('live'), failedRow('a')])
    const toasts = failureToasts()
    expect(toasts).toHaveLength(1)
    // Persistent: a failure that happened while the user was away has to still
    // be on screen when they come back.
    expect(toasts[0].dismissal).toBe('persistent')
    expect(toasts[0].timeoutMs).toBe(0)
  })

  // A component toast used to offer no report at all; the failure now carries its typed
  // incident, so "Send error report…" names the operation and what stopped it.
  it('makes the failure reportable, as the operation and the typed error kind', () => {
    announceFailures([failedRow('a')])
    expect(failureToasts()[0].report).toEqual({
      kind: 'operationFailed',
      from: 'queueFailure',
      operation: 'copy',
      error: 'source_not_found',
      title: expect.any(String) as string,
    })
  })

  it('makes the collapsed summary reportable too', () => {
    announceFailures(['a', 'b', 'c', 'd'].map(failedRow))
    expect(failureToasts().find((t) => t.id === FAILURE_SUMMARY_TOAST_ID)?.report).toEqual({
      kind: 'operationsFailed',
      count: 4,
    })
  })

  it('does not toast the same failure twice when the snapshot is re-emitted', () => {
    announceFailures([failedRow('a')])
    announceFailures([failedRow('a')])
    announceFailures([failedRow('a')])
    expect(failureToasts()).toHaveLength(1)
  })

  it('stays quiet about the failure the foreground progress dialog owns', () => {
    foregroundOperationId = 'a'
    announceFailures([failedRow('a')])
    expect(failureToasts()).toHaveLength(0)
  })

  it('stays quiet about the failure the foreground error dialog is showing', () => {
    // The progress dialog releases its slot the moment it unmounts, so by the
    // time the failure row lands the error dialog is the one holding the op.
    foregroundFailureId = 'a'
    announceFailures([failedRow('a')])
    expect(failureToasts()).toHaveLength(0)
  })

  /**
   * ❗ A move of a locked file fails inside its start command, so the failure row
   * lands while the progress dialog is still waiting to learn the id. The slot was
   * empty, and the toast rode alongside the error dialog (QA round 3).
   */
  it('holds a failure while a progress dialog is still waiting to learn its id', () => {
    claimPending = true
    announceFailures([failedRow('a')])
    expect(failureToasts(), 'nothing can say yet whose failure this is').toHaveLength(0)

    // The claim settles on the id, and the dialog's own error dialog takes it.
    claimPending = false
    foregroundOperationId = 'a'
    announceFailures([failedRow('a')])
    expect(failureToasts()).toHaveLength(0)
  })

  it('announces a held failure once the pending claim turns out to be another operation', () => {
    claimPending = true
    announceFailures([failedRow('a')])
    claimPending = false
    foregroundOperationId = 'b'
    announceFailures([failedRow('a')])
    expect(failureToasts()).toHaveLength(1)
  })

  it('never re-announces a suppressed failure once the dialog closes', () => {
    foregroundFailureId = 'a'
    announceFailures([failedRow('a')])
    foregroundFailureId = null
    announceFailures([failedRow('a')])
    expect(failureToasts()).toHaveLength(0)
  })

  it('gives each of the first three failures its own toast', () => {
    announceFailures([failedRow('a')])
    announceFailures([failedRow('a'), failedRow('b')])
    announceFailures([failedRow('a'), failedRow('b'), failedRow('c')])
    expect(failureToasts()).toHaveLength(MAX_FAILURE_TOASTS)
    expect(failureToasts().some((t) => t.id === FAILURE_SUMMARY_TOAST_ID)).toBe(false)
  })

  it('collapses to a single summary toast past three', () => {
    // The cap is mechanical, not aesthetic: a stack full of persistent toasts
    // silently drops new ones, so an unbounded burst would lose failures.
    announceFailures([failedRow('a'), failedRow('b'), failedRow('c'), failedRow('d')])
    const toasts = failureToasts()
    expect(toasts).toHaveLength(1)
    expect(toasts[0].id).toBe(FAILURE_SUMMARY_TOAST_ID)
  })

  it('leaves the summary alone as more failures arrive: it counts them itself', () => {
    announceFailures([failedRow('a'), failedRow('b'), failedRow('c'), failedRow('d')])
    announceFailures([failedRow('a'), failedRow('b'), failedRow('c'), failedRow('d'), failedRow('e')])
    expect(failureToasts()).toHaveLength(1)
  })

  it('forgets a dismissed failure, so the set cannot grow without bound', () => {
    announceFailures([failedRow('a')])
    expect(failureToasts()).toHaveLength(1)
    clearAllToasts()
    // The user dismissed the row in the queue: it leaves the snapshot, and
    // operation ids are unique, so forgetting it can never re-toast it.
    announceFailures([])
    announceFailures([failedRow('b')])
    expect(failureToasts()).toHaveLength(1)
  })
})
