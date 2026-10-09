import { describe, it, expect } from 'vitest'
import { answersNow, hasReconnectLoop, isLiveSession, showsDisconnect } from './connection-state'
import type { ConnectionState, VolumeBackendCapabilities } from '../types'

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

  it('offers Disconnect wherever a volume is REGISTERED, and nowhere else', () => {
    // ❌ Not `saved`: a greyed row was never connected, so "Disconnect" would
    // promise an action with no subject. ❗ Both sign-in states ARE in: the
    // volume is registered and its session stopped, which is exactly a thing to
    // drop. Leaving them out was a dead end — a share that fell to
    // `needs_sign_in` could be signed into or forgotten and never dropped — and
    // the changed-key banner's own Disconnect is the documented way out of a host
    // key that stopped matching. ❌ `os_mount` stays out because the WORD differs
    // there (it says Eject); `isLiveSession` is what covers it.
    expect(ALL.filter((state) => showsDisconnect(state))).toEqual([
      'direct',
      'disconnected',
      'needs_sign_in',
      'needs_host_key_approval',
    ])
  })
})

describe('answersNow', () => {
  const registered: VolumeBackendCapabilities = {
    backendCanWrite: true,
    canExport: true,
    canBeIndexed: true,
    canShareLinks: false,
    renamesCanCopy: false,
    hasOsMountFallback: false,
  }

  it('answers for a local drive, which has no session to wait for', () => {
    expect(answersNow({ id: 'root' })).toBe(true)
    expect(answersNow({ id: 'vol-backup-0123456789abcdef', connectionState: null })).toBe(true)
  })

  it('answers for a session-backed volume only while its session is live', () => {
    expect(answersNow({ id: 'smb-nas-0123456789abcdef', connectionState: 'direct' })).toBe(true)
    expect(answersNow({ id: 'smb-nas-0123456789abcdef', connectionState: 'saved' })).toBe(false)
    expect(answersNow({ id: 'smb-nas-0123456789abcdef', connectionState: 'disconnected' })).toBe(false)
  })

  // Regression anchor for ERR-JUCNB / ERR-JT9ZX: an ADB phone waiting for its
  // "Allow USB debugging?" tap is listed with no session state at all, so it read
  // as a local drive that answers, and the app offered to index it.
  it("doesn't answer for a phone until its volume is registered", () => {
    expect(answersNow({ id: 'adb-lgh815-0123456789abcdef', connectionState: null })).toBe(false)
    expect(answersNow({ id: 'adb-lgh815-0123456789abcdef', capabilities: null })).toBe(false)
    expect(answersNow({ id: 'adb-lgh815-0123456789abcdef', capabilities: registered })).toBe(true)
    expect(answersNow({ id: 'mtp-lgh815-0123456789abcdef:65537' })).toBe(false)
    expect(answersNow({ id: 'mtp-lgh815-0123456789abcdef:65537', capabilities: registered })).toBe(true)
  })
})
