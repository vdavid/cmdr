/**
 * The chip's pick-and-measure rules, as pure data. Every visibility gate the
 * corner chip has lives in `pickChipOperation`, so it can be proven here without
 * a DOM: which operation wins, which ones the chip must stay silent about, and
 * how full the bar is.
 */

import { describe, it, expect, vi } from 'vitest'
import type { OperationRow } from '$lib/file-operations/queue/operations-store.svelte'
import type { OperationSnapshot, WriteProgressEvent } from '$lib/ipc/bindings'
import { seconds } from '$lib/units'

// The rows' type comes from the operations store, which subscribes to Tauri
// events at import time. Mock the transport so this pure test needs no backend.
vi.mock('$lib/tauri-commands', () => ({
  listOperations: vi.fn(() => Promise.resolve([])),
  onOperationsChanged: vi.fn(() => Promise.resolve(() => {})),
  onWriteProgress: vi.fn(() => Promise.resolve(() => {})),
}))

import { destinationName, pickChipOperation, pickChipState } from './operation-chip'

function progress(over: Partial<WriteProgressEvent> = {}): WriteProgressEvent {
  return {
    operationId: 'op-1',
    operationType: 'copy',
    phase: 'copying',
    currentFile: 'report.pdf',
    filesDone: 2,
    filesTotal: 10,
    bytesDone: 420,
    bytesTotal: 1000,
    ...over,
  }
}

function row(
  over: Partial<OperationSnapshot> = {},
  progressEvent: WriteProgressEvent | null = progress(),
  etaSecondsDisplay: number | null = 80,
): OperationRow {
  const operationId = over.operationId ?? 'op-1'
  return {
    snapshot: {
      operationId,
      operationType: 'copy',
      status: 'running',
      source: '/Users/me/Documents',
      destination: '/Volumes/Naspolya/Backup',
      supportsRollback: true,
      error: null,
      ...over,
    },
    progress: progressEvent === null ? null : { ...progressEvent, operationId },
    etaSecondsDisplay: etaSecondsDisplay === null ? null : seconds(etaSecondsDisplay),
  }
}

