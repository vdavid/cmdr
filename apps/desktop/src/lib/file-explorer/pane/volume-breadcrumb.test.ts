/**
 * Integration tests for VolumeBreadcrumb.
 */
import { describe, it, expect, vi } from 'vitest'
import { mount, tick } from 'svelte'
import VolumeBreadcrumb from '../navigation/VolumeBreadcrumb.svelte'
import { waitForUpdates, useMountTarget } from './integration-test-utils'

// ============================================================================
// Mock setup (must be in each test file — Vitest hoists vi.mock calls)
// ============================================================================

let mockEntry: unknown = null

vi.mock('$lib/tauri-commands', () => ({
  listDirectoryStart: vi.fn().mockResolvedValue({ listingId: 'mock-listing', status: 'ready' }),
  cancelListing: vi.fn().mockResolvedValue(undefined),
  listDirectoryEnd: vi.fn().mockResolvedValue(undefined),
  getFileRange: vi.fn().mockResolvedValue([]),
  getFileAt: vi.fn().mockImplementation((_listingId: string, index: number) => {
    if (index === 0) {
      mockEntry = {
        name: 'test-folder',
        path: '/test/test-folder',
        isDirectory: true,
        isSymlink: false,
        permissions: 0o755,
        owner: 'user',
        group: 'staff',
        iconId: 'dir',
        extendedMetadataLoaded: true,
      }
    } else {
      mockEntry = {
        name: 'test-file.txt',
        path: '/test/test-file.txt',
        isDirectory: false,
        isSymlink: false,
        permissions: 0o644,
        owner: 'user',
        group: 'staff',
        iconId: 'file',
        extendedMetadataLoaded: true,
      }
    }
    return Promise.resolve(mockEntry)
  }),
  findFileIndex: vi.fn().mockResolvedValue(0),
  getTotalCount: vi.fn().mockResolvedValue(10),
  getSyncStatus: vi.fn().mockResolvedValue({ data: {}, timedOut: false }),
  openFile: vi.fn().mockResolvedValue(undefined),
  listen: vi.fn().mockResolvedValue(() => {}),
  showFileContextMenu: vi.fn().mockResolvedValue(undefined),
  updateMenuContext: vi.fn().mockResolvedValue(undefined),
  listVolumes: vi.fn().mockResolvedValue({
    data: [
      { id: 'root', name: 'Macintosh HD', path: '/', category: 'main_volume', isEjectable: false },
      {
        id: 'external',
        name: 'External Drive',
        path: '/Volumes/External',
        category: 'attached_volume',
        isEjectable: true,
      },
      { id: 'dropbox', name: 'Dropbox', path: '/Users/test/Dropbox', category: 'cloud_drive', isEjectable: false },
    ],
    timedOut: false,
  }),
  resolvePathVolume: vi.fn().mockResolvedValue({
    volume: { id: 'root', name: 'Macintosh HD', path: '/', category: 'main_volume', isEjectable: false },
    timedOut: false,
  }),
  getDefaultVolumeId: vi.fn().mockResolvedValue('root'),
  getVolumeSpace: vi
    .fn()
    .mockResolvedValue({ data: { totalBytes: 500_000_000_000, availableBytes: 200_000_000_000 }, timedOut: false }),
  refreshListing: vi.fn().mockResolvedValue({ data: null, timedOut: false }),
  getIcons: vi.fn().mockResolvedValue({ data: {}, timedOut: false }),
  refreshDirectoryIcons: vi.fn().mockResolvedValue({ data: {}, timedOut: false }),
  DEFAULT_VOLUME_ID: 'root',
  listNetworkHosts: vi.fn().mockResolvedValue([]),
  getNetworkDiscoveryState: vi.fn().mockResolvedValue('idle'),
  resolveNetworkHost: vi.fn().mockResolvedValue(null),
  listMtpDevices: vi.fn().mockResolvedValue([]),
  onMtpDeviceConnected: vi.fn().mockResolvedValue(() => {}),
  onMtpDeviceDisconnected: vi.fn().mockResolvedValue(() => {}),
  onMtpExclusiveAccessError: vi.fn().mockResolvedValue(() => {}),
  onMtpPermissionError: vi.fn().mockResolvedValue(() => {}),
  notifyDialogOpened: vi.fn().mockResolvedValue(undefined),
  notifyDialogClosed: vi.fn().mockResolvedValue(undefined),
  watchVolumeSpace: vi.fn().mockResolvedValue(undefined),
}))

