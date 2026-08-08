/**
 * The two foreground slots: which operation the progress modal owns, and which
 * failure its error dialog is showing, so ambient surfaces can stay quiet about
 * both.
 *
 * What matters here is the lifecycle, not the storage: the first slot must empty
 * on every route out of the dialog, a late clear from a dialog that has already
 * handed the slot on must not silence the new owner, and the two slots must be
 * independent — the handover between them (`dialog-state.failure-handover`)
 * exists precisely because the first empties before the failure surfaces.
 */

import { describe, it, expect, beforeEach } from 'vitest'
import {
  setForegroundOperationId,
  getForegroundOperationId,
  clearForegroundOperation,
  setForegroundFailureId,
  getForegroundFailureId,
  beginForegroundClaim,
  endForegroundClaim,
  isForegroundClaimPending,
} from './foreground-operation.svelte'

beforeEach(() => {
  setForegroundOperationId(null)
  setForegroundFailureId(null)
  while (isForegroundClaimPending()) endForegroundClaim()
})

describe('foreground operation slot', () => {
  it('is empty when no progress dialog owns an operation', () => {
    expect(getForegroundOperationId()).toBeNull()
  })

  it('holds the operation the dialog took ownership of', () => {
    setForegroundOperationId('op-1')
    expect(getForegroundOperationId()).toBe('op-1')
  })

  it('empties on an explicit null', () => {
    setForegroundOperationId('op-1')
    setForegroundOperationId(null)
    expect(getForegroundOperationId()).toBeNull()
  })

  it('clears the slot for the operation that owns it', () => {
    setForegroundOperationId('op-1')
    clearForegroundOperation('op-1')
    expect(getForegroundOperationId()).toBeNull()
  })

  it('ignores a clear from an operation that no longer owns the slot', () => {
    // A dialog tearing down after the next one already claimed the slot: its
    // late clear must not silence the operation now in the foreground.
    setForegroundOperationId('op-1')
    setForegroundOperationId('op-2')
    clearForegroundOperation('op-1')
    expect(getForegroundOperationId()).toBe('op-2')
  })

  it('tolerates a clear with nothing in the slot', () => {
    clearForegroundOperation('op-1')
    expect(getForegroundOperationId()).toBeNull()
  })
})

describe('foreground claim', () => {
  it('is not pending when nothing is starting', () => {
    expect(isForegroundClaimPending()).toBe(false)
  })

  it('is pending between the dispatch and the operation id landing', () => {
    // The window a conflict can arrive in: the operation exists on the backend
    // and can already emit, but no slot names it yet.
    beginForegroundClaim()
    expect(isForegroundClaimPending()).toBe(true)

    setForegroundOperationId('op-1')
    endForegroundClaim()
    expect(isForegroundClaimPending()).toBe(false)
  })

  it('settles on an abandoned dispatch, with the slot still empty', () => {
    // Escape during dispatch: the dialog cancels the operation it just started
    // and never claims the slot. The claim still has to end, or every later
    // conflict would defer forever.
    beginForegroundClaim()
    endForegroundClaim()

    expect(isForegroundClaimPending()).toBe(false)
    expect(getForegroundOperationId()).toBeNull()
  })

  it('stays pending while a second dispatch is in flight', () => {
    // Two claims can overlap: a dialog's dispatch is still awaiting its response
    // when the next dialog starts one. A boolean would have the first one's
    // teardown clear the second one's claim, and the second conflict would be
    // decided against an empty slot.
    beginForegroundClaim()
    beginForegroundClaim()

    endForegroundClaim()
    expect(isForegroundClaimPending()).toBe(true)

    endForegroundClaim()
    expect(isForegroundClaimPending()).toBe(false)
  })

  it('never goes negative, so a stray end cannot hide a real claim', () => {
    endForegroundClaim()
    beginForegroundClaim()
    expect(isForegroundClaimPending()).toBe(true)
  })
})

describe('foreground failure slot', () => {
  it('is empty when no error dialog is showing a failure', () => {
    expect(getForegroundFailureId()).toBeNull()
  })

  it('holds the failure the error dialog took over', () => {
    setForegroundFailureId('op-1')
    expect(getForegroundFailureId()).toBe('op-1')
  })

  it('empties on an explicit null, which is how the dialog closing releases it', () => {
    setForegroundFailureId('op-1')
    setForegroundFailureId(null)
    expect(getForegroundFailureId()).toBeNull()
  })

  it('takes the newest failure, so the dialog on screen owns the slot', () => {
    // A second operation can fail while the first error dialog is still up: the
    // dialog re-renders with the new error, so the slot has to follow it, or
    // closing it would dismiss the wrong retained row.
    setForegroundFailureId('op-1')
    setForegroundFailureId('op-2')
    expect(getForegroundFailureId()).toBe('op-2')
  })

  it('outlives the progress dialog releasing the operation slot', () => {
    // The whole reason there are two slots: the progress dialog unmounts the
    // instant the error arrives, and the retained failure row only reaches the
    // snapshot afterwards. With the first slot empty, this one is what stops an
    // ambient surface announcing a failure the user is already reading.
    setForegroundOperationId('op-1')
    setForegroundFailureId('op-1')

    clearForegroundOperation('op-1')

    expect(getForegroundOperationId()).toBeNull()
    expect(getForegroundFailureId()).toBe('op-1')
  })

  it('is untouched by the operation slot moving on to the next operation', () => {
    setForegroundFailureId('op-1')
    setForegroundOperationId('op-2')
    expect(getForegroundFailureId()).toBe('op-1')
  })
})
