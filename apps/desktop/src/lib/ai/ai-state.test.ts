import { describe, it, expect, vi, beforeEach } from 'vitest'
import { listen } from '@tauri-apps/api/event'

vi.mock('$lib/tauri-commands', async () => {
  const { formatBytes, formatDuration } = await import('$lib/tauri-commands/write-operations')
  return {
    formatBytes,
    formatDuration,
    getAiStatus: vi.fn(),
    getAiModelInfo: vi.fn(),
    startAiDownload: vi.fn(),
    cancelAiDownload: vi.fn(),
    dismissAiOffer: vi.fn(),
    optOutAi: vi.fn(),
  }
})

vi.mock('$lib/settings-store', () => ({
  loadSettings: vi.fn(),
}))

import { getAiStatus, getAiModelInfo, startAiDownload, cancelAiDownload, dismissAiOffer } from '$lib/tauri-commands'
import { loadSettings } from '$lib/settings-store'
import {
  getAiState,
  initAiState,
  handleDownload,
  handleCancel,
  handleDismiss,
  handleGotIt,
  notifyAiOnboardingComplete,
  resetForTesting,
} from './ai-state.svelte'

const mockModelInfo = {
  id: 'ministral-3b-instruct-q4km',
  displayName: 'Ministral 3B',
  sizeBytes: 2147023008,
  sizeFormatted: '2.1 GB',
  kvBytesPerToken: 106496,
  baseOverheadBytes: 3500000000,
}

const ONBOARDED = {
  showHiddenFiles: true,
  fullDiskAccessChoice: 'allow' as const,
  isOnboarded: true,
}

const NOT_ONBOARDED = {
  showHiddenFiles: true,
  fullDiskAccessChoice: 'notAskedYet' as const,
  isOnboarded: false,
}

