/**
 * What a volume's `connectionState` means to each consumer, as named predicates.
 *
 * ❌ Never test a `connectionState` with `!= null`. Four backends carry one now
 * (SMB, SFTP, WebDAV, ADB) plus a saved-but-unconnected server, so "has a value"
 * stopped answering any of the questions callers actually ask: whether the
 * reconnect manager should run a backoff cycle, whether an answer this volume
 * just gave can be trusted, whether a Disconnect control has a subject. A phone
 * on its "Allow" prompt starting a backoff loop is the shape of failure this
 * module exists to prevent.
 *
 * The Rust twin of `isLiveSession` is `ConnectionState::is_live`
 * (`crates/cmdr-fs/src/volume/connection.rs`).
 */

import type { ConnectionState } from '../types'

/** `null` and `undefined` both arrive: Rust's `Option::None` serializes to `null`. */
type MaybeState = ConnectionState | null | undefined

/**
 * Whether the per-volume reconnect manager owns this volume's recovery, so a
 * pane sitting on it subscribes.
 *
 * `needs_host_key_approval` is IN: the subscription is what lets the pane render
 * the changed-key banner and hear the state move once the user has looked at the
 * key. `saved` is OUT: a greyed row has nothing in flight, and activating it
 * dials rather than recovers.
 */
export function hasReconnectLoop(state: MaybeState): boolean {
  return (
    state === 'direct' ||
    state === 'os_mount' ||
    state === 'disconnected' ||
    state === 'needs_sign_in' ||
    state === 'needs_host_key_approval'
  )
}

/**
 * Whether the session is serving requests right now, so an answer it gives can be
 * taken at face value.
 */
export function isLiveSession(state: MaybeState): boolean {
  return state === 'direct' || state === 'os_mount'
}

/**
 * Whether the row's eject slot shows Disconnect rather than Eject.
 *
 * The real question is "is a volume REGISTERED under this id", because that is
 * what `disconnect_place` takes away. `disconnected` is IN for that reason, and
 * so are BOTH sign-in states: a session that stopped for a missing credential or
 * a host key that no longer matches is still a session to drop, and refusing them
 * left a reachable dead end — a share that fell to `needs_sign_in` could be
 * signed into or forgotten and never simply dropped. The changed-key banner's own
 * Disconnect is the documented way OUT of a key that stopped matching.
 *
 * ❌ `saved` is out: a greyed row was never connected, so the word would promise
 * an action with no subject. ❌ `os_mount` is out too, and that one is about the
 * WORD rather than the subject: an OS mount says Eject, and `isLiveSession` is
 * what puts a control on it.
 */
export function showsDisconnect(state: MaybeState): boolean {
  return (
    state === 'direct' || state === 'disconnected' || state === 'needs_sign_in' || state === 'needs_host_key_approval'
  )
}
