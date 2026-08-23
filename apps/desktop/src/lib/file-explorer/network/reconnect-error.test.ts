/**
 * Unit tests for the typed reconnect refusal that crosses IPC.
 *
 * These pin the two properties the reconnect manager depends on: a caught value
 * gives its typed refusal back (so nobody parses a sentence to find out what
 * happened), and a refusal from a DIFFERENT family gives back `null` rather than
 * being misread as this one. The diagnostic line is asserted too, because it is
 * what the manager's log records and it must name the reason without ever
 * carrying backend prose.
 */
import { describe, it, expect } from 'vitest'
import type { ReconnectError } from '$lib/ipc/bindings'
import { ReconnectFailure, throwReconnectError, asReconnectError, describeReconnectRefusal } from './reconnect-error'
import { MutationFailure } from '$lib/file-operations/mutation-error'

const volumeGone: ReconnectError = { type: 'volumeNotFound', volumeId: 'smb-nas-backup' }
const volumeRefused: ReconnectError = {
  type: 'volume',
  error: { type: 'permissionDenied', data: '/Volumes/backup' },
}

describe('ReconnectFailure', () => {
  it('keeps the typed refusal on the thrown Error', () => {
    const failure = new ReconnectFailure(volumeGone)

    expect(failure).toBeInstanceOf(Error)
    expect(failure.name).toBe('ReconnectFailure')
    expect(failure.failure).toEqual(volumeGone)
  })

  it('names the reason in the diagnostic, and carries no backend sentence', () => {
    expect(new ReconnectFailure(volumeGone).message).toBe('reconnect refused: volumeNotFound')
    expect(new ReconnectFailure(volumeRefused).message).toBe('reconnect refused: volume')
  })
})

describe('throwReconnectError', () => {
  it('throws a ReconnectFailure the catch site can unwrap', () => {
    let caught: unknown
    try {
      throwReconnectError(volumeRefused)
    } catch (e) {
      caught = e
    }

    expect(caught).toBeInstanceOf(ReconnectFailure)
    expect(asReconnectError(caught)).toEqual(volumeRefused)
  })
})

describe('asReconnectError', () => {
  it('gives the typed refusal back', () => {
    expect(asReconnectError(new ReconnectFailure(volumeGone))).toEqual(volumeGone)
  })

  it('answers null for anything that is not one', () => {
    expect(asReconnectError(new Error('boom'))).toBeNull()
    expect(asReconnectError('boom')).toBeNull()
    expect(asReconnectError(null)).toBeNull()
    expect(asReconnectError(undefined)).toBeNull()
  })

  // Pre-fix this would have passed wrongly if the families shared one class:
  // every typed failure carries a `failure`, so a structural check would hand
  // back another family's payload and the caller would branch on a `type` that
  // means something else entirely.
  it('answers null for a DIFFERENT family, rather than handing back its payload', () => {
    const otherFamily = new MutationFailure({ type: 'notFound', path: '/tmp/gone' })

    expect(asReconnectError(otherFamily)).toBeNull()
  })
})

describe('describeReconnectRefusal', () => {
  it('names which volume went missing', () => {
    expect(describeReconnectRefusal(volumeGone)).toBe('volumeNotFound(smb-nas-backup)')
  })

  it("reaches through to the volume error's own reason", () => {
    expect(describeReconnectRefusal(volumeRefused)).toBe('volume/permissionDenied')
  })
})