describe('ai-state', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    resetForTesting()
    // Default: existing tests assume the user is past onboarding so the offer surfaces.
    vi.mocked(loadSettings).mockResolvedValue(ONBOARDED)
  })

  describe('getAiState', () => {
    it('returns initial hidden state', () => {
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

      await initAiState()

      const state = getAiState()
      expect(state.notificationState).toBe('offer')
    })

    it('stays hidden when status is available', async () => {
      vi.mocked(getAiStatus).mockResolvedValue('available')
      vi.mocked(getAiModelInfo).mockResolvedValue(mockModelInfo)

      await initAiState()

      const state = getAiState()
      expect(state.notificationState).toBe('hidden')
    })

    it('stays hidden when status is unavailable', async () => {
      vi.mocked(getAiStatus).mockResolvedValue('unavailable')
      vi.mocked(getAiModelInfo).mockResolvedValue(mockModelInfo)

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

      await initAiState()

      completeCallback?.()

      const state = getAiState()
      expect(state.notificationState).toBe('ready')
      expect(state.downloadProgress).toBeNull()
    })
  })

  describe('onboarding gate', () => {
    it('suppresses Offer when isOnboarded is false', async () => {
      vi.mocked(loadSettings).mockResolvedValue(NOT_ONBOARDED)
      vi.mocked(getAiStatus).mockResolvedValue('offer')
      vi.mocked(getAiModelInfo).mockResolvedValue(mockModelInfo)

      await initAiState()

      const state = getAiState()
      expect(state.notificationState).toBe('hidden')
      expect(state.pendingOffer).toBe(true)
      expect(state.onboarded).toBe(false)
    })

    it('surfaces Offer immediately when isOnboarded is true at init', async () => {
      vi.mocked(loadSettings).mockResolvedValue(ONBOARDED)
      vi.mocked(getAiStatus).mockResolvedValue('offer')
      vi.mocked(getAiModelInfo).mockResolvedValue(mockModelInfo)

      await initAiState()

      const state = getAiState()
      expect(state.notificationState).toBe('offer')
      expect(state.pendingOffer).toBe(false)
      expect(state.onboarded).toBe(true)
    })

    it('surfaces a deferred Offer when notifyAiOnboardingComplete is called', async () => {
      vi.mocked(loadSettings).mockResolvedValue(NOT_ONBOARDED)
      vi.mocked(getAiStatus).mockResolvedValue('offer')
      vi.mocked(getAiModelInfo).mockResolvedValue(mockModelInfo)

      await initAiState()
      expect(getAiState().notificationState).toBe('hidden')

      notifyAiOnboardingComplete()

      const state = getAiState()
      expect(state.notificationState).toBe('offer')
      expect(state.pendingOffer).toBe(false)
      expect(state.onboarded).toBe(true)
    })

    it('does not flip state to Offer when there is no pending offer', async () => {
      // Backend says Available: model already installed, no offer to surface.
      vi.mocked(loadSettings).mockResolvedValue(NOT_ONBOARDED)
      vi.mocked(getAiStatus).mockResolvedValue('available')
      vi.mocked(getAiModelInfo).mockResolvedValue(mockModelInfo)

      await initAiState()
      expect(getAiState().notificationState).toBe('hidden')

      notifyAiOnboardingComplete()

      const state = getAiState()
      expect(state.notificationState).toBe('hidden')
      expect(state.onboarded).toBe(true)
    })

    it('is idempotent: second call does not re-trigger the offer', async () => {
      vi.mocked(loadSettings).mockResolvedValue(NOT_ONBOARDED)
      vi.mocked(getAiStatus).mockResolvedValue('offer')
      vi.mocked(getAiModelInfo).mockResolvedValue(mockModelInfo)

      await initAiState()
      notifyAiOnboardingComplete()
      // User dismisses the offer.
      vi.mocked(dismissAiOffer).mockResolvedValue(undefined)
      await handleDismiss()
      expect(getAiState().notificationState).toBe('hidden')

      // A second onboarding-complete signal must NOT resurrect the dismissed offer.
      notifyAiOnboardingComplete()

      expect(getAiState().notificationState).toBe('hidden')
    })

    it('does not regress onboarded=true if notifyAiOnboardingComplete fires while loadSettings is in flight', async () => {
      // Race: legacy fallback paths in `+page.svelte` (hasFda + !isOnboarded, deny + !isOnboarded)
      // call `notifyAiOnboardingComplete()` without any user gate. If `initAiState` is still
      // awaiting `loadSettings()` when that hook fires, and disk hasn't synced the
      // `notifyOnboardingComplete()` save yet. A plain assignment in initAiState would clobber
      // the hook's `onboarded = true` back to a stale `false`, leaving the offer permanently gated.
      let resolveSettings: ((value: typeof NOT_ONBOARDED) => void) | undefined
      vi.mocked(loadSettings).mockImplementation(
        () =>
          new Promise<typeof NOT_ONBOARDED>((resolve) => {
            resolveSettings = resolve
          }),
      )
      vi.mocked(getAiStatus).mockResolvedValue('offer')
      vi.mocked(getAiModelInfo).mockResolvedValue(mockModelInfo)

      const initPromise = initAiState()

      // Simulate the legacy fallback firing the hook before loadSettings resolves.
      notifyAiOnboardingComplete()
      expect(getAiState().onboarded).toBe(true)

      // Now resolve loadSettings with stale `isOnboarded: false` (disk not yet synced).
      resolveSettings?.(NOT_ONBOARDED)
      await initPromise

      const state = getAiState()
      expect(state.onboarded).toBe(true)
      // Status is `offer` and onboarded is sticky-true, so the offer surfaces directly.
      expect(state.notificationState).toBe('offer')
      expect(state.pendingOffer).toBe(false)
    })

    it('does not gate downloading state: install events flow through even when not onboarded', async () => {
      // Edge case: user starts onboarding, app keeps running, backend somehow emits installing/ready.
      // The gate only suppresses the initial Offer, not in-flight install signals (which can only
      // arise after the user accepted the offer somewhere).
      vi.mocked(loadSettings).mockResolvedValue(NOT_ONBOARDED)
      vi.mocked(getAiStatus).mockResolvedValue('offer')
      vi.mocked(getAiModelInfo).mockResolvedValue(mockModelInfo)
      let installingCallback: (() => void) | undefined
      let completeCallback: (() => void) | undefined
      vi.mocked(listen).mockImplementation((event, callback) => {
        if (event === 'ai-installing') installingCallback = callback as () => void
        if (event === 'ai-install-complete') completeCallback = callback as () => void
        return Promise.resolve(() => {})
      })

      await initAiState()
      expect(getAiState().notificationState).toBe('hidden')

      installingCallback?.()
      expect(getAiState().notificationState).toBe('installing')

      completeCallback?.()
      expect(getAiState().notificationState).toBe('ready')
    })
  })
})
