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
    }),
    saveAppStatus: vi.fn().mockResolvedValue(undefined),
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
    getDefaultVolumeId: vi.fn().mockResolvedValue('root'),
    DEFAULT_VOLUME_ID: 'root',
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

        // Wait for async initialization (paths, volumes, settings)
        await tick()
        await tick()
        await tick()
        await tick()
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
