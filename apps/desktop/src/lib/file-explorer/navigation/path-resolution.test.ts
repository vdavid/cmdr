/**
 * The walk-up, and the one place it must NOT walk.
 *
 * A scheme path (`sftp://…`, `mtp://…`, `adb://…`) names a volume with no local
 * mount, so every `pathExists` probe on it is false. The plain walk-up then
 * chops the scheme itself (`sftp://ada@nas:22` → `sftp:/` → `sftp:`), gives up,
 * and answers `~`, which lands the pane on the boot disk's home folder. Four of
 * this function's six callers hand a `null` to `navigateToFallback`, which turns
 * it into `~` on the ROOT volume, so `null` is the same failure spelled
 * differently. The guard below is what keeps a restored server tab on its server.
 */
import { describe, it, expect, vi } from 'vitest'
import { resolveValidPath } from './path-resolution'

/** A probe that says no to everything, the way a remote path always answers. */
const nothingExists = vi.fn(() => Promise.resolve(false))

describe('resolveValidPath on a scheme path', () => {
  it('answers the scheme root rather than walking off the volume', async () => {
    const resolved = await resolveValidPath('sftp://ada@nas.local:22/srv/data/photos', {
      pathExistsFn: nothingExists,
      timeoutMs: 0,
    })
    expect(resolved).toBe('sftp://ada@nas.local:22')
  })

  it('❌ never answers `~`, `/`, or null for one', async () => {
    for (const path of [
      'webdav://ada@nas.local:5006/dav/files/ada',
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
    const resolved = await resolveValidPath('sftp://ada@nas.local:22/srv/data/photos', {
      pathExistsFn: exists,
      timeoutMs: 0,
    })
    expect(resolved).toBe('sftp://ada@nas.local:22/srv/data/photos')
    expect(exists).toHaveBeenCalledTimes(1)
  })

  it('walks up within the server before settling on its root', async () => {
    const exists = vi.fn((p: string) => Promise.resolve(p === 'sftp://ada@nas.local:22/srv'))
    const resolved = await resolveValidPath('sftp://ada@nas.local:22/srv/data/photos', {
      pathExistsFn: exists,
      timeoutMs: 0,
    })
    expect(resolved).toBe('sftp://ada@nas.local:22/srv')
  })

  it('leaves a local walk-up exactly as it was', async () => {
    const exists = vi.fn((p: string) => Promise.resolve(p === '/Users/ada'))
    expect(await resolveValidPath('/Users/ada/gone/deeper', { pathExistsFn: exists, timeoutMs: 0 })).toBe('/Users/ada')
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
