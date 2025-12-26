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
    }),
    saveAppStatus: vi.fn().mockResolvedValue(undefined),
}))

// Mock pathExists
vi.mock('$lib/tauri-commands', () => ({
    pathExists: vi.fn().mockResolvedValue(true),
    listDirectoryContents: vi.fn().mockResolvedValue([]),
    openFile: vi.fn().mockResolvedValue(undefined),
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

        // Wait for async initialization
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
