import { describe, expect, it } from 'vitest'
import { isVolumeEjectable } from './eject-predicate'
import type { VolumeInfo } from '../types'

function makeVolume(overrides: Partial<VolumeInfo>): VolumeInfo {
  return {
    id: 'test',
    name: 'Test',
    path: '/Volumes/Test',
    category: 'attached_volume',
    isEjectable: false,
    ...overrides,
  }
}

describe('isVolumeEjectable', () => {
  it('returns false for undefined', () => {
    expect(isVolumeEjectable(undefined)).toBe(false)
  })

  it('returns false for a non-ejectable local volume', () => {
    expect(isVolumeEjectable(makeVolume({ id: 'root', category: 'main_volume' }))).toBe(false)
  })

  it('returns false when connectionState is null (Rust None → JSON null)', () => {
    // Regression: an earlier version checked `!== undefined`, which is true for
    // null too, so every non-SMB volume falsely qualified as ejectable. The
    // bindings type says `ConnectionState | undefined` but the wire value is
    // null. The predicate must reject both.
    const v = makeVolume({ id: 'root', category: 'main_volume' })
    // The bindings type only allows `ConnectionState | undefined`. We need
    // to inject the actual wire shape (null), so we widen to a writable index
    // signature for this one assignment.
    ;(v as unknown as { connectionState: null }).connectionState = null
    expect(isVolumeEjectable(v)).toBe(false)
  })

  it('returns true when isEjectable is set (USB, SD, DMG, MTP)', () => {
    expect(isVolumeEjectable(makeVolume({ isEjectable: true }))).toBe(true)
  })

  it('returns true for an SMB volume in Direct state, even when isEjectable is false', () => {
    // NSURL reports false for SMB mounts; the SMB connection state is the signal.
    expect(isVolumeEjectable(makeVolume({ connectionState: 'direct' }))).toBe(true)
  })

  it('returns true for an SMB volume in OsMount state', () => {
    expect(isVolumeEjectable(makeVolume({ connectionState: 'os_mount' }))).toBe(true)
  })

  it('returns true for an SMB volume in Disconnected state', () => {
    // Lets the user dismiss a disconnected share that's still mounted by the OS.
    expect(isVolumeEjectable(makeVolume({ connectionState: 'disconnected' }))).toBe(true)
  })

  it('returns false for a saved server that was never connected', () => {
    // The greyed row with the hollow dot: nothing is mounted and no session is
    // open, so neither Eject nor Disconnect has a subject. Under the old
    // `!= null` test every saved server would have offered one.
    expect(isVolumeEjectable(makeVolume({ connectionState: 'saved' }))).toBe(false)
  })

  /**
   * ❗ Both sign-in states mean a REGISTERED volume whose session stopped, which
   * is a thing to drop. Refusing them left a reachable dead end: an SMB share
   * (whose `isEjectable` is `false` from NSURL) or an SFTP place that fell to
   * `needs_sign_in` could be signed into or forgotten and never simply dropped,
   * and the changed-key banner's own Disconnect is the documented way OUT of a
   * host key that stopped matching.
   */
  it('returns true for a server waiting on a sign-in or a host key', () => {
    expect(isVolumeEjectable(makeVolume({ connectionState: 'needs_sign_in' }))).toBe(true)
    expect(isVolumeEjectable(makeVolume({ connectionState: 'needs_host_key_approval' }))).toBe(true)
  })

  /**
   * ❗ A phone's row carries `isEjectable: true` unconditionally
   * (`device_volumes.rs`) and its `connectionState` is ALWAYS `null` (readiness
   * is presence, never session health), so the plain predicate offered a live
   * Disconnect on every device row — including a greyed `unavailable` one that
   * cannot even be opened. Readiness is what answers for a device.
   */
  it('offers a phone a Disconnect only once it is ready', () => {
    const phone = (readiness: VolumeInfo['deviceReadiness']) =>
      makeVolume({
        id: 'adb-pixel-7-a1b2c3d',
        path: 'adb://R58M12345',
        category: 'mobile_device',
        isEjectable: true,
        deviceReadiness: readiness,
      })
    expect(isVolumeEjectable(phone({ kind: 'ready' }))).toBe(true)
    expect(isVolumeEjectable(phone({ kind: 'waiting_for_authorization' }))).toBe(false)
    expect(isVolumeEjectable(phone({ kind: 'unavailable', reason: 'offline' }))).toBe(false)
  })

  it('leaves an MTP device alone, which earns its Eject by closing the session', () => {
    // MTP rows carry no `deviceReadiness` and a real `isEjectable`, so nothing
    // about the device gate applies to them.
    expect(isVolumeEjectable(makeVolume({ id: 'mtp-336592896:65537', isEjectable: true }))).toBe(true)
  })

  it('returns false for cloud drives (iCloud / Dropbox / etc.)', () => {
    expect(isVolumeEjectable(makeVolume({ category: 'cloud_drive' }))).toBe(false)
  })

  it('returns false for favorites', () => {
    expect(isVolumeEjectable(makeVolume({ category: 'favorite' }))).toBe(false)
  })
})
