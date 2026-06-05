import { describe, it, expect, vi } from 'vitest'
import { checkTransferDestinationGuard, resolveSourceVolumeId } from './transfer-entry'
import { SEARCH_RESULTS_NOT_A_FOLDER_TOAST } from '$lib/search/capabilities'
import type { VolumeInfo } from '../types'
import type { PathVolumeResolution } from '$lib/tauri-commands'

const ROOT: VolumeInfo = {
  id: 'root',
  name: 'Macintosh HD',
  path: '/',
  category: 'main_volume',
  isEjectable: false,
  isReadOnly: false,
}
const EXT: VolumeInfo = {
  id: 'ext',
  name: 'Ext',
  path: '/Volumes/Ext',
  category: 'attached_volume',
  isEjectable: true,
  isReadOnly: false,
}
const SD_CARD: VolumeInfo = {
  id: 'mtp-dev:65538',
  name: 'Virtual Pixel 9 - SD Card',
  path: 'mtp://dev/65538',
  category: 'mobile_device',
  isEjectable: true,
  isReadOnly: true,
}

// Favorites are pseudo-volumes that exist ONLY in the volume picker; the backend
// VolumeManager has no such volume. Their root is a location on the local FS.
const FAV_DESKTOP: VolumeInfo = {
  id: 'fav-desktop',
  name: 'Desktop',
  path: '/Users/me/Desktop',
  category: 'favorite',
  isEjectable: false,
}
const FAV_DOCUMENTS: VolumeInfo = {
  id: 'fav-documents',
  name: 'Documents',
  path: '/Users/me/Documents',
  category: 'favorite',
  isEjectable: false,
}
const SMB_SHARE: VolumeInfo = {
  id: 'smb-server-share',
  name: 'share on server',
  path: 'smb://server/share',
  category: 'network',
  isEjectable: true,
  isReadOnly: false,
}

/**
 * The PRODUCTION-shaped volume list the live drop resolver actually sees: the
 * local root, two favorites (Desktop, Documents — pseudo-volumes the backend
 * doesn't know), an MTP storage, and an SMB share. Mirrors the real volume
 * picker so resolution can't be fooled by a pseudo-volume root.
 */
const PROD_VOLUMES = [ROOT, FAV_DESKTOP, FAV_DOCUMENTS, SD_CARD, SMB_SHARE]

describe('checkTransferDestinationGuard', () => {
  it('allows a writable local destination', () => {
    const result = checkTransferDestinationGuard('root', [ROOT, EXT])
    expect(result.ok).toBe(true)
  })

  it('blocks a read-only destination with the exact "Read-only device" alert F5 shows', () => {
    const result = checkTransferDestinationGuard('mtp-dev:65538', [ROOT, SD_CARD])
    expect(result.ok).toBe(false)
    if (result.ok) throw new Error('expected a block')
    expect(result.alert).toEqual({
      title: 'Read-only device',
      message: '"Virtual Pixel 9 - SD Card" is read-only. You can copy files from it, but not to it.',
    })
    expect(result.toast).toBeUndefined()
  })

  it('blocks a search-results destination with the not-a-folder toast', () => {
    const searchResults: VolumeInfo = {
      id: 'search-results',
      name: 'Search results',
      path: 'search-results://x',
      category: 'main_volume',
      isEjectable: false,
    }
    const result = checkTransferDestinationGuard('search-results', [ROOT, searchResults])
    expect(result.ok).toBe(false)
    if (result.ok) throw new Error('expected a block')
    expect(result.toast).toEqual({ message: SEARCH_RESULTS_NOT_A_FOLDER_TOAST, level: 'warn' })
    expect(result.alert).toBeUndefined()
  })

  it('allows an unknown destination volume id (no VolumeInfo, not a known virtual kind)', () => {
    // Honest degrade: a missing VolumeInfo means we can't prove read-only, so we
    // don't block. The backend still rejects a genuinely read-only write.
    const result = checkTransferDestinationGuard('vanished', [ROOT])
    expect(result.ok).toBe(true)
  })
})