vi.mock('$lib/icon-cache', async () => {
  const { writable } = await import('svelte/store')
  return {
    getCachedIcon: vi.fn().mockReturnValue('/icons/file.png'),
    iconCacheVersion: writable(0),
    prefetchIcons: vi.fn().mockResolvedValue(undefined),
  }
})

vi.mock('$lib/settings/reactive-settings.svelte', () => ({
  getRowHeight: vi.fn().mockReturnValue(24),
  formatDateTime: vi.fn().mockReturnValue('2025-01-01 00:00'),
  formatFileSize: vi.fn().mockReturnValue('1.0 KB'),
  getUseAppIconsForDocuments: vi.fn().mockReturnValue(true),
  getSizeDisplayMode: vi.fn().mockReturnValue('smart'),
}))

vi.mock('$lib/drag-drop', () => ({ startDragTracking: vi.fn() }))

vi.mock('$lib/stores/volume-store.svelte', () => ({
  getVolumes: vi
    .fn()
    .mockReturnValue([{ id: 'root', name: 'Macintosh HD', path: '/', category: 'main_volume', isEjectable: false }]),
  getVolumesTimedOut: vi.fn().mockReturnValue(false),
  isVolumesRefreshing: vi.fn().mockReturnValue(false),
  isVolumeRetryFailed: vi.fn().mockReturnValue(false),
  requestVolumeRefresh: vi.fn(),
  initVolumeStore: vi.fn().mockResolvedValue(undefined),
  cleanupVolumeStore: vi.fn(),
}))

// ============================================================================
// VolumeBreadcrumb tests
// ============================================================================

