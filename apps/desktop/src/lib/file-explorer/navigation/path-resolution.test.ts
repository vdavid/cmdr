/**
 * The walk-up, and the one place it must NOT walk.
 *
 * A scheme path (`sftp://…`, `mtp://…`, `adb://…`) names a volume with no local
 * mount, so every `pathExists` probe on it is false. The plain walk-up then
 * chops the scheme itself (`sftp://test_user_1@nas:22` → `sftp:/` → `sftp:`), gives up,
 * and answers `~`, which lands the pane on the boot disk's home folder. Four of
 * this function's six callers hand a `null` to `navigateToFallback`, which turns
 * it into `~` on the ROOT volume, so `null` is the same failure spelled
 * differently. The guard below is what keeps a restored server tab on its server.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest'

const { pathExists, pathExistsChecked } = vi.hoisted(() => ({
  pathExists: vi.fn((_path: string, _volumeId?: string): Promise<boolean> => Promise.resolve(false)),
  pathExistsChecked: vi.fn((_path: string, _volumeId?: string): Promise<{ data: boolean; timedOut: boolean }> =>
    Promise.resolve({ data: false, timedOut: false }),
  ),
}))
vi.mock('#lib/tauri-commands/index.js', () => ({ pathExists, pathExistsChecked }))

import { resolveValidPath } from './path-resolution'

/** A probe that says no to everything, the way a remote path always answers. */
const nothingExists = vi.fn(() => Promise.resolve(false))

describe('resolveValidPath on a scheme path', () => {
  it('answers the scheme root rather than walking off the volume', async () => {
    const resolved = await resolveValidPath('sftp://test_user_1@testnas.local:22/srv/data/photos', {
      pathExistsFn: nothingExists,
      timeoutMs: 0,
    })
    expect(resolved).toBe('sftp://test_user_1@testnas.local:22')
  })

  it('❌ never answers `~`, `/`, or null for one', async () => {
    for (const path of [
      'webdav://test_user_1@testnas.local:5006/dav/files/test_user_1',
      'mtp://device-1/65537/DCIM',
      'adb://R58M12345/sdcard/Download',
    ]) {
      const resolved = await resolveValidPath(path, { pathExistsFn: nothingExists, timeoutMs: 0 })
      expect(resolved, path).not.toBeNull()
      expect(resolved, path).not.toBe('~')
      expect(resolved, path).not.toBe('/')
      expect(resolved?.startsWith(path.split('://')[0]), path).toBe(true)
    }
  })

  /**
   * ❗ A guard keyed on `://` is one character wide, and what slips past it is
   * not merely an odd-looking path: the walk chops `sftp:/srv/data` to `sftp:`,
   * then to `/`, and answers `~` on the BOOT DISK, which is the exact failure
   * this module exists to prevent.
   */
  it('stops on a scheme path spelled with ONE slash too', async () => {
    const resolved = await resolveValidPath('sftp:/srv/data/photos', {
      pathExistsFn: nothingExists,
      timeoutMs: 0,
    })
    expect(resolved).not.toBeNull()
    expect(resolved).not.toBe('~')
    expect(resolved).not.toBe('/')
    expect(resolved?.startsWith('sftp:')).toBe(true)
  })

  it('takes a path that DOES answer, without walking at all', async () => {
    const exists = vi.fn(() => Promise.resolve(true))
    const resolved = await resolveValidPath('sftp://test_user_1@testnas.local:22/srv/data/photos', {
      pathExistsFn: exists,
      timeoutMs: 0,
    })
    expect(resolved).toBe('sftp://test_user_1@testnas.local:22/srv/data/photos')
    expect(exists).toHaveBeenCalledTimes(1)
  })

  it('walks up within the server before settling on its root', async () => {
    const exists = vi.fn((p: string) => Promise.resolve(p === 'sftp://test_user_1@testnas.local:22/srv'))
    const resolved = await resolveValidPath('sftp://test_user_1@testnas.local:22/srv/data/photos', {
      pathExistsFn: exists,
      timeoutMs: 0,
    })
    expect(resolved).toBe('sftp://test_user_1@testnas.local:22/srv')
  })

  it('leaves a local walk-up exactly as it was', async () => {
    const exists = vi.fn((p: string) => Promise.resolve(p === '/Users/test_user_1'))
    expect(await resolveValidPath('/Users/test_user_1/gone/deeper', { pathExistsFn: exists, timeoutMs: 0 })).toBe(
      '/Users/test_user_1',
    )
    expect(await resolveValidPath('/nowhere', { pathExistsFn: nothingExists, timeoutMs: 0 })).toBeNull()
  })
})

describe('resolveValidPath with a volume root on the same scheme', () => {
  it('stops at the VOLUME root, which can sit below the scheme root', async () => {
    // An MTP storage is `mtp://<device>/<storage>`. Stopping at `mtp://<device>`
    // would leave the pane off its own volume.
    const resolved = await resolveValidPath('mtp://device-1/65537/DCIM/Camera', {
      pathExistsFn: nothingExists,
      timeoutMs: 0,
      volumeRoot: 'mtp://device-1/65537',
    })
    expect(resolved).toBe('mtp://device-1/65537')
  })
})

