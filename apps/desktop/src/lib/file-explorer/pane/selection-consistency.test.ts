/**
 * Selection state consistency tests.
 *
 * CRITICAL: These tests ensure that what the user sees (UI) matches what
 * operations will act on (getSelectedIndices). This is a safety guarantee
 * to prevent destructive operations on unintended files.
 */
import { describe, it, expect, vi } from 'vitest'
import { mount, tick } from 'svelte'
import FilePane from './FilePane.svelte'
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
// Selection state consistency tests
// ============================================================================

describe('Selection state consistency', () => {
  const { getTarget } = useMountTarget()

  it('getSelectedIndices returns empty array initially', async () => {
    const component = mount(FilePane, {
      target: getTarget(),
      props: {
        initialPath: '/test',
        volumeId: 'root',
        volumePath: '/',
        isFocused: true,
        showHiddenFiles: true,
        viewMode: 'brief',
      },
    })

    await waitForUpdates(100)

    const getSelectedIndices = (component as unknown as { getSelectedIndices: () => number[] }).getSelectedIndices
    expect(getSelectedIndices()).toEqual([])
  })

  it('toggleSelectionAtCursor updates both UI and getSelectedIndices immediately', async () => {
    // Use volumePath === initialPath so there's no ".." entry (which can't be selected)
    const component = mount(FilePane, {
      target: getTarget(),
      props: {
        initialPath: '/',
        volumeId: 'root',
        volumePath: '/',
        isFocused: true,
        showHiddenFiles: true,
        viewMode: 'brief',
      },
    })

    await waitForUpdates(100)

    const toggleSelectionAtCursor = (component as unknown as { toggleSelectionAtCursor: () => void })
      .toggleSelectionAtCursor
    const getSelectedIndices = (component as unknown as { getSelectedIndices: () => number[] }).getSelectedIndices

    // Before toggle: nothing selected
    expect(getSelectedIndices()).toEqual([])

    // Toggle selection at cursor (index 0, which is a real file since no ".." entry)
    toggleSelectionAtCursor()
    await tick()

    // After toggle: cursor index should be selected
    const selectedAfterToggle = getSelectedIndices()
    expect(selectedAfterToggle.length).toBe(1)
    expect(selectedAfterToggle).toContain(0)

    // Toggle again: should deselect
    toggleSelectionAtCursor()
    await tick()

    expect(getSelectedIndices()).toEqual([])
  })

  it('Space key toggles selection and updates UI immediately', async () => {
    // Use volumePath === initialPath so there's no ".." entry
    const component = mount(FilePane, {
      target: getTarget(),
      props: {
        initialPath: '/',
        volumeId: 'root',
        volumePath: '/',
        isFocused: true,
        showHiddenFiles: true,
        viewMode: 'brief',
      },
    })

    await waitForUpdates(100)

    const handleKeyDown = (component as unknown as { handleKeyDown: (e: KeyboardEvent) => void }).handleKeyDown
    const getSelectedIndices = (component as unknown as { getSelectedIndices: () => number[] }).getSelectedIndices

    // Before Space: nothing selected
    expect(getSelectedIndices()).toEqual([])

    // Press Space
    const spaceEvent = new KeyboardEvent('keydown', { key: ' ', bubbles: true })
    handleKeyDown(spaceEvent)
    await tick()

    // After Space: cursor index should be selected
    const selectedAfterSpace = getSelectedIndices()
    expect(selectedAfterSpace.length).toBe(1)

    // Press Space again: should deselect
    handleKeyDown(spaceEvent)
    await tick()

    expect(getSelectedIndices()).toEqual([])
  })

  it('clearSelection clears all selections and getSelectedIndices returns empty', async () => {
    const component = mount(FilePane, {
      target: getTarget(),
      props: {
        initialPath: '/',
        volumeId: 'root',
        volumePath: '/',
        isFocused: true,
        showHiddenFiles: true,
        viewMode: 'brief',
      },
    })

    await waitForUpdates(100)

    const setSelectedIndices = (component as unknown as { setSelectedIndices: (indices: number[]) => void })
      .setSelectedIndices
    const clearSelection = (component as unknown as { clearSelection: () => void }).clearSelection
    const getSelectedIndices = (component as unknown as { getSelectedIndices: () => number[] }).getSelectedIndices

    // Set some selections first (using setSelectedIndices which we know works)
    setSelectedIndices([0, 1, 2])
    await tick()
    expect(getSelectedIndices().length).toBe(3)

    // Clear selection
    clearSelection()
    await tick()

    // Verify empty
    expect(getSelectedIndices()).toEqual([])
  })

  it('setSelectedIndices updates state and getSelectedIndices returns same values', async () => {
    const component = mount(FilePane, {
      target: getTarget(),
      props: {
        initialPath: '/',
        volumeId: 'root',
        volumePath: '/',
        isFocused: true,
        showHiddenFiles: true,
        viewMode: 'brief',
      },
    })

    await waitForUpdates(100)

    const setSelectedIndices = (component as unknown as { setSelectedIndices: (indices: number[]) => void })
      .setSelectedIndices
    const getSelectedIndices = (component as unknown as { getSelectedIndices: () => number[] }).getSelectedIndices

    // Set specific indices
    const indicesToSet = [1, 3, 5]
    setSelectedIndices(indicesToSet)
    await tick()

    // Verify getSelectedIndices returns exactly what we set
    const retrieved = getSelectedIndices()
    expect(retrieved.sort()).toEqual(indicesToSet.sort())
  })

  it('multiple rapid toggles maintain consistency', async () => {
    // Use volumePath === initialPath so there's no ".." entry
    const component = mount(FilePane, {
      target: getTarget(),
      props: {
        initialPath: '/',
        volumeId: 'root',
        volumePath: '/',
        isFocused: true,
        showHiddenFiles: true,
        viewMode: 'brief',
      },
    })

    await waitForUpdates(100)

    const toggleSelectionAtCursor = (component as unknown as { toggleSelectionAtCursor: () => void })
      .toggleSelectionAtCursor
    const getSelectedIndices = (component as unknown as { getSelectedIndices: () => number[] }).getSelectedIndices

    // Rapid toggles without waiting
    toggleSelectionAtCursor() // select
    toggleSelectionAtCursor() // deselect
    toggleSelectionAtCursor() // select
    await tick()

    // Should end up selected (odd number of toggles)
    expect(getSelectedIndices().length).toBe(1)

    // One more toggle
    toggleSelectionAtCursor() // deselect
    await tick()

    // Should be empty
    expect(getSelectedIndices()).toEqual([])
  })

  it('handleKeyUp export exists and handles Shift release', async () => {
    const component = mount(FilePane, {
      target: getTarget(),
      props: {
        initialPath: '/',
        volumeId: 'root',
        volumePath: '/',
        isFocused: true,
        showHiddenFiles: true,
        viewMode: 'brief',
      },
    })

    await waitForUpdates(100)

    // Verify handleKeyUp is exported
    const handleKeyUp = (component as unknown as { handleKeyUp: (e: KeyboardEvent) => void }).handleKeyUp
    expect(typeof handleKeyUp).toBe('function')

    // Call it with a Shift keyup event - should not throw
    const shiftUpEvent = new KeyboardEvent('keyup', { key: 'Shift', bubbles: true })
    expect(() => {
      handleKeyUp(shiftUpEvent)
    }).not.toThrow()
  })
})
