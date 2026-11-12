import { describe, it, expect, vi, beforeEach } from 'vitest'
import type { VolumeInfo } from '../types'
import type { NavigateIntent, NavigateResult } from './navigate'

const { resolvePathVolumeSpy } = vi.hoisted(() => ({
  resolvePathVolumeSpy: vi.fn<() => Promise<{ volume: { id: string } | null }>>(),
}))

vi.mock('#lib/tauri-commands/index.js', () => ({
  resolvePathVolume: resolvePathVolumeSpy,
  // Picking a favorite reports `favorite_opened`; fire-and-forget, so a stub suffices.
  trackEvent: vi.fn(() => Promise.resolve()),
}))
vi.mock('#lib/ui/toast/index.js', () => ({ addToast: vi.fn(), addToastForPane: vi.fn() }))
vi.mock('#lib/settings/settings-window.js', () => ({ openSettingsWindow: vi.fn() }))
vi.mock('#lib/logging/logger.js', () => ({
  getAppLogger: () => ({ warn: vi.fn(), info: vi.fn(), error: vi.fn(), debug: vi.fn() }),
}))

import { createVolumeSelection, type VolumeSelectionDeps } from './volume-selection'

function vol(over: Partial<VolumeInfo>): VolumeInfo {
  return { id: 'v', name: 'V', path: '/v', category: 'device', ...over } as unknown as VolumeInfo
}

function setup(volumes: VolumeInfo[]) {
  const started: NavigateResult = { status: 'started', settled: Promise.resolve(), corrected: Promise.resolve() }
  const navigate = vi.fn<(intent: NavigateIntent) => NavigateResult>(() => started)
  const deps: VolumeSelectionDeps = { getVolumes: () => volumes, navigate }
  return { ops: createVolumeSelection(deps), navigate, started }
}

