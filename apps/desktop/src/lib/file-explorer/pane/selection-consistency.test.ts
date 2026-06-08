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
import { findFileIndex } from '$lib/tauri-commands'

// ============================================================================
// Mock setup (must be in each test file; Vitest hoists vi.mock calls)
// ============================================================================

let mockEntry: unknown = null

vi.mock('$lib/tauri-commands', () => ({
  listDirectoryStart: vi.fn().mockResolvedValue({ listingId: 'mock-listing', status: { status: 'ready' } }),
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
  onVolumeSpaceChanged: vi.fn().mockResolvedValue(() => {}),
  onWriteSourceItemDone: vi.fn().mockResolvedValue(() => {}),
  onDirectoryDiff: vi.fn().mockResolvedValue(() => {}),
  onDirectoryDeleted: vi.fn().mockResolvedValue(() => {}),
  onMtpExclusiveAccessError: vi.fn().mockResolvedValue(() => {}),
  onMtpPermissionError: vi.fn().mockResolvedValue(() => {}),
  notifyDialogOpened: vi.fn().mockResolvedValue(undefined),
  notifyDialogClosed: vi.fn().mockResolvedValue(undefined),
  watchVolumeSpace: vi.fn().mockResolvedValue(undefined),
  getDirStatsBatch: vi.fn().mockResolvedValue({}),
}))

vi.mock('$lib/icon-cache', async () => {
  const { writable } = await import('svelte/store')
  return {
    getCachedIcon: vi.fn().mockReturnValue('/icons/file.png'),
    iconCacheVersion: writable(0),
    iconCacheCleared: writable(0),
    prefetchIcons: vi.fn().mockResolvedValue(undefined),
    prefetchCustomFolderIcons: vi.fn().mockResolvedValue(undefined),
    evictPerPathIconsForDir: vi.fn(),
  }
})