/**
 * A slow FUSE mount (pCloud's `~/pCloud Drive`) whose root doesn't answer in time. Pre-fix
 * the walk skipped the silent root, `~` answered, and a pick of that drive showed the home
 * folder on Macintosh HD as if it were the drive (cmdr-reports#4).
 */
describe('resolveValidPath on a volume whose root does not answer', () => {
  const root = '/Users/test_user_1/pCloud Drive'
  /** The volume's rungs never answer; `~` and `/` do. */
  const silentVolume = vi.fn((p: string) =>
    p === '~' || p === '/' ? Promise.resolve(true) : new Promise<boolean>(() => {}),
  )

  it('stays on the volume root when the caller keeps a silent volume', async () => {
    const resolved = await resolveValidPath(`${root}/Docs`, {
      pathExistsFn: silentVolume,
      timeoutMs: 10,
      volumeRoot: root,
      keepSilentVolume: true,
    })
    expect(resolved).toBe(root)
  })

  it('stays when the backend says it could not tell, too', async () => {
    pathExistsChecked.mockImplementation((path) =>
      Promise.resolve(path === '~' ? { data: true, timedOut: false } : { data: false, timedOut: true }),
    )
    const resolved = await resolveValidPath(root, {
      volumeId: 'pcloud',
      volumeRoot: root,
      timeoutMs: 0,
      keepSilentVolume: true,
    })
    expect(resolved).toBe(root)
  })

  it('still leaves a volume whose root says it is gone', async () => {
    const goneVolume = vi.fn((p: string) => Promise.resolve(p === '~'))
    const resolved = await resolveValidPath(`${root}/Docs`, {
      pathExistsFn: goneVolume,
      timeoutMs: 10,
      volumeRoot: root,
      keepSilentVolume: true,
    })
    expect(resolved).toBe('~')
  })

  it('leaves a silent volume for a caller that walks to leave it (the SMB handlers)', async () => {
    const resolved = await resolveValidPath(`${root}/Docs`, {
      pathExistsFn: silentVolume,
      timeoutMs: 10,
      volumeRoot: root,
    })
    expect(resolved).toBe('~')
  })
})

describe('resolveValidPath on the volume it was given', () => {
  beforeEach(() => {
    pathExists.mockReset()
    pathExistsChecked.mockReset()
    pathExistsChecked.mockResolvedValue({ data: false, timedOut: false })
  })

  /**
   * Pre-fix every probe went out with no volume id, so the backend asked the
   * Mac's boot disk about `sftp://…`, heard "gone" at every level, and the pane
   * landed on the server root instead of the parent that was still there.
   */
  it('lands on the nearest parent that exists there, asking that volume every time', async () => {
    pathExistsChecked.mockImplementation((path, volumeId) =>
      Promise.resolve({
        data: volumeId === 'sftp-nas' && path === 'sftp://test_user_1@testnas.local:22/a',
        timedOut: false,
      }),
    )
    const resolved = await resolveValidPath('sftp://test_user_1@testnas.local:22/a/b/c', {
      volumeId: 'sftp-nas',
      volumeRoot: 'sftp://test_user_1@testnas.local:22',
      timeoutMs: 0,
    })
    expect(resolved).toBe('sftp://test_user_1@testnas.local:22/a')
    expect(pathExistsChecked.mock.calls).toEqual([
      ['sftp://test_user_1@testnas.local:22/a/b/c', 'sftp-nas'],
      ['sftp://test_user_1@testnas.local:22/a/b', 'sftp-nas'],
      ['sftp://test_user_1@testnas.local:22/a', 'sftp-nas'],
    ])
  })

  it('asks the boot disk about `~`, never the volume the walk ran on', async () => {
    pathExists.mockImplementation((path, volumeId) => Promise.resolve(path === '~' && volumeId === undefined))
    const resolved = await resolveValidPath('/Volumes/testshare/gone', {
      volumeId: 'smb-testshare',
      volumeRoot: '/Volumes/testshare',
      timeoutMs: 0,
    })
    expect(resolved).toBe('~')
    expect(pathExistsChecked).toHaveBeenCalledWith('/Volumes/testshare/gone', 'smb-testshare')
    expect(pathExists.mock.calls).toEqual([['~']])
  })
})

describe('resolveValidPath falling back off a deleted folder', () => {
  const onlyRootAndHome = (p: string) => Promise.resolve(p === '/' || p === '~')

  it('lands a deleted top-level folder on the boot disk on `/`, its nearest surviving parent', async () => {
    expect(await resolveValidPath('/foo', { pathExistsFn: onlyRootAndHome, timeoutMs: 0 })).toBe('/')
    expect(await resolveValidPath('/foo/bar', { pathExistsFn: onlyRootAndHome, volumeRoot: '/', timeoutMs: 0 })).toBe(
      '/',
    )
  })

  it('still sends a gone volume home, since its parents are another drive', async () => {
    const resolved = await resolveValidPath('/Volumes/Gone/Docs', {
      pathExistsFn: onlyRootAndHome,
      volumeRoot: '/Volumes/Gone',
      timeoutMs: 0,
    })
    expect(resolved).toBe('~')
  })
})
