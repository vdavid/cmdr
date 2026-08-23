/**
 * What `throwIpcError` leaves behind for a log to read.
 *
 * It is the LAST-RESORT thrower, for the handful of commands that still answer
 * with a bare `String`. Every typed command family throws a `TypedFailure`
 * subclass instead and words its refusal from the message catalog. What matters
 * here is that a caught value keeps its fields (so nothing has to parse a
 * sentence to recover a classification) and that its `Error.message` names the
 * REASON rather than dumping JSON.
 */

import { describe, it, expect } from 'vitest'
import { throwIpcError } from './ipc-types'

/** Runs `throwIpcError(value)` and hands back what it threw. */
function caught(value: unknown): unknown {
  try {
    throwIpcError(value)
  } catch (e) {
    return e
  }
  throw new Error('throwIpcError must throw')
}

describe('throwIpcError', () => {
  it('passes an Error straight through, stack and all', () => {
    const original = new Error('already an Error')
    expect(caught(original)).toBe(original)
  })

  it('wraps a bare string, which is what a `Result<_, String>` command sends', () => {
    expect(caught('the server said no')).toEqual(new Error('the server said no'))
  })

  it('names the VARIANT of a tagged wire error, so a log line reads the reason', () => {
    const thrown = caught({ type: 'timedOut' })

    expect(thrown).toBeInstanceOf(Error)
    expect((thrown as Error).message).toBe('timedOut')
  })

  it("keeps the tagged error's own fields on the Error, so nothing has to parse the message", () => {
    const thrown = caught({ type: 'volumeNotFound', volumeId: 'volumes-usb' })

    expect(thrown).toMatchObject({ type: 'volumeNotFound', volumeId: 'volumes-usb' })
  })

  it("reads a `kind`-tagged family too, which is how the viewer's errors are shaped", () => {
    expect((caught({ kind: 'sessionNotFound', sessionId: 'sess-1' }) as Error).message).toBe('sessionNotFound')
  })

  it('prefers an explicit message over the tag when a value carries both', () => {
    expect((caught({ message: 'the store refused', type: 'other' }) as Error).message).toBe('the store refused')
  })

  it('falls back to JSON for a value carrying neither, rather than losing it', () => {
    expect((caught({ somethingElse: 42 }) as Error).message).toBe('{"somethingElse":42}')
  })
})