describe('pickChipOperation', () => {
  it('says nothing when the queue is empty', () => {
    expect(pickChipOperation([], null)).toBeNull()
  })

  it('picks the first running operation when several lanes run at once', () => {
    const picked = pickChipOperation([row({ operationId: 'a' }), row({ operationId: 'b' })], null)
    expect(picked?.row.snapshot.operationId).toBe('a')
  })

  it('measures the bar in bytes', () => {
    const picked = pickChipOperation([row({}, progress({ bytesDone: 420, bytesTotal: 1000 }))], null)
    expect(picked?.fraction).toBeCloseTo(0.42)
    expect(picked?.percent).toBe(42)
  })

  it('falls back to the file count when the operation moves no bytes', () => {
    // A same-volume move renames server-side: zero bytes cross the wire, so a
    // bytes bar would sit at 0% for the whole operation and lie about it.
    const picked = pickChipOperation(
      [row({ operationType: 'move' }, progress({ bytesDone: 0, bytesTotal: 0, filesDone: 3, filesTotal: 10 }))],
      null,
    )
    expect(picked?.fraction).toBeCloseTo(0.3)
    expect(picked?.percent).toBe(30)
  })

  it('reads 0% with neither bytes nor files to count, and never NaN', () => {
    const picked = pickChipOperation(
      [row({}, progress({ bytesDone: 0, bytesTotal: 0, filesDone: 0, filesTotal: 0 }))],
      null,
    )
    expect(picked?.fraction).toBe(0)
    expect(picked?.percent).toBe(0)
  })

  it('reads 0% before the first progress tick', () => {
    const picked = pickChipOperation([row({}, null)], null)
    expect(picked?.percent).toBe(0)
  })

  it.each(['rename', 'create_folder', 'create_file'] as const)('stays quiet for an instant %s', (operationType) => {
    expect(pickChipOperation([row({ operationType })], null)).toBeNull()
  })

  it('skips an instant op to reach the real work behind it', () => {
    const picked = pickChipOperation(
      [row({ operationId: 'quick', operationType: 'rename' }), row({ operationId: 'slow' })],
      null,
    )
    expect(picked?.row.snapshot.operationId).toBe('slow')
  })

  it('stays quiet about the operation the foreground dialog owns', () => {
    expect(pickChipOperation([row({ operationId: 'op-1' })], 'op-1')).toBeNull()
  })

  it('speaks up about a second operation the foreground dialog does not own', () => {
    const picked = pickChipOperation([row({ operationId: 'a' }), row({ operationId: 'b' })], 'a')
    expect(picked?.row.snapshot.operationId).toBe('b')
  })

  it('keeps showing a paused-only queue, flagged as paused', () => {
    const picked = pickChipOperation([row({ status: 'paused' })], null)
    expect(picked?.paused).toBe(true)
    expect(picked?.percent).toBe(42)
  })

  it('prefers a running operation over a paused one, whatever the order', () => {
    const picked = pickChipOperation([row({ operationId: 'a', status: 'paused' }), row({ operationId: 'b' })], null)
    expect(picked?.row.snapshot.operationId).toBe('b')
    expect(picked?.paused).toBe(false)
  })

  it('stays quiet for a queue that is only waiting on a lane', () => {
    expect(pickChipOperation([row({ status: 'queued' })], null)).toBeNull()
  })

  it('names the destination folder, trailing slash or not', () => {
    expect(destinationName('/Volumes/Naspolya/Backup')).toBe('Backup')
    expect(destinationName('/Volumes/Naspolya/Backup/')).toBe('Backup')
  })

  it('has no destination to name for a delete', () => {
    expect(destinationName(null)).toBe('')
  })

  it('clamps a bar that overshoots its total', () => {
    // The scan can revise a total downward mid-copy; the bar must not overflow.
    const picked = pickChipOperation([row({}, progress({ bytesDone: 1500, bytesTotal: 1000 }))], null)
    expect(picked?.fraction).toBe(1)
    expect(picked?.percent).toBe(100)
  })
})

/** A retained failure as it arrives on the snapshot. */
function failedRow(operationId = 'gone'): OperationRow {
  return row(
    { operationId, status: 'failed', error: { type: 'source_not_found', path: '/gone.txt' } },
    null,
    null,
  )
}

describe('pickChipState', () => {
  it('says nothing with neither work nor failures', () => {
    expect(pickChipState([], null, null)).toBeNull()
  })

  it('shows a retained failure once nothing is running', () => {
    const state = pickChipState([failedRow()], null, null)
    expect(state).toEqual({ kind: 'failure', count: 1 })
  })

  it('counts every retained failure', () => {
    const state = pickChipState([failedRow('a'), failedRow('b')], null, null)
    expect(state).toEqual({ kind: 'failure', count: 2 })
  })

  it('lets live work win the corner over a failure', () => {
    // The failure is still in the queue and in its toast; the corner is one
    // slot, and what's moving right now is the more useful readout.
    const state = pickChipState([failedRow('a'), row({ operationId: 'b' })], null, null)
    expect(state?.kind).toBe('progress')
    expect(state?.kind === 'progress' && state.operation.row.snapshot.operationId).toBe('b')
  })

  it('lets a PAUSED operation win too: it is still work in flight', () => {
    const state = pickChipState([failedRow('a'), row({ operationId: 'b', status: 'paused' })], null, null)
    expect(state?.kind).toBe('progress')
  })

  it('stays quiet about the failure the foreground error dialog is showing', () => {
    expect(pickChipState([failedRow('a')], null, 'a')).toBeNull()
    expect(pickChipState([failedRow('a'), failedRow('b')], null, 'a')).toEqual({ kind: 'failure', count: 1 })
  })
})