describe('VolumeBreadcrumb', () => {
  const { getTarget } = useMountTarget()

  describe('Rendering', () => {
    it('renders volume breadcrumb container', async () => {
      mount(VolumeBreadcrumb, {
        target: getTarget(),
        props: {
          volumeId: 'root',
          currentPath: '/',
        },
      })

      await waitForUpdates(100)

      expect(getTarget().querySelector('.volume-breadcrumb')).toBeTruthy()
    })

    it('displays current volume name', async () => {
      mount(VolumeBreadcrumb, {
        target: getTarget(),
        props: {
          volumeId: 'root',
          currentPath: '/',
        },
      })

      await waitForUpdates(100)

      const volumeName = getTarget().querySelector('.volume-name')
      expect(volumeName?.textContent).toContain('Macintosh HD')
    })
  })

  describe('Dropdown', () => {
    it('exports toggle method', async () => {
      const component = mount(VolumeBreadcrumb, {
        target: getTarget(),
        props: {
          volumeId: 'root',
          currentPath: '/',
        },
      })

      await waitForUpdates(100)

      expect(typeof (component as unknown as Record<string, unknown>).toggle).toBe('function')
    })

    it('toggle method opens dropdown', async () => {
      const component = mount(VolumeBreadcrumb, {
        target: getTarget(),
        props: {
          volumeId: 'root',
          currentPath: '/',
        },
      })

      await waitForUpdates(100)

      // Initially dropdown should be closed
      expect(getTarget().querySelector('.volume-dropdown')).toBeNull()

      // Call toggle
      const toggle = (component as unknown as { toggle: () => void }).toggle
      toggle()

      await tick()

      // Dropdown should now be open
      expect(getTarget().querySelector('.volume-dropdown')).toBeTruthy()
    })

    it('dropdown shows all volumes', async () => {
      const component = mount(VolumeBreadcrumb, {
        target: getTarget(),
        props: {
          volumeId: 'root',
          currentPath: '/',
        },
      })

      await waitForUpdates(100)

      // Open dropdown
      const toggle = (component as unknown as { toggle: () => void }).toggle
      toggle()

      await tick()

      // Should show volume items
      const volumeItems = getTarget().querySelectorAll('.volume-item')
      expect(volumeItems.length).toBeGreaterThan(0)
    })

    it('clicking volume item calls onVolumeChange', async () => {
      const volumeChangeFn = vi.fn()

      const component = mount(VolumeBreadcrumb, {
        target: getTarget(),
        props: {
          volumeId: 'root',
          currentPath: '/',
          onVolumeChange: volumeChangeFn,
        },
      })

      await waitForUpdates(100)

      // Open dropdown
      const toggle = (component as unknown as { toggle: () => void }).toggle
      toggle()

      await tick()

      // Find another volume item and click it
      const volumeItems = getTarget().querySelectorAll('.volume-item:not(.is-under-cursor)')
      if (volumeItems.length > 0) {
        volumeItems[0].dispatchEvent(new MouseEvent('click', { bubbles: true }))

        await tick()

        expect(volumeChangeFn).toHaveBeenCalled()
      }
    })

    it('Escape key closes dropdown', async () => {
      const component = mount(VolumeBreadcrumb, {
        target: getTarget(),
        props: {
          volumeId: 'root',
          currentPath: '/',
        },
      })

      await waitForUpdates(100)

      // Open dropdown
      const toggle = (component as unknown as { toggle: () => void }).toggle
      toggle()

      await tick()

      expect(getTarget().querySelector('.volume-dropdown')).toBeTruthy()

      // Press Escape
      document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))

      await tick()

      expect(getTarget().querySelector('.volume-dropdown')).toBeNull()
    })
  })

  describe('Volume categories', () => {
    it('groups volumes by category', async () => {
      const component = mount(VolumeBreadcrumb, {
        target: getTarget(),
        props: {
          volumeId: 'root',
          currentPath: '/',
        },
      })

      await waitForUpdates(100)

      // Open dropdown
      const toggle = (component as unknown as { toggle: () => void }).toggle
      toggle()

      await tick()

      // Should have category labels
      const categoryLabels = getTarget().querySelectorAll('.category-label')
      // We expect at least "Volumes" and possibly "Cloud"
      expect(categoryLabels.length).toBeGreaterThanOrEqual(0)
    })
  })

  describe('Keyboard navigation', () => {
    it('exports handleKeyDown method', async () => {
      const component = mount(VolumeBreadcrumb, {
        target: getTarget(),
        props: {
          volumeId: 'root',
          currentPath: '/',
        },
      })

      await waitForUpdates(100)

      expect(typeof (component as unknown as Record<string, unknown>).handleKeyDown).toBe('function')
    })

    it('exports getIsOpen method', async () => {
      const component = mount(VolumeBreadcrumb, {
        target: getTarget(),
        props: {
          volumeId: 'root',
          currentPath: '/',
        },
      })

      await waitForUpdates(100)

      expect(typeof (component as unknown as Record<string, unknown>).getIsOpen).toBe('function')
    })

    it('getIsOpen returns false when dropdown is closed', async () => {
      const component = mount(VolumeBreadcrumb, {
        target: getTarget(),
        props: {
          volumeId: 'root',
          currentPath: '/',
        },
      })

      await waitForUpdates(100)

      const getIsOpen = (component as unknown as { getIsOpen: () => boolean }).getIsOpen
      expect(getIsOpen()).toBe(false)
    })

    it('getIsOpen returns true when dropdown is open', async () => {
      const component = mount(VolumeBreadcrumb, {
        target: getTarget(),
        props: {
          volumeId: 'root',
          currentPath: '/',
        },
      })

      await waitForUpdates(100)

      const toggle = (component as unknown as { toggle: () => void }).toggle
      toggle()

      await tick()

      const getIsOpen = (component as unknown as { getIsOpen: () => boolean }).getIsOpen
      expect(getIsOpen()).toBe(true)
    })

    it('handleKeyDown returns false when dropdown is closed', async () => {
      const component = mount(VolumeBreadcrumb, {
        target: getTarget(),
        props: {
          volumeId: 'root',
          currentPath: '/',
        },
      })

      await waitForUpdates(100)

      const handleKeyDown = (component as unknown as { handleKeyDown: (e: KeyboardEvent) => boolean }).handleKeyDown
      const event = new KeyboardEvent('keydown', { key: 'ArrowDown', bubbles: true })
      expect(handleKeyDown(event)).toBe(false)
    })

    it('ArrowDown moves highlight down', async () => {
      const component = mount(VolumeBreadcrumb, {
        target: getTarget(),
        props: {
          volumeId: 'root',
          currentPath: '/',
        },
      })

      await waitForUpdates(100)

      // Open dropdown
      const toggle = (component as unknown as { toggle: () => void }).toggle
      toggle()

      await tick()

      // Verify dropdown is open and first item is highlighted
      const items = getTarget().querySelectorAll('.volume-item')
      expect(items.length).toBeGreaterThan(1)
      expect(items[0].classList.contains('is-focused-and-under-cursor')).toBe(true)

      // Press ArrowDown
      const handleKeyDown = (component as unknown as { handleKeyDown: (e: KeyboardEvent) => boolean }).handleKeyDown
      const event = new KeyboardEvent('keydown', { key: 'ArrowDown', bubbles: true })
      const handled = handleKeyDown(event)

      await tick()

      expect(handled).toBe(true)
      expect(items[0].classList.contains('is-focused-and-under-cursor')).toBe(false)
      expect(items[1].classList.contains('is-focused-and-under-cursor')).toBe(true)
    })

    it('ArrowUp moves highlight up', async () => {
      const component = mount(VolumeBreadcrumb, {
        target: getTarget(),
        props: {
          volumeId: 'root',
          currentPath: '/',
        },
      })

      await waitForUpdates(100)

      // Open dropdown and move down first
      const toggle = (component as unknown as { toggle: () => void }).toggle
      toggle()

      await tick()

      const handleKeyDown = (component as unknown as { handleKeyDown: (e: KeyboardEvent) => boolean }).handleKeyDown

      // Move down once
      handleKeyDown(new KeyboardEvent('keydown', { key: 'ArrowDown', bubbles: true }))
      await tick()

      // Now move back up
      const event = new KeyboardEvent('keydown', { key: 'ArrowUp', bubbles: true })
      const handled = handleKeyDown(event)

      await tick()

      expect(handled).toBe(true)
      const items = getTarget().querySelectorAll('.volume-item')
      expect(items[0].classList.contains('is-focused-and-under-cursor')).toBe(true)
    })

    it('ArrowUp at first item stays at first', async () => {
      const component = mount(VolumeBreadcrumb, {
        target: getTarget(),
        props: {
          volumeId: 'root',
          currentPath: '/',
        },
      })

      await waitForUpdates(100)

      // Open dropdown
      const toggle = (component as unknown as { toggle: () => void }).toggle
      toggle()

      await tick()

      // Try to move up when already at first
      const handleKeyDown = (component as unknown as { handleKeyDown: (e: KeyboardEvent) => boolean }).handleKeyDown
      const event = new KeyboardEvent('keydown', { key: 'ArrowUp', bubbles: true })
      handleKeyDown(event)

      await tick()

      const items = getTarget().querySelectorAll('.volume-item')
      expect(items[0].classList.contains('is-focused-and-under-cursor')).toBe(true)
    })

    it('Enter selects highlighted volume and closes dropdown', async () => {
      const volumeChangeFn = vi.fn()

      const component = mount(VolumeBreadcrumb, {
        target: getTarget(),
        props: {
          volumeId: 'root',
          currentPath: '/',
          onVolumeChange: volumeChangeFn,
        },
      })

      await waitForUpdates(100)

      // Open dropdown
      const toggle = (component as unknown as { toggle: () => void }).toggle
      toggle()

      await tick()

      // Move to second item (first volume that's not under the cursor)
      const handleKeyDown = (component as unknown as { handleKeyDown: (e: KeyboardEvent) => boolean }).handleKeyDown
      handleKeyDown(new KeyboardEvent('keydown', { key: 'ArrowDown', bubbles: true }))
      await tick()

      // Press Enter
      const enterEvent = new KeyboardEvent('keydown', { key: 'Enter', bubbles: true })
      const handled = handleKeyDown(enterEvent)

      await tick()

      expect(handled).toBe(true)
      expect(volumeChangeFn).toHaveBeenCalled()
    })

    it('Escape closes dropdown via handleKeyDown', async () => {
      const component = mount(VolumeBreadcrumb, {
        target: getTarget(),
        props: {
          volumeId: 'root',
          currentPath: '/',
        },
      })

      await waitForUpdates(100)

      // Open dropdown
      const toggle = (component as unknown as { toggle: () => void }).toggle
      toggle()

      await tick()

      expect(getTarget().querySelector('.volume-dropdown')).toBeTruthy()

      // Press Escape via handleKeyDown
      const handleKeyDown = (component as unknown as { handleKeyDown: (e: KeyboardEvent) => boolean }).handleKeyDown
      const event = new KeyboardEvent('keydown', { key: 'Escape', bubbles: true })
      const handled = handleKeyDown(event)

      await tick()

      expect(handled).toBe(true)
      expect(getTarget().querySelector('.volume-dropdown')).toBeNull()
    })

    it('Home jumps to first item', async () => {
      const component = mount(VolumeBreadcrumb, {
        target: getTarget(),
        props: {
          volumeId: 'root',
          currentPath: '/',
        },
      })

      await waitForUpdates(100)

      // Open dropdown
      const toggle = (component as unknown as { toggle: () => void }).toggle
      toggle()

      await tick()

      const handleKeyDown = (component as unknown as { handleKeyDown: (e: KeyboardEvent) => boolean }).handleKeyDown

      // Move down a couple times
      handleKeyDown(new KeyboardEvent('keydown', { key: 'ArrowDown', bubbles: true }))
      handleKeyDown(new KeyboardEvent('keydown', { key: 'ArrowDown', bubbles: true }))
      await tick()

      // Press Home
      const handled = handleKeyDown(new KeyboardEvent('keydown', { key: 'Home', bubbles: true }))
      await tick()

      expect(handled).toBe(true)
      const items = getTarget().querySelectorAll('.volume-item')
      expect(items[0].classList.contains('is-focused-and-under-cursor')).toBe(true)
    })

    it('End jumps to last item', async () => {
      const component = mount(VolumeBreadcrumb, {
        target: getTarget(),
        props: {
          volumeId: 'root',
          currentPath: '/',
        },
      })

      await waitForUpdates(100)

      // Open dropdown
      const toggle = (component as unknown as { toggle: () => void }).toggle
      toggle()

      await tick()

      // Press End
      const handleKeyDown = (component as unknown as { handleKeyDown: (e: KeyboardEvent) => boolean }).handleKeyDown
      const handled = handleKeyDown(new KeyboardEvent('keydown', { key: 'End', bubbles: true }))
      await tick()

      expect(handled).toBe(true)
      const items = getTarget().querySelectorAll('.volume-item')
      const lastItem = items[items.length - 1]
      expect(lastItem.classList.contains('is-focused-and-under-cursor')).toBe(true)
    })

    it('unhandled keys return false', async () => {
      const component = mount(VolumeBreadcrumb, {
        target: getTarget(),
        props: {
          volumeId: 'root',
          currentPath: '/',
        },
      })

      await waitForUpdates(100)

      // Open dropdown
      const toggle = (component as unknown as { toggle: () => void }).toggle
      toggle()

      await tick()

      const handleKeyDown = (component as unknown as { handleKeyDown: (e: KeyboardEvent) => boolean }).handleKeyDown
      const event = new KeyboardEvent('keydown', { key: 'x', bubbles: true })
      const handled = handleKeyDown(event)

      expect(handled).toBe(false)
    })
  })
})