vi.mock('$lib/settings/reactive-settings.svelte', () => ({
  getRowHeight: vi.fn().mockReturnValue(24),
  formatDateTime: vi.fn().mockReturnValue('2025-01-01 00:00'),
  formattedDate: vi.fn().mockReturnValue({
    text: '2025-01-01 00:00',
    parts: {
      left: [
        { text: '2025', ageClass: 'age-fresh' as const },
        { text: '-', ageClass: null },
        { text: '01', ageClass: null },
        { text: '-', ageClass: null },
        { text: '01', ageClass: null },
      ],
      right: [
        { text: '00', ageClass: null },
        { text: ':', ageClass: null },
        { text: '00', ageClass: null },
      ],
    },
  }),
  formatFileSize: vi.fn().mockReturnValue('1.0 KB'),
  getFileSizeFormat: vi.fn().mockReturnValue('binary'),
  getFileSizeUnit: vi.fn().mockReturnValue('bytes'),
  getUseAppIconsForDocuments: vi.fn().mockReturnValue(true),
  getSizeDisplayMode: vi.fn().mockReturnValue('smart'),
  getNetworkEnabled: vi.fn().mockReturnValue(true),
  getSizeMismatchWarning: vi.fn().mockReturnValue(false),
  getStripedRows: vi.fn().mockReturnValue(false),
  getBriefColumnWidthMode: vi.fn().mockReturnValue('auto'),
  getBriefColumnWidthMaxPx: vi.fn().mockReturnValue(400),
  getIsCmdrGold: vi.fn().mockReturnValue(false),
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

  it('Insert key toggles selection and moves cursor down', async () => {
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

    type Api = {
      handleKeyDown: (e: KeyboardEvent) => void
      adoptListing: (s: {
        currentPath: string
        listingId: string
        totalCount: number
        cursorIndex: number
        selectedIndices: number[]
        lastSequence: number
      }) => void
      getSelectedIndices: () => number[]
      getCursorIndex: () => number
    }
    const c = component as unknown as Api

    // Test mocks don't drive the `listing-complete` event, so totalCount stays
    // at 0 unless we adopt a listing. Seed: 10 rows, no parent, cursor at 0.
    c.adoptListing({
      currentPath: '/',
      listingId: 'mock-listing',
      totalCount: 10,
      cursorIndex: 0,
      selectedIndices: [],
      lastSequence: 0,
    })
    // Let the listingId-driven $effect (refetch totalCount + clamp cursor) settle
    // before we exercise the handler — otherwise its async chain races with the keypress.
    await waitForUpdates(50)

    expect(c.getSelectedIndices()).toEqual([])
    expect(c.getCursorIndex()).toBe(0)

    const insertEvent = new KeyboardEvent('keydown', { key: 'Insert', bubbles: true })
    c.handleKeyDown(insertEvent)
    await tick()

    // Cursor row 0 is selected, cursor advanced to row 1
    expect(c.getSelectedIndices()).toEqual([0])
    expect(c.getCursorIndex()).toBe(1)

    // Press again at row 1: select row 1, advance to row 2
    c.handleKeyDown(insertEvent)
    await tick()
    expect(c.getSelectedIndices().sort()).toEqual([0, 1])
    expect(c.getCursorIndex()).toBe(2)
  })

  it('Insert on ".." advances cursor without selecting the parent entry', async () => {
    // initialPath !== volumePath produces a ".." entry at index 0
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

    type Api = {
      handleKeyDown: (e: KeyboardEvent) => void
      adoptListing: (s: {
        currentPath: string
        listingId: string
        totalCount: number
        cursorIndex: number
        selectedIndices: number[]
        lastSequence: number
      }) => void
      getSelectedIndices: () => number[]
      getCursorIndex: () => number
    }
    const c = component as unknown as Api

    // 10 backend rows + ".." = 11 frontend rows; cursor on ".." (index 0).
    // Same race as the "last row" test: the listingId-driven $effect calls
    // findFileIndex (mocked to 0) and would yank the cursor off ".." onto
    // the first real row. Return null so the effect leaves cursor alone.
    vi.mocked(findFileIndex).mockResolvedValueOnce(null)
    c.adoptListing({
      currentPath: '/test',
      listingId: 'mock-listing',
      totalCount: 10,
      cursorIndex: 0,
      selectedIndices: [],
      lastSequence: 0,
    })
    await waitForUpdates(50)

    expect(c.getCursorIndex()).toBe(0) // sitting on ".."
    expect(c.getSelectedIndices()).toEqual([])

    const insertEvent = new KeyboardEvent('keydown', { key: 'Insert', bubbles: true })
    c.handleKeyDown(insertEvent)
    await tick()

    // ".." stayed unselected; cursor moved down anyway
    expect(c.getSelectedIndices()).toEqual([])
    expect(c.getCursorIndex()).toBe(1)
  })

  it('Insert at last row toggles but does not move cursor past end', async () => {
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

    type Api = {
      handleKeyDown: (e: KeyboardEvent) => void
      adoptListing: (s: {
        currentPath: string
        listingId: string
        totalCount: number
        cursorIndex: number
        selectedIndices: number[]
        lastSequence: number
      }) => void
      getSelectedIndices: () => number[]
      getCursorIndex: () => number
    }
    const c = component as unknown as Api

    // 10 rows, no parent, cursor on the last row (index 9).
    // The listingId-driven $effect would otherwise call findFileIndex (mocked to 0)
    // and yank the cursor back to 0 after we adopt cursor: 9.
    vi.mocked(findFileIndex).mockResolvedValueOnce(8)
    c.adoptListing({
      currentPath: '/',
      listingId: 'mock-listing',
      totalCount: 10,
      cursorIndex: 9,
      selectedIndices: [],
      lastSequence: 0,
    })
    await waitForUpdates(50)
    // After the effect settles, cursor lands wherever findFileIndex pointed.
    // We just need it pinned to the last row before pressing Insert.
    const setCursorIndex = (component as unknown as { setCursorIndex: (i: number) => Promise<void> }).setCursorIndex
    await setCursorIndex(9)
    await tick()

    expect(c.getCursorIndex()).toBe(9)

    const insertEvent = new KeyboardEvent('keydown', { key: 'Insert', bubbles: true })
    c.handleKeyDown(insertEvent)
    await tick()

    // Last row toggled; cursor stayed put
    expect(c.getSelectedIndices()).toEqual([9])
    expect(c.getCursorIndex()).toBe(9)
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