describe('resolveSourceVolumeId', () => {
  const volumes = [ROOT, EXT, SD_CARD]

  it('resolves a single local path via longest-prefix (FE, no BE round-trip)', async () => {
    const resolvePathVolume = vi.fn<(path: string) => Promise<PathVolumeResolution>>()
    const id = await resolveSourceVolumeId(['/Volumes/Ext/photos/a.jpg'], volumes, resolvePathVolume)
    expect(id).toBe('ext')
    expect(resolvePathVolume).not.toHaveBeenCalled()
  })

  it('resolves MTP-shaped dropped paths to the MTP volume via longest-prefix', async () => {
    const resolvePathVolume = vi.fn<(path: string) => Promise<PathVolumeResolution>>()
    const id = await resolveSourceVolumeId(['mtp://dev/65538/DCIM/IMG_0001.JPG'], volumes, resolvePathVolume)
    expect(id).toBe('mtp-dev:65538')
    expect(resolvePathVolume).not.toHaveBeenCalled()
  })

  it('resolves siblings on the same volume via their common parent', async () => {
    const resolvePathVolume = vi.fn<(path: string) => Promise<PathVolumeResolution>>()
    const id = await resolveSourceVolumeId(
      ['/Volumes/Ext/a.txt', '/Volumes/Ext/b.txt', '/Volumes/Ext/sub/c.txt'],
      volumes,
      resolvePathVolume,
    )
    expect(id).toBe('ext')
    expect(resolvePathVolume).not.toHaveBeenCalled()
  })

  it('falls back to the backend resolver when no registered volume root matches', async () => {
    // No `/`-rooted volume in the list, and the path lives under none of the
    // registered roots (e.g. a virtual MTP/SMB path whose volume isn't mounted).
    const resolvePathVolume = vi.fn<(path: string) => Promise<PathVolumeResolution>>().mockResolvedValue({
      volume: EXT,
      timedOut: false,
    })
    const id = await resolveSourceVolumeId(['smb://server/share/sub/file.txt'], [EXT, SD_CARD], resolvePathVolume)
    expect(id).toBe('ext')
    // BE resolver runs against the common parent, not each path.
    expect(resolvePathVolume).toHaveBeenCalledWith('smb://server/share/sub')
  })

  it('falls back to root (honest unknown) when neither FE nor BE can resolve', async () => {
    const resolvePathVolume = vi
      .fn<(path: string) => Promise<PathVolumeResolution>>()
      .mockResolvedValue({ volume: null, timedOut: true })
    const id = await resolveSourceVolumeId(['smb://server/share/sub/file.txt'], [EXT, SD_CARD], resolvePathVolume)
    expect(id).toBe('root')
  })

  it('falls back to root when sources genuinely span volumes (common parent is /)', async () => {
    // /Volumes/Ext/a vs /Users/x/b → common parent "/" which the BE can resolve
    // to root, but the sources don't share a real volume. We report the honest
    // unknown (root) rather than a knowingly-wrong specific volume.
    const resolvePathVolume = vi
      .fn<(path: string) => Promise<PathVolumeResolution>>()
      .mockResolvedValue({ volume: ROOT, timedOut: false })
    const id = await resolveSourceVolumeId(['/Volumes/Ext/a.txt', '/Users/x/b.txt'], volumes, resolvePathVolume)
    expect(id).toBe('root')
    // Spanning volumes resolve to the safe default without even a BE call.
    expect(resolvePathVolume).not.toHaveBeenCalled()
  })

  it('falls back to root for an empty path list', async () => {
    const resolvePathVolume = vi.fn<(path: string) => Promise<PathVolumeResolution>>()
    const id = await resolveSourceVolumeId([], volumes, resolvePathVolume)
    expect(id).toBe('root')
    expect(resolvePathVolume).not.toHaveBeenCalled()
  })

  describe('favorites never poison resolution (production-shaped volume list)', () => {
    it('resolves a Desktop-path drop to the backing local volume (root), NEVER fav-desktop', async () => {
      // A file dragged from the macOS Desktop into Cmdr. The Desktop favorite's
      // root (`/Users/me/Desktop`) is a longer prefix than `/`, so a naive
      // longest-prefix match would pick `fav-desktop` — a pseudo-volume the
      // backend VolumeManager has no record of, making the transfer dispatch
      // fail with "Source volume 'fav-desktop' not found". The favorite is a
      // location on the LOCAL fs, so it must resolve to its backing volume: root.
      const resolvePathVolume = vi.fn<(path: string) => Promise<PathVolumeResolution>>()
      const id = await resolveSourceVolumeId(['/Users/me/Desktop/photo.jpg'], PROD_VOLUMES, resolvePathVolume)
      expect(id).toBe('root')
      expect(id).not.toBe('fav-desktop')
    })

    it('resolves a Documents-path drop to root, not fav-documents', async () => {
      const resolvePathVolume = vi.fn<(path: string) => Promise<PathVolumeResolution>>()
      const id = await resolveSourceVolumeId(['/Users/me/Documents/report.pdf'], PROD_VOLUMES, resolvePathVolume)
      expect(id).toBe('root')
    })

    it('still resolves a real MTP path correctly with favorites present', async () => {
      const resolvePathVolume = vi.fn<(path: string) => Promise<PathVolumeResolution>>()
      const id = await resolveSourceVolumeId(['mtp://dev/65538/DCIM/IMG.JPG'], PROD_VOLUMES, resolvePathVolume)
      expect(id).toBe('mtp-dev:65538')
    })

    it('still resolves a real SMB path correctly with favorites present', async () => {
      const resolvePathVolume = vi.fn<(path: string) => Promise<PathVolumeResolution>>()
      const id = await resolveSourceVolumeId(['smb://server/share/dir/file.txt'], PROD_VOLUMES, resolvePathVolume)
      expect(id).toBe('smb-server-share')
    })
  })
})