describe('createVolumeSelection', () => {
  beforeEach(() => vi.clearAllMocks())

  it('selectVolumeByName("Servers") navigates to the synthetic hub volume', async () => {
    // The hub row is synthesized by `volume-grouping.ts`, so no `findIndex` over the volume list
    // can reach it.
    const { ops, navigate, started } = setup([])
    const outcome = await ops.selectVolumeByName('left', 'Servers')
    expect(outcome).toEqual({ kind: 'selected', volumeId: 'network', navigation: started })
    expect(navigate).toHaveBeenCalledWith({
      pane: 'left',
      to: { selectVolume: { volumeId: 'network', path: 'smb://' } },
      source: 'user',
    })
  })

  it('selectVolumeByName reaches the hub by either name when the UI speaks another language', async () => {
    // Regression: an agent reads `Servers` in `cmdr://volumes`, while a Czech UI calls it `Servery`.
    const { setLocale } = await import('#lib/intl/messages.svelte.js')
    await setLocale('cs')
    try {
      const { ops, started } = setup([])
      const hub = { kind: 'selected', volumeId: 'network', navigation: started }
      expect(await ops.selectVolumeByName('left', 'Servers')).toEqual(hub)
      expect(await ops.selectVolumeByName('left', 'Servery')).toEqual(hub)
    } finally {
      await setLocale(null)
    }
  })

  it('selectVolumeByName for a real volume opens it at its root', async () => {
    const { ops, navigate, started } = setup([vol({ id: 'usb', name: 'USB', path: '/Volumes/USB' })])
    const outcome = await ops.selectVolumeByName('right', 'USB')
    expect(outcome).toEqual({ kind: 'selected', volumeId: 'usb', navigation: started })
    expect(navigate).toHaveBeenCalledWith({
      pane: 'right',
      to: { selectVolume: { volumeId: 'usb', path: '/Volumes/USB' } },
      source: 'user',
    })
  })

  it('selectVolumeById chooses the exact connection when MTP and ADB share a phone name', async () => {
    const { ops, navigate, started } = setup([
      vol({ id: 'mtp-pixel:1', name: 'Pixel 9', path: 'mtp://pixel/1', category: 'mobile_device' }),
      vol({ id: 'adb-pixel', name: 'Pixel 9', path: 'adb://pixel', category: 'mobile_device' }),
    ])

    const outcome = await ops.selectVolumeById('left', 'adb-pixel')

    expect(outcome).toEqual({ kind: 'selected', volumeId: 'adb-pixel', navigation: started })
    expect(navigate).toHaveBeenCalledExactlyOnceWith({
      pane: 'left',
      to: { selectVolume: { volumeId: 'adb-pixel', path: 'adb://pixel' } },
      source: 'user',
    })
  })

  it('selectVolumeByName refuses an ambiguous MTP and ADB phone name', async () => {
    const { ops, navigate } = setup([
      vol({ id: 'mtp-pixel:1', name: 'Pixel 9', path: 'mtp://pixel/1', category: 'mobile_device' }),
      vol({ id: 'adb-pixel', name: 'Pixel 9', path: 'adb://pixel', category: 'mobile_device' }),
    ])

    expect(await ops.selectVolumeByName('left', 'Pixel 9')).toEqual({ kind: 'not-found' })
    expect(navigate).not.toHaveBeenCalled()
  })

  // The palette and MCP walk the same list the switcher's greyed row comes from.
  it('refuses a phone whose MTP session is still connecting, by name and by id', async () => {
    const { ops, navigate } = setup([
      vol({
        id: 'mtp-pixel:waiting',
        name: 'Pixel 9',
        path: 'mtp://mtp-pixel/waiting',
        category: 'mobile_device',
        deviceReadiness: { kind: 'connecting' },
      }),
    ])

    expect(await ops.selectVolumeByName('left', 'Pixel 9')).toEqual({ kind: 'not-found' })
    expect(await ops.selectVolumeById('left', 'mtp-pixel:waiting')).toEqual({ kind: 'not-found' })
    expect(navigate).not.toHaveBeenCalled()
  })

  it('selectVolumeByName for a saved server place opens it on its start folder', async () => {
    const place = vol({
      id: 'sftp-nas',
      name: 'Testnas',
      path: 'sftp://test_user_1@testnas.local:22/srv/data',
      category: 'network',
      connectionState: 'saved',
      landingPath: 'sftp://test_user_1@testnas.local:22/srv/data/photos',
    })
    const { ops, navigate } = setup([place])
    expect(await ops.selectVolumeByName('left', 'Testnas')).toMatchObject({ kind: 'selected', volumeId: 'sftp-nas' })
    expect(navigate).toHaveBeenCalledWith({
      pane: 'left',
      to: { selectVolume: { volumeId: 'sftp-nas', path: 'sftp://test_user_1@testnas.local:22/srv/data/photos' } },
      source: 'user',
    })
  })

  it('selectVolumeByName for a connected server place opens it at its root, where the switch picks the path', async () => {
    const place = vol({
      id: 'sftp-nas',
      name: 'Testnas',
      path: 'sftp://test_user_1@testnas.local:22/srv/data',
      category: 'network',
      connectionState: 'direct',
      landingPath: 'sftp://test_user_1@testnas.local:22/srv/data/photos',
    })
    const { ops, navigate } = setup([place])
    expect(await ops.selectVolumeByName('left', 'Testnas')).toMatchObject({ kind: 'selected', volumeId: 'sftp-nas' })
    expect(navigate).toHaveBeenCalledWith({
      pane: 'left',
      to: { selectVolume: { volumeId: 'sftp-nas', path: 'sftp://test_user_1@testnas.local:22/srv/data' } },
      source: 'user',
    })
  })

  it('selectVolumeByName for a favorite navigates to its path on the containing volume', async () => {
    resolvePathVolumeSpy.mockResolvedValue({ volume: { id: 'root' } })
    const { ops, navigate, started } = setup([
      vol({ id: 'fav', name: 'Docs', path: '/Users/me/Docs', category: 'favorite' }),
    ])
    const outcome = await ops.selectVolumeByName('left', 'Docs')
    // The volume a favorite selects is its CONTAINING one: that's where the pane lands.
    expect(outcome).toEqual({ kind: 'selected', volumeId: 'root', navigation: started })
    expect(resolvePathVolumeSpy).toHaveBeenCalledWith('/Users/me/Docs')
    expect(navigate).toHaveBeenCalledWith({
      pane: 'left',
      to: { selectVolume: { volumeId: 'root', path: '/Users/me/Docs' } },
      source: 'user',
      exact: true,
    })
  })

  it('selectVolumeByName refuses a favorite whose containing volume does not resolve', async () => {
    // ❗ It used to fall back to volume `root` at the raw path, which for a dead
    // `search-results://` favorite evicts the pane and then errors about a path
    // nobody typed. `navigation/open-favorite.ts` refuses instead, and a volume
    // this pane can't be sent to is exactly what `not-found` means here.
    resolvePathVolumeSpy.mockResolvedValue({ volume: null })
    const { ops, navigate } = setup([
      vol({ id: 'fav', name: 'Old search', path: 'search-results://dead-id', category: 'favorite' }),
    ])
    expect(await ops.selectVolumeByName('left', 'Old search')).toMatchObject({
      kind: 'unreachable-favorite',
      refusal: { kind: 'not_found' },
    })
    expect(navigate).not.toHaveBeenCalled()
  })

  it('selectVolumeById enters a favorite on an unmounted saved share at its target, which dials', async () => {
    const { ops, navigate, started } = setup([
      vol({
        id: 'fav-1',
        name: 'Docs',
        path: '/Volumes/testshare/docs',
        category: 'favorite',
        favoriteTarget: {
          volumeId: 'smb-testshare',
          volumeName: 'testshare',
          volumeRoot: '/Volumes/testshare',
          reach: { kind: 'connects' },
        },
      }),
    ])
    expect(await ops.selectVolumeById('left', 'fav-1')).toEqual({
      kind: 'selected',
      volumeId: 'smb-testshare',
      navigation: started,
    })
    expect(resolvePathVolumeSpy).not.toHaveBeenCalled()
    expect(navigate).toHaveBeenCalledWith({
      pane: 'left',
      to: { selectVolume: { volumeId: 'smb-testshare', path: '/Volumes/testshare/docs' } },
      source: 'user',
      exact: true,
    })
  })

  it('selectVolumeById reports the surface that picked a favorite (the Dock tile menu)', async () => {
    const { trackEvent } = await import('#lib/tauri-commands/index.js')
    const { ops } = setup([
      vol({
        id: 'fav-3',
        name: 'Docs',
        path: '/Users/me/Docs',
        category: 'favorite',
        favoriteTarget: { volumeId: 'root', volumeName: 'Macintosh HD', volumeRoot: '/', reach: { kind: 'ready' } },
      }),
    ])
    await ops.selectVolumeById('left', 'fav-3', { surface: 'dock', via: 'dock' })
    expect(trackEvent).toHaveBeenCalledWith('favorite_opened', { surface: 'dock', via: 'dock', reach: 'ready' })
  })

  it('selectVolumeById words why a favorite on a forgotten server can`t open, for the MCP reply', async () => {
    const { ops, navigate } = setup([
      vol({
        id: 'fav-2',
        name: 'Docs',
        path: '/Volumes/testshare/docs',
        category: 'favorite',
        favoriteTarget: {
          volumeId: 'smb-testshare',
          volumeName: 'testshare',
          volumeRoot: null,
          reach: { kind: 'forgotten' },
        },
      }),
    ])
    const outcome = await ops.selectVolumeById('left', 'fav-2')
    expect(outcome).toMatchObject({ kind: 'unreachable-favorite', refusal: { kind: 'forgotten' } })
    expect(outcome.kind === 'unreachable-favorite' && outcome.message).toContain('testshare')
    expect(navigate).not.toHaveBeenCalled()
  })

  it('selectVolumeByName says not-found and does not navigate when the name is unknown', async () => {
    const { ops, navigate } = setup([vol({ name: 'USB' })])
    expect(await ops.selectVolumeByName('left', 'Nope')).toEqual({ kind: 'not-found' })
    expect(navigate).not.toHaveBeenCalled()
  })

  it('selectVolumeById says not-found and does not navigate when the id is unknown', async () => {
    const { ops, navigate } = setup([vol({ id: 'usb', name: 'USB' })])
    expect(await ops.selectVolumeById('left', 'missing')).toEqual({ kind: 'not-found' })
    expect(navigate).not.toHaveBeenCalled()
  })

  it('selectVolumeByIndex out of range says not-found', async () => {
    const { ops, navigate } = setup([vol({})])
    expect(await ops.selectVolumeByIndex('left', 5)).toEqual({ kind: 'not-found' })
    expect(navigate).not.toHaveBeenCalled()
  })
})
