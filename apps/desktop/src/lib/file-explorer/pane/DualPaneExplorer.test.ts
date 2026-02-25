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
    saveAppStatus: vi.fn(),
    getLastUsedPathForVolume: vi.fn().mockResolvedValue(undefined),
    saveLastUsedPathForVolume: vi.fn().mockResolvedValue(undefined),
    loadPaneTabs: vi.fn().mockResolvedValue({
        tabs: [
            {
                id: 'mock-tab-id',
                path: '~',
                volumeId: 'root',
                sortBy: 'name',
                sortOrder: 'ascending',
                viewMode: 'brief',
                pinned: false,
            },
        ],
        activeTabId: 'mock-tab-id',
    }),
    savePaneTabs: vi.fn().mockResolvedValue(undefined),
}))

// @tauri-apps/api/event is mocked globally in test-setup.ts

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
    getE2eStartPath: vi.fn().mockResolvedValue(null),
    formatBytes: vi.fn().mockReturnValue('0 B'),
    getFileAt: vi.fn().mockResolvedValue(null),
    updateFocusedPane: vi.fn().mockResolvedValue(undefined),
    findFileIndex: vi.fn().mockResolvedValue(null),
    resortListing: vi.fn().mockResolvedValue({}),
    // Network discovery mocks
    listNetworkHosts: vi.fn().mockResolvedValue([]),
    getNetworkDiscoveryState: vi.fn().mockResolvedValue('idle'),
    resolveNetworkHost: vi.fn().mockResolvedValue(null),
    // MTP device mocks
    listMtpDevices: vi.fn().mockResolvedValue([]),
    onMtpDeviceConnected: vi.fn().mockResolvedValue(() => {}),
    onMtpDeviceDisconnected: vi.fn().mockResolvedValue(() => {}),
    onMtpExclusiveAccessError: vi.fn().mockResolvedValue(() => {}),
    onMtpDeviceDetected: vi.fn().mockResolvedValue(() => {}),
    onMtpDeviceRemoved: vi.fn().mockResolvedValue(() => {}),
    // Tab mocks
    updatePaneTabs: vi.fn().mockResolvedValue(undefined),
    showTabContextMenu: vi.fn().mockResolvedValue(null),
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
    it('initializes sort state from persisted app status', async () => {
        const { loadAppStatus } = await import('$lib/app-status-store')
        const mockLoadAppStatus = vi.mocked(loadAppStatus)
        mockLoadAppStatus.mockResolvedValue({
            leftPath: '~',
            rightPath: '~',
            focusedPane: 'left',
            leftVolumeId: 'root',
            rightVolumeId: 'root',
            leftSortBy: 'size',
            rightSortBy: 'modified',
            leftViewMode: 'brief',
            rightViewMode: 'brief',
            leftPaneWidthPercent: 50,
        })

        const target = document.createElement('div')
        mount(DualPaneExplorer, { target })

        // Wait for initialization
        for (let i = 0; i < 10; i++) {
            await tick()
        }
        await new Promise((resolve) => setTimeout(resolve, 10))
        await tick()

        // loadAppStatus should have been called during initialization
        expect(mockLoadAppStatus).toHaveBeenCalled()
    })
})
