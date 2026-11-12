/**
 * The eject control's words per subject and per OS: a drive says Eject everywhere, a mounted
 * network share says each OS's own word (Finder's Eject, File Explorer's Disconnect, the Linux file
 * manager's Unmount), and a phone says Disconnect (#1117).
 */
import { describe, it, expect, beforeAll, afterAll, beforeEach } from 'vitest'
import { _setLocaleForTests } from '#lib/intl/locale.js'
import { withPlatform } from '#lib/platform/test-platform.js'
import type { Platform } from '#lib/platform/index.js'
import { detachControlFor, detachSubjectOf } from './detach-control'
import type { VolumeInfo } from '../types'

const drive: VolumeInfo = {
  id: 'vol-1234',
  name: 'Stick',
  path: '/Volumes/Stick',
  category: 'attached_volume',
  isEjectable: true,
}

/** A share on its OS mount: macOS `/Volumes/…` with `os_mount`, or a Linux GVFS row with no state. */
const osMountedShare: VolumeInfo = {
  id: 'smb-naspi-media',
  name: 'media',
  path: '/Volumes/media',
  category: 'attached_volume',
  isEjectable: true,
  connectionState: 'os_mount',
}
const gvfsShare: VolumeInfo = {
  ...osMountedShare,
  path: '/run/user/1000/gvfs/smb-share:server=naspi,share=media',
  category: 'network',
  connectionState: null,
}
const directShare: VolumeInfo = { ...osMountedShare, connectionState: 'direct' }

const idle = { busy: false, ejecting: false }
const busy = { busy: true, ejecting: false }
const ejecting = { busy: false, ejecting: true }

beforeAll(() => {
  _setLocaleForTests('en-US')
})
afterAll(() => {
  _setLocaleForTests(null)
})

describe('detachSubjectOf', () => {
  it('tells a share from a drive and a phone off the volume id, whatever the session state', () => {
    expect(detachSubjectOf(drive)).toBe('drive')
    expect(detachSubjectOf(osMountedShare)).toBe('share')
    expect(detachSubjectOf(gvfsShare)).toBe('share')
    expect(detachSubjectOf(directShare)).toBe('share')
    expect(detachSubjectOf({ id: 'adb-pixel' })).toBe('phone')
    expect(detachSubjectOf({ id: 'mtp-39041FDJH00A0K' })).toBe('drive')
  })
})

const shareWords: Record<Platform, { idle: string; ejecting: string; busy: string }> = {
  macos: {
    idle: 'Eject media',
    ejecting: 'Ejecting media…',
    busy: 'Cmdr is still working with files on this device. You can eject it once that’s done.',
  },
  windows: {
    idle: 'Disconnect media',
    ejecting: 'Disconnecting media…',
    busy: 'Cmdr is still working with files on this device. You can disconnect it once that’s done.',
  },
  linux: {
    idle: 'Unmount media',
    ejecting: 'Unmounting media…',
    busy: 'Cmdr is still working with files on this device. You can unmount it once that’s done.',
  },
}

describe.each(['macos', 'windows', 'linux'] as const)('detachControlFor on %s', (platform) => {
  beforeEach(() => withPlatform(platform))

  it('words a mounted share with this OS’s verb, in every state', () => {
    for (const share of [osMountedShare, gvfsShare, directShare]) {
      expect(detachControlFor(share, idle)?.button).toMatchObject({ label: shareWords[platform].idle, icon: 'eject' })
      expect(detachControlFor(share, ejecting)?.button.label).toBe(shareWords[platform].ejecting)
      expect(detachControlFor(share, busy)?.button.label).toBe(shareWords[platform].busy)
    }
  })

  it('keeps Eject for a drive', () => {
    expect(detachControlFor(drive, idle)?.button.label).toBe('Eject Stick')
    expect(detachControlFor(drive, ejecting)?.button.label).toBe('Ejecting Stick…')
  })
})
