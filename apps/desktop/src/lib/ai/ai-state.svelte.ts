import {
  cancelAiDownload,
  formatBytes,
  formatDuration,
  getAiModelInfo,
  getAiStatus,
  isE2eMode,
  onAiDownloadProgress,
  onAiInstallComplete,
  onAiInstalling,
  onAiServerReady,
  onAiStarting,
  type AiDownloadProgress,
  type AiModelInfo,
  type AiStatus,
} from '$lib/tauri-commands'
import { getSetting } from '$lib/settings'
import { colorizeSizeString } from '$lib/file-explorer/selection/selection-info-utils'

/**
 * The AI toast's lifecycle now only tracks the runtime install pipeline: download → install →
 * ready → starting. First-launch AI consent (the old `'offer'` state) is owned end-to-end by the
 * onboarding wizard, so the toast no longer surfaces an offer and no longer needs an `onboarded`
 * gate or a `pendingOffer` deferred-signal. See `apps/desktop/src/lib/onboarding/CLAUDE.md` for
 * the wizard side.
 */
type AiNotificationState = 'hidden' | 'downloading' | 'installing' | 'ready' | 'starting'

interface AiStateData {
  notificationState: AiNotificationState
  downloadProgress: AiDownloadProgress | null
  progressText: string
  modelInfo: AiModelInfo | null
  /**
   * Set to true when the user clicks the X on the downloading toast. While true, the toast sync
   * effect won't re-add the toast, even though the download keeps running in the background. The
   * flag resets whenever a new download run starts (on transition into `'downloading'`).
   */
  downloadToastUserDismissed: boolean
}

const aiState = $state<AiStateData>({
  notificationState: 'hidden',
  downloadProgress: null,
  progressText: '',
  modelInfo: null,
  downloadToastUserDismissed: false,
})

export function getAiState(): AiStateData {
  return aiState
}

/** Resets all state to initial values. For use in tests only. */
export function resetForTesting(): void {
  aiState.notificationState = 'hidden'
  aiState.downloadProgress = null
  aiState.progressText = ''
  aiState.modelInfo = null
  aiState.downloadToastUserDismissed = false
}

/** Marks the downloading toast as user-dismissed for the current download run. */
export function markDownloadToastDismissed(): void {
  aiState.downloadToastUserDismissed = true
}

export async function initAiState(): Promise<() => void> {
  // Suppress the AI runtime toast in E2E. The leak-detector safety net in
  // fixtures.ts fails any test that leaves a toast open, and ~160 specs would
  // otherwise need to dismiss it.
  if (await isE2eMode()) {
    return () => {}
  }

  // Don't show toast when provider is off or cloud
  const aiProvider = getSetting('ai.provider')
  if (aiProvider === 'off' || aiProvider === 'cloud') {
    return () => {}
  }

  // Fetch model info and status in parallel
  const [status, modelInfo] = await Promise.all([getAiStatus(), getAiModelInfo()])
  aiState.modelInfo = modelInfo
  updateNotificationFromStatus(status)

  const unlistenProgress = await onAiDownloadProgress((payload) => {
    aiState.downloadProgress = payload
    aiState.progressText = formatProgressText(payload)
  })

  const unlistenInstalling = await onAiInstalling(() => {
    aiState.notificationState = 'installing'
    aiState.downloadProgress = null
  })

  const unlistenComplete = await onAiInstallComplete(() => {
    aiState.notificationState = 'ready'
    aiState.downloadProgress = null
  })

  // Listen for server starting (shown on app startup when model already downloaded)
  const unlistenStarting = await onAiStarting(() => {
    aiState.notificationState = 'starting'
  })

  // Listen for server ready (hides the "starting" notification)
  const unlistenServerReady = await onAiServerReady(() => {
    aiState.notificationState = 'hidden'
  })

  return () => {
    unlistenProgress()
    unlistenInstalling()
    unlistenComplete()
    unlistenStarting()
    unlistenServerReady()
  }
}

export async function handleCancel(): Promise<void> {
  await cancelAiDownload()
  aiState.notificationState = 'hidden'
  aiState.downloadProgress = null
}

export function handleGotIt(): void {
  aiState.notificationState = 'hidden'
}

function updateNotificationFromStatus(status: AiStatus): void {
  // The backend's `Offer` status is no longer surfaced as a toast — the wizard owns first-launch
  // AI consent. Treat every status as "hidden" by default; the runtime events (`ai-installing`,
  // `ai-install-complete`, `ai-starting`, `ai-server-ready`) drive the toast from there.
  switch (status) {
    case 'available':
      aiState.notificationState = 'hidden' // Already installed; runtime events drive any later UI.
      break
    default:
      aiState.notificationState = 'hidden'
  }
}

function formatProgressText(progress: AiDownloadProgress): string {
  if (progress.totalBytes === 0) return 'Starting download...'
  const percent = Math.round((progress.bytesDownloaded / progress.totalBytes) * 100)
  const downloaded = colorizeSizeString(formatBytes(progress.bytesDownloaded))
  const total = colorizeSizeString(formatBytes(progress.totalBytes))
  const speed = colorizeSizeString(formatBytes(progress.speed))
  const eta = progress.etaSeconds > 0 ? formatDuration(progress.etaSeconds) : ''
  const etaPart = eta ? ` · ${eta} remaining` : ''
  return `${String(percent)}% · ${downloaded} / ${total} · ${speed}/s${etaPart}`
}
