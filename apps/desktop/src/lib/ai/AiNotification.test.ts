import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, flushSync } from 'svelte'

// Track the state object so tests can mutate it
let mockState = {
    notificationState: 'hidden' as string,
    downloadProgress: null as { bytesDownloaded: number; totalBytes: number; speed: number; etaSeconds: number } | null,
    progressText: '',
    modelInfo: {
        id: 'ministral-3b-instruct-q4km',
        displayName: 'Ministral 3B',
        sizeBytes: 2147023008,
        sizeFormatted: '2.1 GB',
    },
}

vi.mock('./ai-state.svelte', () => ({
    getAiState: () => mockState,
    handleDownload: vi.fn(() => Promise.resolve()),
    handleCancel: vi.fn(() => Promise.resolve()),
    handleDismiss: vi.fn(() => Promise.resolve()),
    handleOptOut: vi.fn(() => Promise.resolve()),
    handleGotIt: vi.fn(),
}))

vi.mock('$lib/ui/toast', () => ({
    addToast: vi.fn(),
    dismissToast: vi.fn(),
}))

import AiToastContent from './AiToastContent.svelte'
import { handleDownload, handleCancel, handleDismiss, handleOptOut, handleGotIt } from './ai-state.svelte'

describe('AiToastContent', () => {
    beforeEach(() => {
        vi.clearAllMocks()
        mockState = {
            notificationState: 'hidden',
            downloadProgress: null,
            progressText: '',
            modelInfo: {
                id: 'ministral-3b-instruct-q4km',
                displayName: 'Ministral 3B',
                sizeBytes: 2147023008,
                sizeFormatted: '2.1 GB',
            },
        }
    })

    it('renders offer notification with download size and settings hint', () => {
        mockState.notificationState = 'offer'
        const target = document.createElement('div')
        mount(AiToastContent, { target })

        const description = target.querySelector('.ai-description')
        expect(description?.textContent).toContain('2.1 GB')

        const hint = target.querySelector('.ai-hint')
        expect(hint?.textContent).toContain('settings')
    })

    it('renders nothing when state is hidden', () => {
        const target = document.createElement('div')
        mount(AiToastContent, { target })
        expect(target.querySelector('.ai-content')).toBeNull()
    })

    it("renders offer notification with Download, Not now, and I don't want AI buttons", () => {
        mockState.notificationState = 'offer'
        const target = document.createElement('div')
        mount(AiToastContent, { target })

        const title = target.querySelector('.ai-title')
        expect(title?.textContent).toBe('AI features available')

        const buttons = target.querySelectorAll('.ai-actions button')
        expect(buttons).toHaveLength(3)
        expect(buttons[0].textContent).toBe('Download')
        expect(buttons[1].textContent).toBe('Not now')
        expect(buttons[2].textContent).toBe("I don't want AI")
    })

    it('calls handleDownload when Download is clicked', () => {
        mockState.notificationState = 'offer'
        const target = document.createElement('div')
        mount(AiToastContent, { target })

        const downloadButton = target.querySelector('.btn-primary') as HTMLButtonElement
        downloadButton.click()
        flushSync()

        expect(handleDownload).toHaveBeenCalledOnce()
    })

    it('calls handleDismiss when Not now is clicked', () => {
        mockState.notificationState = 'offer'
        const target = document.createElement('div')
        mount(AiToastContent, { target })

        const dismissButton = target.querySelector('.btn-secondary') as HTMLButtonElement
        dismissButton.click()
        flushSync()

        expect(handleDismiss).toHaveBeenCalledOnce()
    })

    it("calls handleOptOut when I don't want AI is clicked", () => {
        mockState.notificationState = 'offer'
        const target = document.createElement('div')
        mount(AiToastContent, { target })

        const optOutButton = target.querySelector('.tertiary-link') as HTMLButtonElement
        optOutButton.click()
        flushSync()

        expect(handleOptOut).toHaveBeenCalledOnce()
    })

    it('renders downloading state with progress text', () => {
        mockState.notificationState = 'downloading'
        mockState.downloadProgress = { bytesDownloaded: 500000, totalBytes: 4000000, speed: 100000, etaSeconds: 35 }
        mockState.progressText = '12% — 500.0 KB / 4.0 MB — 100.0 KB/s — 35s remaining'

        const target = document.createElement('div')
        mount(AiToastContent, { target })

        const title = target.querySelector('.ai-title')
        expect(title?.textContent).toBe('Downloading AI model...')

        const progressText = target.querySelector('.ai-progress-text')
        expect(progressText?.textContent).toContain('12%')
    })

    it('renders downloading state with "Starting download..." when no total', () => {
        mockState.notificationState = 'downloading'
        mockState.downloadProgress = null

        const target = document.createElement('div')
        mount(AiToastContent, { target })

        const progressText = target.querySelector('.ai-progress-text')
        expect(progressText?.textContent).toBe('Starting download...')
    })

    it('calls handleCancel when Cancel is clicked in downloading state', () => {
        mockState.notificationState = 'downloading'
        mockState.downloadProgress = { bytesDownloaded: 100, totalBytes: 1000, speed: 50, etaSeconds: 18 }

        const target = document.createElement('div')
        mount(AiToastContent, { target })

        const cancelButton = target.querySelector('.btn-secondary') as HTMLButtonElement
        cancelButton.click()
        flushSync()

        expect(handleCancel).toHaveBeenCalledOnce()
    })

    it('renders installing state', () => {
        mockState.notificationState = 'installing'

        const target = document.createElement('div')
        mount(AiToastContent, { target })

        const title = target.querySelector('.ai-title')
        expect(title?.textContent).toBe('Setting up AI...')

        const description = target.querySelector('.ai-description')
        expect(description?.textContent).toBe('Starting inference server')

        // No buttons in installing state
        expect(target.querySelectorAll('.ai-actions button')).toHaveLength(0)
    })

    it('renders ready state with Got it button', () => {
        mockState.notificationState = 'ready'

        const target = document.createElement('div')
        mount(AiToastContent, { target })

        const title = target.querySelector('.ai-title')
        expect(title?.textContent).toBe('AI ready')

        const button = target.querySelector('.btn-primary') as HTMLButtonElement
        expect(button.textContent).toBe('Got it')
    })

    it('calls handleGotIt when Got it is clicked', () => {
        mockState.notificationState = 'ready'

        const target = document.createElement('div')
        mount(AiToastContent, { target })

        const button = target.querySelector('.btn-primary') as HTMLButtonElement
        button.click()
        flushSync()

        expect(handleGotIt).toHaveBeenCalledOnce()
    })

    it('shows progress bar when downloading with known total', () => {
        mockState.notificationState = 'downloading'
        mockState.downloadProgress = { bytesDownloaded: 2000000, totalBytes: 4000000, speed: 100000, etaSeconds: 20 }
        mockState.progressText = '50% — 2.0 MB / 4.0 MB'

        const target = document.createElement('div')
        mount(AiToastContent, { target })

        const progressBar = target.querySelector('.progress-bar-fill') as HTMLElement
        expect(progressBar).not.toBeNull()
        expect(progressBar.style.width).toBe('50%')
    })
})
