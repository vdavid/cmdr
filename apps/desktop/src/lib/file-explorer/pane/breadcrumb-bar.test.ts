/**
 * Tests for `breadcrumb-bar.ts`, the pane header's path readout and its three
 * interactions. They pin:
 * - the display path per volume kind: `~` for the home prefix on the root
 *   volume, volume-relative for every other real volume, the MTP display form,
 *   and the snapshot label (not a path) on a search-results pane,
 * - a segment click navigating to that ancestor without surfacing a rejection,
 * - the context menu passing eject info only for an ejectable volume,
 * - the volume switch: the new path is committed and loaded, disk-space watching
 *   follows the NEW volume, network and device-only MTP targets skip the load,
 *   and a disk image gets no space poll at all.
 */
import { describe, it, expect, vi, beforeEach, type Mock } from 'vitest'

const { ipc, shortcuts, eject, mtp, volumeStore } = vi.hoisted<{
  ipc: { showBreadcrumbContextMenu: Mock }
  shortcuts: { getEffectiveShortcuts: Mock; toDisplayShortcut: Mock }
  eject: { isVolumeEjectable: Mock }
  mtp: { isMtpVolumeId: Mock; getMtpDisplayPath: Mock }
  volumeStore: { getVolumes: Mock }
}>(() => ({
  ipc: { showBreadcrumbContextMenu: vi.fn() },
  shortcuts: { getEffectiveShortcuts: vi.fn(), toDisplayShortcut: vi.fn() },
  eject: { isVolumeEjectable: vi.fn() },
  mtp: { isMtpVolumeId: vi.fn(), getMtpDisplayPath: vi.fn() },
  volumeStore: { getVolumes: vi.fn() },
}))

vi.mock('$lib/tauri-commands', () => ({ showBreadcrumbContextMenu: ipc.showBreadcrumbContextMenu }))
vi.mock('$lib/shortcuts/shortcuts-store', () => ({ getEffectiveShortcuts: shortcuts.getEffectiveShortcuts }))
vi.mock('$lib/shortcuts/key-capture', () => ({ toDisplayShortcut: shortcuts.toDisplayShortcut }))
vi.mock('../navigation/eject-predicate', () => ({ isVolumeEjectable: eject.isVolumeEjectable }))
vi.mock('$lib/mtp', () => ({ isMtpVolumeId: mtp.isMtpVolumeId, getMtpDisplayPath: mtp.getMtpDisplayPath }))
vi.mock('$lib/stores/volume-store.svelte', () => ({ getVolumes: volumeStore.getVolumes }))

import { breadcrumbDisplayPath, createBreadcrumbHandlers, type BreadcrumbHandlerDeps } from './breadcrumb-bar'

