import { describe, it, expect } from 'vitest'
import type { WriteCompleteEvent } from '#lib/ipc/bindings.js'
import { doneFigures, doneSkippedLine, doneTitle } from './transfer-done-summary'

function event(over: Partial<WriteCompleteEvent> = {}): WriteCompleteEvent {
  return {
    operationId: 'op-1',
    operationType: 'copy',
    filesProcessed: 1_240,
    filesSkipped: 6,
    bytesProcessed: 50_000_000,
    ...over,
  }
}

describe('doneTitle', () => {
  it('counts what the operation did, not what it skipped, with grouping', () => {
    expect(doneTitle('copy', event())).toBe('Copied 1,234 files')
    expect(doneTitle('move', event({ filesProcessed: 1, filesSkipped: 0 }))).toBe('Moved 1 file')
    expect(doneTitle('trash', event({ filesProcessed: 3, filesSkipped: 0 }))).toBe('Moved 3 files to the trash')
    expect(doneTitle('delete', event({ filesProcessed: 2, filesSkipped: 0 }))).toBe('Deleted 2 files')
  })

  it('says so when everything was skipped', () => {
    expect(doneTitle('copy', event({ filesProcessed: 4, filesSkipped: 4 }))).toBe('Nothing copied')
  })
})

describe('doneSkippedLine', () => {
  it('names why the skips happened', () => {
    expect(doneSkippedLine(event({ filesSkipped: 12 }))).toBe('Skipped 12 files that already existed')
    expect(doneSkippedLine(event({ filesSkipped: 1 }))).toBe('Skipped 1 file that already existed')
  })

  it('says nothing about skips that didn’t happen', () => {
    expect(doneSkippedLine(event({ filesSkipped: 0 }))).toBeNull()
  })
})

describe('doneFigures', () => {
  it('gives size, time, and average speed when it knows all three', () => {
    expect(doneFigures(event({ bytesProcessed: 50_000_000 }), 10_000)).toEqual({
      kind: 'rate',
      bytes: 50_000_000,
      seconds: 10,
      bytesPerSecond: 5_000_000,
    })
  })

  it('gives the size alone when it doesn’t know how long it took, or under a second', () => {
    expect(doneFigures(event({ bytesProcessed: 900 }), null)).toEqual({ kind: 'size', bytes: 900 })
    expect(doneFigures(event({ bytesProcessed: 900 }), 400)).toEqual({ kind: 'size', bytes: 900 })
  })

  it('gives the time alone for an operation that moved no bytes', () => {
    expect(doneFigures(event({ bytesProcessed: 0 }), 4_000)).toEqual({ kind: 'time', seconds: 4 })
  })

  it('says nothing with neither', () => {
    expect(doneFigures(event({ bytesProcessed: 0 }), null)).toBeNull()
  })
})
