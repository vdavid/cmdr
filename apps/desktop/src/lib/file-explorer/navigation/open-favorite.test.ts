/**
 * The one way a favorite opens.
 *
 * Pinned here and nowhere else: a favorite whose row names its target goes straight
 * there (`volumeId`, `volumeRoot`, its path, `exact`) with no path lookup, so an
 * unmounted share's favorite can't resolve onto the boot disk; `connects` goes the
 * same way (the pane's own connect view dials); every other reach leaves the pane
 * where it is and says why in one toast; and `favorite_opened` fires once per pick,
 * carrying the reach.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest'
import type { FavoriteReach } from '$lib/ipc/bindings'
import type { VolumeInfo } from '$lib/file-explorer/types'

const resolvePathVolume = vi.fn<(path: string) => Promise<{ volume: VolumeInfo | null; timedOut: boolean }>>()
const trackEvent = vi.fn()
const removeFavorite = vi.fn<(id: string) => Promise<void>>()

vi.mock('$lib/tauri-commands', () => ({
  resolvePathVolume: (path: string) => resolvePathVolume(path),
  removeFavorite: (id: string) => removeFavorite(id),
  stripFavoritePrefix: (id: string) => id.replace(/^fav-/, ''),
  trackEvent: (...args: unknown[]) => {
    trackEvent(...(args as []))
    return Promise.resolve()
  },
}))

const addToastForPane = vi.fn()
const addToast = vi.fn()
vi.mock('$lib/ui/toast', () => ({
  addToastForPane: (...args: unknown[]) => {
    addToastForPane(...(args as []))
    return 'toast-id'
  },
  addToast: (...args: unknown[]) => {
    addToast(...(args as []))
    return 'toast-id'
  },
}))

const openSettingsWindow = vi.fn<(surface: string, section?: string[]) => Promise<void>>()
vi.mock('$lib/settings/settings-window', () => ({
  openSettingsWindow: (surface: string, section?: string[]) => openSettingsWindow(surface, section),
}))

vi.mock('$lib/logging/logger', () => ({
  getAppLogger: () => ({ warn: vi.fn(), info: vi.fn() }),
}))

import { openFavorite } from './open-favorite'

const drive: VolumeInfo = {
  id: 'disk1',
  name: 'Macintosh HD',
  path: '/',
  category: 'main_volume',
  isEjectable: false,
}

function favorite(
  reach: FavoriteReach,
  target: { volumeId?: string | null; volumeName?: string | null; volumeRoot?: string | null } = {},
): VolumeInfo {
  return {
    id: 'fav-123',
    name: 'docs',
    path: '/Volumes/naspi/docs',
    category: 'favorite',
    isEjectable: false,
    favoriteTarget: {
      volumeId: target.volumeId === undefined ? 'smb-naspi' : target.volumeId,
      volumeName: target.volumeName === undefined ? 'naspi on nas.local' : target.volumeName,
      volumeRoot: target.volumeRoot === undefined ? '/Volumes/naspi' : target.volumeRoot,
      reach,
    },
  }
}

/** The last toast's props, as `FavoriteRefusalToastContent` receives them. */
function lastToastProps(): { message: string; actionLabel: string | null; onAction: () => void } {
  const call = addToastForPane.mock.calls.at(-1) as [string, unknown, { props: never }]
  return call[2].props
}

beforeEach(() => {
  resolvePathVolume.mockReset()
  trackEvent.mockReset()
  addToastForPane.mockReset()
  addToast.mockReset()
  removeFavorite.mockReset()
  openSettingsWindow.mockReset()
  openSettingsWindow.mockResolvedValue(undefined)
})