describe('breadcrumbDisplayPath', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    mtp.isMtpVolumeId.mockReturnValue(false)
  })

  it('replaces the home prefix with `~` on the root volume', () => {
    expect(
      breadcrumbDisplayPath({
        currentPath: '/Users/test/Documents',
        volumeId: 'root',
        volumePath: '/',
        userHomePath: '/Users/test',
        isSearchResultsView: false,
        searchLabel: undefined,
      }),
    ).toBe('~/Documents')
  })

  it('shows a bare `~` at the home folder itself', () => {
    expect(
      breadcrumbDisplayPath({
        currentPath: '/Users/test',
        volumeId: 'root',
        volumePath: '/',
        userHomePath: '/Users/test',
        isSearchResultsView: false,
        searchLabel: undefined,
      }),
    ).toBe('~')
  })

  it('leaves an already-`~`-rooted path alone', () => {
    expect(
      breadcrumbDisplayPath({
        currentPath: '~/Documents',
        volumeId: 'root',
        volumePath: '/',
        userHomePath: '/Users/test',
        isSearchResultsView: false,
        searchLabel: undefined,
      }),
    ).toBe('~/Documents')
  })

  it('shows an absolute path outside the home folder as-is', () => {
    expect(
      breadcrumbDisplayPath({
        currentPath: '/etc',
        volumeId: 'root',
        volumePath: '/',
        userHomePath: '/Users/test',
        isSearchResultsView: false,
        searchLabel: undefined,
      }),
    ).toBe('/etc')
  })

  it('strips the volume root on a non-root volume', () => {
    expect(
      breadcrumbDisplayPath({
        currentPath: '/Volumes/Ext/photos',
        volumeId: 'ext',
        volumePath: '/Volumes/Ext',
        userHomePath: '/Users/test',
        isSearchResultsView: false,
        searchLabel: undefined,
      }),
    ).toBe('/photos')
  })

  it('shows `/` at a non-root volume root', () => {
    expect(
      breadcrumbDisplayPath({
        currentPath: '/Volumes/Ext',
        volumeId: 'ext',
        volumePath: '/Volumes/Ext',
        userHomePath: '/Users/test',
        isSearchResultsView: false,
        searchLabel: undefined,
      }),
    ).toBe('/')
  })

  it('uses the MTP display form on an MTP volume', () => {
    mtp.isMtpVolumeId.mockReturnValue(true)
    mtp.getMtpDisplayPath.mockReturnValue('Phone/DCIM')
    expect(
      breadcrumbDisplayPath({
        currentPath: 'mtp://x/DCIM',
        volumeId: 'mtp-1:2',
        volumePath: '/',
        userHomePath: '/Users/test',
        isSearchResultsView: false,
        searchLabel: undefined,
      }),
    ).toBe('Phone/DCIM')
  })

  it('shows the snapshot label as the path on a search-results pane', () => {
    expect(
      breadcrumbDisplayPath({
        currentPath: 'search-results://sr-1',
        volumeId: 'search-results',
        volumePath: '/',
        userHomePath: '/Users/test',
        isSearchResultsView: true,
        searchLabel: '*.pdf',
      }),
    ).toBe('*.pdf')
  })

  it('falls back to `Search` when the snapshot is gone', () => {
    expect(
      breadcrumbDisplayPath({
        currentPath: 'search-results://sr-1',
        volumeId: 'search-results',
        volumePath: '/',
        userHomePath: '/Users/test',
        isSearchResultsView: true,
        searchLabel: undefined,
      }),
    ).toBe('Search')
  })
})

