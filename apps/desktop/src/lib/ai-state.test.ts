import { describe, it, expect, vi, beforeEach } from 'vitest'
import { listen } from '@tauri-apps/api/event'

vi.mock('$lib/tauri-commands', async (importOriginal) => {
    const original = await importOriginal<typeof import('$lib/tauri-commands')>()
    return {
        ...original,
        getAiStatus: vi.fn(),
        getAiModelInfo: vi.fn(),
        startAiDownload: vi.fn(),
        cancelAiDownload: vi.fn(),
        dismissAiOffer: vi.fn(),
    }
})

import { getAiStatus, getAiModelInfo, startAiDownload, cancelAiDownload, dismissAiOffer } from '$lib/tauri-commands'

const mockModelInfo = {
    id: 'ministral-3b-instruct-q4km',
    displayName: 'Ministral 3B',
    sizeBytes: 2147023008,
    sizeFormatted: '2.1 GB',
}

describe('ai-state', () => {
    beforeEach(() => {
        vi.clearAllMocks()
        vi.resetModules()
    })

    async function loadModule() {
        return await import('./ai-state.svelte')
    }

    describe('getAiState', () => {
        it('returns initial hidden state', async () => {
            const { getAiState } = await loadModule()
            const state = getAiState()
            expect(state.notificationState).toBe('hidden')
            expect(state.downloadProgress).toBeNull()
            expect(state.progressText).toBe('')
        })
    })

    describe('initAiState', () => {
        it('sets offer state when status is offer', async () => {
            vi.mocked(getAiStatus).mockResolvedValue('offer')
            vi.mocked(getAiModelInfo).mockResolvedValue(mockModelInfo)
            const { initAiState, getAiState } = await loadModule()

            await initAiState()

            const state = getAiState()
            expect(state.notificationState).toBe('offer')
        })

        it('stays hidden when status is available', async () => {
            vi.mocked(getAiStatus).mockResolvedValue('available')
            vi.mocked(getAiModelInfo).mockResolvedValue(mockModelInfo)
            const { initAiState, getAiState } = await loadModule()

            await initAiState()

            const state = getAiState()
            expect(state.notificationState).toBe('hidden')
        })

        it('stays hidden when status is unavailable', async () => {
            vi.mocked(getAiStatus).mockResolvedValue('unavailable')
            vi.mocked(getAiModelInfo).mockResolvedValue(mockModelInfo)
            const { initAiState, getAiState } = await loadModule()

            await initAiState()

            const state = getAiState()
            expect(state.notificationState).toBe('hidden')
        })

        it('registers event listeners and returns cleanup function', async () => {
            vi.mocked(getAiStatus).mockResolvedValue('offer')
            vi.mocked(getAiModelInfo).mockResolvedValue(mockModelInfo)
            const unlistenFns = [vi.fn(), vi.fn(), vi.fn(), vi.fn(), vi.fn()]
            vi.mocked(listen)
                .mockResolvedValueOnce(unlistenFns[0])
                .mockResolvedValueOnce(unlistenFns[1])
                .mockResolvedValueOnce(unlistenFns[2])
                .mockResolvedValueOnce(unlistenFns[3])
                .mockResolvedValueOnce(unlistenFns[4])

            const { initAiState } = await loadModule()
            const cleanup = await initAiState()

            expect(listen).toHaveBeenCalledTimes(5)
            expect(listen).toHaveBeenCalledWith('ai-download-progress', expect.any(Function))
            expect(listen).toHaveBeenCalledWith('ai-installing', expect.any(Function))
            expect(listen).toHaveBeenCalledWith('ai-install-complete', expect.any(Function))
            expect(listen).toHaveBeenCalledWith('ai-starting', expect.any(Function))
            expect(listen).toHaveBeenCalledWith('ai-server-ready', expect.any(Function))

            cleanup()
            for (const unlisten of unlistenFns) {
                expect(unlisten).toHaveBeenCalledOnce()
            }
        })
    })

    describe('handleDownload', () => {
        it('sets downloading state and calls startAiDownload', async () => {
            vi.mocked(getAiStatus).mockResolvedValue('offer')
            vi.mocked(getAiModelInfo).mockResolvedValue(mockModelInfo)
            vi.mocked(startAiDownload).mockResolvedValue(undefined)

            const { initAiState, handleDownload, getAiState } = await loadModule()
            await initAiState()

            await handleDownload()

            const state = getAiState()
            expect(state.notificationState).toBe('downloading')
            expect(startAiDownload).toHaveBeenCalledOnce()
        })

        it('resets to offer state on download error', async () => {
            vi.mocked(getAiStatus).mockResolvedValue('offer')
            vi.mocked(getAiModelInfo).mockResolvedValue(mockModelInfo)
            vi.mocked(startAiDownload).mockRejectedValue(new Error('Network error'))

            const { initAiState, handleDownload, getAiState } = await loadModule()
            await initAiState()

            await handleDownload()

            const state = getAiState()
            expect(state.notificationState).toBe('offer')
            expect(state.downloadProgress).toBeNull()
        })
    })

    describe('handleCancel', () => {
        it('calls cancelAiDownload and resets to offer', async () => {
            vi.mocked(getAiStatus).mockResolvedValue('offer')
            vi.mocked(getAiModelInfo).mockResolvedValue(mockModelInfo)
            vi.mocked(cancelAiDownload).mockResolvedValue(undefined)

            const { initAiState, handleCancel, getAiState } = await loadModule()
            await initAiState()

            await handleCancel()

            expect(cancelAiDownload).toHaveBeenCalledOnce()
            const state = getAiState()
            expect(state.notificationState).toBe('offer')
            expect(state.downloadProgress).toBeNull()
        })
    })

    describe('handleDismiss', () => {
        it('calls dismissAiOffer and hides notification', async () => {
            vi.mocked(getAiStatus).mockResolvedValue('offer')
            vi.mocked(getAiModelInfo).mockResolvedValue(mockModelInfo)
            vi.mocked(dismissAiOffer).mockResolvedValue(undefined)

            const { initAiState, handleDismiss, getAiState } = await loadModule()
            await initAiState()
            expect(getAiState().notificationState).toBe('offer')

            await handleDismiss()

            expect(dismissAiOffer).toHaveBeenCalledOnce()
            expect(getAiState().notificationState).toBe('hidden')
        })
    })

    describe('handleGotIt', () => {
        it('hides the notification', async () => {
            vi.mocked(getAiStatus).mockResolvedValue('offer')
            vi.mocked(getAiModelInfo).mockResolvedValue(mockModelInfo)
            const { initAiState, handleGotIt, getAiState } = await loadModule()
            await initAiState()

            handleGotIt()

            expect(getAiState().notificationState).toBe('hidden')
        })
    })

    describe('download progress events', () => {
        it('updates progress state on ai-download-progress event', async () => {
            vi.mocked(getAiStatus).mockResolvedValue('offer')
            vi.mocked(getAiModelInfo).mockResolvedValue(mockModelInfo)
            let progressCallback: ((event: { payload: unknown }) => void) | undefined
            vi.mocked(listen).mockImplementation((event, callback) => {
                if (event === 'ai-download-progress') {
                    progressCallback = callback as (event: { payload: unknown }) => void
                }
                return Promise.resolve(() => {})
            })

            const { initAiState, getAiState } = await loadModule()
            await initAiState()

            progressCallback?.({
                payload: { bytesDownloaded: 1000000, totalBytes: 4000000, speed: 50000, etaSeconds: 60 },
            })

            const state = getAiState()
            expect(state.downloadProgress).not.toBeNull()
            expect(state.downloadProgress?.bytesDownloaded).toBe(1000000)
            expect(state.progressText).toContain('25%')
        })

        it('shows "Starting download..." when totalBytes is 0', async () => {
            vi.mocked(getAiStatus).mockResolvedValue('offer')
            vi.mocked(getAiModelInfo).mockResolvedValue(mockModelInfo)
            let progressCallback: ((event: { payload: unknown }) => void) | undefined
            vi.mocked(listen).mockImplementation((event, callback) => {
                if (event === 'ai-download-progress') {
                    progressCallback = callback as (event: { payload: unknown }) => void
                }
                return Promise.resolve(() => {})
            })

            const { initAiState, getAiState } = await loadModule()
            await initAiState()

            progressCallback?.({ payload: { bytesDownloaded: 0, totalBytes: 0, speed: 0, etaSeconds: 0 } })

            const state = getAiState()
            expect(state.progressText).toBe('Starting download...')
        })

        it('sets installing state on ai-installing event', async () => {
            vi.mocked(getAiStatus).mockResolvedValue('offer')
            vi.mocked(getAiModelInfo).mockResolvedValue(mockModelInfo)
            let installingCallback: (() => void) | undefined
            vi.mocked(listen).mockImplementation((event, callback) => {
                if (event === 'ai-installing') {
                    installingCallback = callback as () => void
                }
                return Promise.resolve(() => {})
            })

            const { initAiState, getAiState } = await loadModule()
            await initAiState()

            installingCallback?.()

            const state = getAiState()
            expect(state.notificationState).toBe('installing')
            expect(state.downloadProgress).toBeNull()
        })

        it('sets ready state on ai-install-complete event', async () => {
            vi.mocked(getAiStatus).mockResolvedValue('offer')
            vi.mocked(getAiModelInfo).mockResolvedValue(mockModelInfo)
            let completeCallback: (() => void) | undefined
            vi.mocked(listen).mockImplementation((event, callback) => {
                if (event === 'ai-install-complete') {
                    completeCallback = callback as () => void
                }
                return Promise.resolve(() => {})
            })

            const { initAiState, getAiState } = await loadModule()
            await initAiState()

            completeCallback?.()

            const state = getAiState()
            expect(state.notificationState).toBe('ready')
            expect(state.downloadProgress).toBeNull()
        })
    })
})