describe('openFavorite on a row that names its target', () => {
  it.each<FavoriteReach>([{ kind: 'ready' }, { kind: 'connects' }])(
    'sends the pane to the target volume, at the favorite`s path, exactly ($kind)',
    async (reach) => {
      const go = vi.fn(() => 'went')

      const result = await openFavorite({
        favorite: favorite(reach),
        pane: 'left',
        picked: { surface: 'favorites_menu', via: 'digit' },
        go,
      })

      // No path lookup: for an unmounted share it would walk up to the boot disk.
      expect(resolvePathVolume).not.toHaveBeenCalled()
      expect(go).toHaveBeenCalledWith({
        volumeId: 'smb-naspi',
        volumePath: '/Volumes/naspi',
        targetPath: '/Volumes/naspi/docs',
        exact: true,
      })
      expect(result).toEqual({ kind: 'opened', opened: 'went' })
      expect(addToastForPane).not.toHaveBeenCalled()
    },
  )

  it('emits `favorite_opened` once, with the surface, via, and reach', async () => {
    await openFavorite({
      favorite: favorite({ kind: 'connects' }),
      pane: 'left',
      picked: { surface: 'favorites_menu', via: 'pointer' },
      go: () => undefined,
    })

    expect(trackEvent).toHaveBeenCalledTimes(1)
    expect(trackEvent).toHaveBeenCalledWith('favorite_opened', {
      surface: 'favorites_menu',
      via: 'pointer',
      reach: 'connects',
    })
  })

  it.each<[string, FavoriteReach, string | null]>([
    ['an unplugged phone', { kind: 'unplugged', device: 'phone', reason: null }, null],
    ['an unplugged drive', { kind: 'unplugged', device: 'drive', reason: null }, null],
    ['phones switched off', { kind: 'access_off', backend: 'adb' }, 'Open settings'],
    ['a forgotten server', { kind: 'forgotten' }, 'Show servers'],
    ['a folder that`s gone', { kind: 'not_found' }, 'Remove favorite'],
  ])('leaves the pane put on %s, with one info toast for that pane', async (_label, reach, actionLabel) => {
    const go = vi.fn()

    const result = await openFavorite({
      favorite: favorite(reach),
      pane: 'right',
      picked: { surface: 'favorites_menu', via: 'keyboard' },
      go,
    })

    expect(go).not.toHaveBeenCalled()
    expect(result).toMatchObject({ kind: 'not-opened', refusal: reach })
    expect(addToastForPane).toHaveBeenCalledTimes(1)
    expect(addToastForPane.mock.calls[0][0]).toBe('right')
    expect(addToastForPane.mock.calls[0][2]).toMatchObject({ level: 'info' })
    expect(lastToastProps().actionLabel).toBe(actionLabel)
    // A pick the pane couldn't follow still counts, with its reach.
    expect(trackEvent).toHaveBeenCalledWith('favorite_opened', expect.objectContaining({ reach: reach.kind }))
  })

  it('words a refusal with the volume`s name', async () => {
    const result = await openFavorite({
      favorite: favorite({ kind: 'unplugged', device: 'phone', reason: null }, { volumeName: 'Pixel 7' }),
      pane: 'left',
      picked: { surface: 'command', via: 'command' },
      go: vi.fn(),
    })

    expect(result.kind === 'not-opened' && result.message).toContain('Pixel 7')
    expect(lastToastProps().message).toContain('Pixel 7')
  })

  it('opens the switch`s own Settings section from the toast', async () => {
    await openFavorite({
      favorite: favorite({ kind: 'access_off', backend: 'mtp' }),
      pane: 'left',
      picked: { surface: 'command', via: 'command' },
      go: vi.fn(),
    })

    lastToastProps().onAction()
    expect(openSettingsWindow).toHaveBeenCalledWith('favorite-toast', ['File systems', 'MTP (Android/Kindle/cameras)'])
  })

  it('puts THIS pane on the servers list from a forgotten favorite`s toast', async () => {
    const go = vi.fn()
    await openFavorite({
      favorite: favorite({ kind: 'forgotten' }),
      pane: 'left',
      picked: { surface: 'command', via: 'command' },
      go,
    })

    lastToastProps().onAction()
    expect(go).toHaveBeenCalledWith({ volumeId: 'network', volumePath: 'smb://', targetPath: 'smb://' })
  })

  it('removes the favorite (by its bare id) from a gone folder`s toast', async () => {
    removeFavorite.mockResolvedValue(undefined)
    await openFavorite({
      favorite: favorite({ kind: 'not_found' }),
      pane: 'left',
      picked: { surface: 'command', via: 'command' },
      go: vi.fn(),
    })

    lastToastProps().onAction()
    expect(removeFavorite).toHaveBeenCalledWith('123')
  })
})

describe('openFavorite on a row with no target (an older listing)', () => {
  const bare: VolumeInfo = {
    id: 'fav-9',
    name: 'projects',
    path: '/Users/me/projects',
    category: 'favorite',
    isEjectable: false,
  }

  it('asks which volume contains the path, then goes there', async () => {
    resolvePathVolume.mockResolvedValue({ volume: drive, timedOut: false })
    const go = vi.fn(() => 'went')

    const result = await openFavorite({
      favorite: bare,
      pane: 'left',
      picked: { surface: 'command', via: 'command' },
      go,
    })

    expect(resolvePathVolume).toHaveBeenCalledWith('/Users/me/projects')
    expect(go).toHaveBeenCalledWith({
      volumeId: 'disk1',
      volumePath: '/',
      targetPath: '/Users/me/projects',
      exact: true,
    })
    expect(result).toEqual({ kind: 'opened', opened: 'went' })
    expect(trackEvent).toHaveBeenCalledWith('favorite_opened', expect.objectContaining({ reach: 'ready' }))
  })

  it('navigates NOWHERE when no volume claims the path, and says so', async () => {
    resolvePathVolume.mockResolvedValue({ volume: null, timedOut: false })
    const go = vi.fn()

    const result = await openFavorite({
      favorite: bare,
      pane: 'left',
      picked: { surface: 'command', via: 'command' },
      go,
    })

    expect(go).not.toHaveBeenCalled()
    expect(result).toMatchObject({ kind: 'not-opened', refusal: { kind: 'not_found' } })
    expect(addToastForPane).toHaveBeenCalledTimes(1)
  })
})