describe('createBreadcrumbHandlers', () => {
  let deps: BreadcrumbHandlerDeps
  let calls: Record<string, Mock>
  let volumes: { id: string; name: string; isDiskImage?: boolean }[]

  beforeEach(() => {
    vi.clearAllMocks()
    shortcuts.getEffectiveShortcuts.mockReturnValue(['cmd+shift+c'])
    shortcuts.toDisplayShortcut.mockReturnValue('⌘⇧C')
    eject.isVolumeEjectable.mockReturnValue(false)
    mtp.isMtpVolumeId.mockReturnValue(false)
    ipc.showBreadcrumbContextMenu.mockResolvedValue(undefined)
    volumes = [{ id: 'ext', name: 'External' }]
    volumeStore.getVolumes.mockImplementation(() => volumes)
    calls = {
      navigateToPath: vi.fn().mockResolvedValue(undefined),
      setCurrentPath: vi.fn(),
      onVolumeChange: vi.fn(),
      onRequestFocus: vi.fn(),
      loadDirectory: vi.fn(),
      refreshSpace: vi.fn(),
      watchSpace: vi.fn(),
      unwatchSpace: vi.fn(),
      clearSpace: vi.fn(),
    }
    deps = {
      getCurrentVolumeInfo: () => ({ id: 'ext', name: 'External' }) as never,
      navigateToPath: calls.navigateToPath,
      setCurrentPath: calls.setCurrentPath,
      onVolumeChange: calls.onVolumeChange,
      onRequestFocus: calls.onRequestFocus,
      loadDirectory: calls.loadDirectory,
      refreshSpace: calls.refreshSpace,
      watchSpace: calls.watchSpace,
      unwatchSpace: calls.unwatchSpace,
      clearSpace: calls.clearSpace,
    }
  })

  it('navigates to the clicked ancestor', () => {
    createBreadcrumbHandlers(deps).handleSegmentClick('/Users/test')
    expect(calls.navigateToPath).toHaveBeenCalledWith('/Users/test')
  })

  it('swallows a failed ancestor navigation (the pane shows the error itself)', async () => {
    calls.navigateToPath.mockRejectedValue(new Error('gone'))
    expect(() => {
      createBreadcrumbHandlers(deps).handleSegmentClick('/gone')
    }).not.toThrow()
    await Promise.resolve()
  })

  it('focuses the pane and offers no eject for a non-ejectable volume', () => {
    const event = { preventDefault: vi.fn() } as unknown as MouseEvent
    createBreadcrumbHandlers(deps).handleContextMenu(event)
    expect(calls.onRequestFocus).toHaveBeenCalledTimes(1)
    expect(ipc.showBreadcrumbContextMenu).toHaveBeenCalledWith('⌘⇧C', undefined, undefined)
  })

  it('passes the volume id and name for an ejectable volume', () => {
    eject.isVolumeEjectable.mockReturnValue(true)
    createBreadcrumbHandlers(deps).handleContextMenu({ preventDefault: vi.fn() } as unknown as MouseEvent)
    expect(ipc.showBreadcrumbContextMenu).toHaveBeenCalledWith('⌘⇧C', 'ext', 'External')
  })

  describe('switching volume', () => {
    it('commits the target path, loads it, and follows it with the space watch', () => {
      createBreadcrumbHandlers(deps).handleVolumeChange({
        volumeId: 'ext',
        volumePath: '/Volumes/Ext',
        targetPath: '/Volumes/Ext/photos',
      })
      expect(calls.setCurrentPath).toHaveBeenCalledWith('/Volumes/Ext/photos')
      expect(calls.onVolumeChange).toHaveBeenCalledWith({
        volumeId: 'ext',
        volumePath: '/Volumes/Ext',
        targetPath: '/Volumes/Ext/photos',
      })
      expect(calls.loadDirectory).toHaveBeenCalledWith('/Volumes/Ext/photos')
      expect(calls.unwatchSpace).toHaveBeenCalledTimes(1)
      expect(calls.watchSpace).toHaveBeenCalledWith('ext', '/Volumes/Ext/photos')
      expect(calls.refreshSpace).toHaveBeenCalledTimes(1)
    })

    it('reads the disk-image flag off the NEW volume, before the prop catches up', () => {
      volumes = [{ id: 'dmg', name: 'Installer', isDiskImage: true }]
      createBreadcrumbHandlers(deps).handleVolumeChange({
        volumeId: 'dmg',
        volumePath: '/Volumes/Installer',
        targetPath: '/Volumes/Installer',
      })
      expect(calls.clearSpace).toHaveBeenCalledTimes(1)
      expect(calls.watchSpace).not.toHaveBeenCalled()
      expect(calls.refreshSpace).not.toHaveBeenCalled()
    })

    it('skips the load for the network volume and stops watching space', () => {
      createBreadcrumbHandlers(deps).handleVolumeChange({ volumeId: 'network', volumePath: '/', targetPath: '/' })
      expect(calls.loadDirectory).not.toHaveBeenCalled()
      expect(calls.unwatchSpace).toHaveBeenCalledTimes(1)
      expect(calls.watchSpace).not.toHaveBeenCalled()
    })

    it('skips the load for a device-only MTP target, which needs connecting first', () => {
      mtp.isMtpVolumeId.mockReturnValue(true)
      createBreadcrumbHandlers(deps).handleVolumeChange({ volumeId: 'mtp-2097152', volumePath: '/', targetPath: '/' })
      expect(calls.loadDirectory).not.toHaveBeenCalled()
    })

    it('loads a connected MTP storage target', () => {
      mtp.isMtpVolumeId.mockReturnValue(true)
      createBreadcrumbHandlers(deps).handleVolumeChange({
        volumeId: 'mtp-2097152:65537',
        volumePath: '/',
        targetPath: '/DCIM',
      })
      expect(calls.loadDirectory).toHaveBeenCalledWith('/DCIM')
    })
  })
})
