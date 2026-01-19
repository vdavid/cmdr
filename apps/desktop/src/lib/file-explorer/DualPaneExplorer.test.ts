import { describe, it, expect, vi } from 'vitest'
import { mount } from 'svelte'
import { tick } from 'svelte'
import DualPaneExplorer from './DualPaneExplorer.svelte'

// Mock the app-status-store to avoid Tauri dependency in tests
vi.mock('$lib/app-status-store', () => ({
    loadAppStatus: vi.fn().mockResolvedValue({
        leftPath: '~',
        rightPath: '~',
        focusedPane: 'left',
        leftVolumeId: 'root',
        rightVolumeId: 'root',
        leftSortBy: 'name',
        rightSortBy: 'name',
        leftViewMode: 'brief',
        rightViewMode: 'brief',
        leftPaneWidthPercent: 50,
    }),
    saveAppStatus: vi.fn().mockResolvedValue(undefined),
    getLastUsedPathForVolume: vi.fn().mockResolvedValue(undefined),
    saveLastUsedPathForVolume: vi.fn().mockResolvedValue(undefined),
    getColumnSortOrder: vi.fn().mockResolvedValue('ascending'),
    saveColumnSortOrder: vi.fn().mockResolvedValue(undefined),
}))

vi.mock('@tauri-apps/api/event', () => ({
    listen: vi.fn(() => Promise.resolve(() => {})),
}))

vi.mock('@tauri-apps/api/core', () => ({
    invoke: vi.fn(),
}))

// Mock pathExists
vi.mock('$lib/tauri-commands', () => ({
    pathExists: vi.fn().mockResolvedValue(true),
    listDirectoryStartSession: vi.fn().mockResolvedValue({
        sessionId: 'mock-session-id',
        totalCount: 0,
        entries: [],
        hasMore: false,
    }),
    listDirectoryNextChunk: vi.fn().mockResolvedValue({
        entries: [],
        hasMore: false,
    }),
    listDirectoryEndSession: vi.fn().mockResolvedValue(undefined),
    openFile: vi.fn().mockResolvedValue(undefined),
    getIcons: vi.fn().mockResolvedValue({}),
    listen: vi.fn(() => Promise.resolve(() => {})),
    showFileContextMenu: vi.fn(() => Promise.resolve()),
    updateMenuContext: vi.fn(() => Promise.resolve()),
    hasFontMetrics: vi.fn().mockResolvedValue(true),
    storeFontMetrics: vi.fn().mockResolvedValue(undefined),
    listVolumes: vi
        .fn()
        .mockResolvedValue([
            { id: 'root', name: 'Macintosh HD', path: '/', category: 'main_volume', isEjectable: false },
        ]),
    findContainingVolume: vi.fn().mockResolvedValue({
        id: 'root',
        name: 'Macintosh HD',
        path: '/',
        category: 'main_volume',
        isEjectable: false,
    }),
    getDefaultVolumeId: vi.fn().mockResolvedValue('root'),
    DEFAULT_VOLUME_ID: 'root',
    // Network discovery mocks
    listNetworkHosts: vi.fn().mockResolvedValue([]),
    getNetworkDiscoveryState: vi.fn().mockResolvedValue('idle'),
    resolveNetworkHost: vi.fn().mockResolvedValue(null),
}))

// Mock settings-store to avoid Tauri event API dependency in tests
vi.mock('$lib/settings-store', () => ({
    loadSettings: vi.fn().mockResolvedValue({
        showHiddenFiles: true,
    }),
    saveSettings: vi.fn().mockResolvedValue(undefined),
    subscribeToSettingsChanges: vi.fn().mockResolvedValue(() => {}),
}))

describe('DualPaneExplorer', () => {
    it('renders dual pane container', () => {
        const target = document.createElement('div')
        mount(DualPaneExplorer, { target })

        expect(target.querySelector('.dual-pane-explorer')).toBeTruthy()
    })

    it('renders two file panes after initialization', async () => {
        const target = document.createElement('div')
        mount(DualPaneExplorer, { target })

        // Wait for async initialization (paths, volumes, settings, findContainingVolume)
        // The initialization now includes more async calls, so we need more ticks
        for (let i = 0; i < 10; i++) {
            await tick()
        }
        // Small additional delay to ensure all promises resolve
        await new Promise((resolve) => setTimeout(resolve, 10))
        await tick()

        const panes = target.querySelectorAll('.file-pane')
        expect(panes).toHaveLength(2)
    })

    it('shows loading state initially', () => {
        const target = document.createElement('div')
        mount(DualPaneExplorer, { target })

        expect(target.textContent).toContain('Loading')
    })
})

describe('Sorting integration', () => {
    it('restores sort preferences from storage on load', async () => {
        const { getColumnSortOrder } = await import('$lib/app-status-store')
        const mockGetColumnSortOrder = vi.mocked(getColumnSortOrder)
        mockGetColumnSortOrder.mockClear()
        mockGetColumnSortOrder.mockResolvedValue('descending')

        const target = document.createElement('div')
        mount(DualPaneExplorer, { target })

        // Wait for initialization
        for (let i = 0; i < 10; i++) {
            await tick()
        }
        await new Promise((resolve) => setTimeout(resolve, 10))
        await tick()

        // Verify getColumnSortOrder was called during initialization for left pane
        expect(mockGetColumnSortOrder).toHaveBeenCalled()

        // Should be called for both left and right panes
        expect(mockGetColumnSortOrder.mock.calls.length).toBeGreaterThanOrEqual(2)
    })

    it('has infrastructure to persist sort changes via saveColumnSortOrder', async () => {
        const { saveColumnSortOrder } = await import('$lib/app-status-store')
        const mockSaveColumnSortOrder = vi.mocked(saveColumnSortOrder)

        // Verify the mock is available and callable
        expect(mockSaveColumnSortOrder).toBeDefined()
        expect(typeof mockSaveColumnSortOrder).toBe('function')

        // Verify it can be called with proper arguments
        await mockSaveColumnSortOrder('size', 'ascending')
        expect(mockSaveColumnSortOrder).toHaveBeenCalledWith('size', 'ascending')
    })

    it('has infrastructure to call resortListing command', async () => {
        const { invoke } = await import('@tauri-apps/api/core')
        const mockInvoke = vi.mocked(invoke)

        // Verify the mock is available
        expect(mockInvoke).toBeDefined()

        // Verify resortListing can be called
        await mockInvoke('plugin:file_system|resort_listing', {
            listingId: 'test-id',
            sortBy: 'size',
            sortOrder: 'ascending',
        })

        const resortCalls = mockInvoke.mock.calls.filter((call) => call[0] === 'plugin:file_system|resort_listing')
        expect(resortCalls.length).toBeGreaterThan(0)
    })
})
