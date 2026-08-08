/**
 * The two rules the conflict host is built on, as pure data.
 *
 * Both are seams, and both are where this feature goes next: ownership widens
 * when the Foreground work lets a queue row adopt a running operation back into
 * the progress dialog, and the pause narrows when "pause just this one, let the
 * parallel and next-in-line ones carry on" lands. Testing them here rather than
 * through the controller is what makes each branch provable without a listener,
 * a store, or a DOM.
 */

import { describe, it, expect } from 'vitest'
import type { OperationSnapshot } from '$lib/tauri-commands'
import type { OperationRow } from './queue/operations-store.svelte'
import { conflictOwner, operationsToPauseFor } from './operation-conflict-rules'

function row(id: string, status: OperationSnapshot['status'], type: OperationSnapshot['operationType'] = 'copy') {
  return {
    snapshot: {
      operationId: id,
      operationType: type,
      status,
      source: '/s',
      destination: '/d',
      supportsRollback: true,
      error: null,
    },
    progress: null,
    etaSecondsDisplay: null,
  } satisfies OperationRow
}

describe('conflictOwner', () => {
  it('gives a conflict to this window when no dialog owns the operation', () => {
    expect(conflictOwner('op-1', { foregroundOperationId: null, claimPending: false })).toBe('here')
  })

  it('leaves the foreground dialog its own conflict', () => {
    // The progress dialog shows the clash in its own body and always has. Two
    // prompts for one clash is the failure mode this branch exists to prevent.
    expect(conflictOwner('op-1', { foregroundOperationId: 'op-1', claimPending: false })).toBe('foreground')
  })

  it('takes a conflict for a different operation while a dialog is up', () => {
    expect(conflictOwner('op-2', { foregroundOperationId: 'op-1', claimPending: false })).toBe('here')
  })

  it('defers while a dispatch has not named its operation yet', () => {
    // The window where a conflict can beat the start command's response: the
    // slot is empty but a dialog is about to fill it, so neither answer is
    // knowable and guessing costs a double prompt or a wedge.
    expect(conflictOwner('op-1', { foregroundOperationId: null, claimPending: true })).toBe('unknown')
  })

  it('still defers a conflict for an operation another dialog already owns', () => {
    // A second dispatch in flight says nothing about who owns op-2; only the
    // claim settling does.
    expect(conflictOwner('op-2', { foregroundOperationId: 'op-1', claimPending: true })).toBe('unknown')
  })
})

describe('operationsToPauseFor', () => {
  it('pauses every running operation, including the one that is asking', () => {
    const rows = [row('op-1', 'running'), row('op-2', 'running')]
    expect(operationsToPauseFor('op-1', rows)).toEqual(['op-1', 'op-2'])
  })

  it('leaves an operation the user paused by hand out of it', () => {
    // It must not come back on resume: the controller resumes exactly what it
    // paused, so anything already paused has to stay out of the set.
    const rows = [row('op-1', 'running'), row('op-2', 'paused')]
    expect(operationsToPauseFor('op-1', rows)).toEqual(['op-1'])
  })

  it('leaves a queued operation alone', () => {
    // Nothing is executing there; the lane it waits on is what holds it, and
    // pausing a queued op is a backend no-op anyway.
    const rows = [row('op-1', 'running'), row('op-2', 'queued')]
    expect(operationsToPauseFor('op-1', rows)).toEqual(['op-1'])
  })

  it('leaves a retained failure alone', () => {
    const rows = [row('op-1', 'running'), row('op-2', 'failed')]
    expect(operationsToPauseFor('op-1', rows)).toEqual(['op-1'])
  })

  it('names the conflicting operation even when the snapshot has not caught up', () => {
    // The rows arrive on their own stream, so a conflict can land a tick before
    // the operation shows up as running. The one operation we know is executing
    // is the one that just asked a question.
    expect(operationsToPauseFor('op-1', [])).toEqual(['op-1'])
  })

  it('never names the same operation twice', () => {
    const rows = [row('op-1', 'running')]
    expect(operationsToPauseFor('op-1', rows)).toEqual(['op-1'])
  })
})
