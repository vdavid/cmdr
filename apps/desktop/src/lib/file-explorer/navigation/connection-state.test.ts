import { describe, it, expect } from 'vitest'
import { hasReconnectLoop, isLiveSession, showsDisconnect } from './connection-state'
import type { ConnectionState } from '../types'

const ALL: ConnectionState[] = [
  'direct',
  'os_mount',
  'disconnected',
  'needs_sign_in',
  'needs_host_key_approval',
  'saved',
]

describe('connection-state predicates', () => {
  it('answers false for a volume with no session at all', () => {
    // A local disk, a favorite, the hub row: `undefined` and `null` both arrive
    // (Rust's `None` serializes to `null`), and neither is a session.
    for (const predicate of [hasReconnectLoop, isLiveSession, showsDisconnect]) {
      expect(predicate(undefined)).toBe(false)
      expect(predicate(null)).toBe(false)
    }
  })

  it('enrolls every state with a session behind it in the reconnect manager', () => {
    // `needs_host_key_approval` is in so the pane keeps its subscription and can
    // render the key banner. `saved` is out: nothing is in flight to recover.
    const enrolled = ALL.filter((state) => hasReconnectLoop(state))
    expect(enrolled).toEqual(['direct', 'os_mount', 'disconnected', 'needs_sign_in', 'needs_host_key_approval'])
  })

  it('calls only a serving session live', () => {
    expect(ALL.filter((state) => isLiveSession(state))).toEqual(['direct', 'os_mount'])
  })

  it('offers Disconnect only where there is something to disconnect', () => {
    // ❌ Not `saved`: a greyed row was never connected, so "Disconnect" would
    // promise an action with no subject.
    expect(ALL.filter((state) => showsDisconnect(state))).toEqual(['direct', 'disconnected'])
  })
})
