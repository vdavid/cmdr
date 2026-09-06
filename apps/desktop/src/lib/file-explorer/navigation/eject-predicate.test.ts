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

  it('returns false for a server waiting on a sign-in or a host key', () => {
    expect(isVolumeEjectable(makeVolume({ connectionState: 'needs_sign_in' }))).toBe(false)
    expect(isVolumeEjectable(makeVolume({ connectionState: 'needs_host_key_approval' }))).toBe(false)
  })

  it('returns false for cloud drives (iCloud / Dropbox / etc.)', () => {
    expect(isVolumeEjectable(makeVolume({ category: 'cloud_drive' }))).toBe(false)
  })

  it('returns false for favorites', () => {
    expect(isVolumeEjectable(makeVolume({ category: 'favorite' }))).toBe(false)
  })
})
