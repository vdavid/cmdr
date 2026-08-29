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
import type { OperationSnapshot } from '$lib/ipc/bindings'
import type { OperationRow } from '$lib/file-operations/queue/operations-store.svelte'
import { clearAllToasts, getToasts } from '$lib/ui/toast'

let foregroundOperationId: string | null = null
let foregroundFailureId: string | null = null
vi.mock('$lib/file-operations/foreground-operation.svelte', () => ({
  getForegroundOperationId: () => foregroundOperationId,
  getForegroundFailureId: () => foregroundFailureId,
}))

// The queue-window opener drags in `@tauri-apps/api/webviewWindow`; the toast
// content only needs it to exist for a click that these tests don't make.
vi.mock('$lib/file-operations/queue/queue-window', () => ({
  openQueueWindow: () => Promise.resolve(),
}))

vi.mock('$lib/file-operations/queue/main-window-operations.svelte', () => ({
  getMainWindowOperationRows: () => [] as OperationRow[],
}))

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
    supportsRollback: false,
    reverses: null,
    error: { type: 'source_not_found', path: '/Users/me/Documents/report.pdf' },
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
})

afterEach(() => {
  clearAllToasts()
})

describe('announceFailures', () => {
  it('raises one persistent, grouped toast for a failure', () => {
    announceFailures([runningRow('live'), failedRow('a')])
    const toasts = failureToasts()
    expect(toasts).toHaveLength(1)
    // Persistent: a failure that happened while the user was away has to still
    // be on screen when they come back.
    expect(toasts[0].dismissal).toBe('persistent')
    expect(toasts[0].timeoutMs).toBe(0)
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
