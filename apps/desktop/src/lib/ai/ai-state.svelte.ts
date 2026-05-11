import { listen } from '@tauri-apps/api/event'
import {
  cancelAiDownload,
  dismissAiOffer,
  formatBytes,
  formatDuration,
  getAiModelInfo,
  getAiStatus,
  optOutAi,
  startAiDownload,
  type AiDownloadProgress,
  type AiModelInfo,
  type AiStatus,
} from '$lib/tauri-commands'
import { getSetting, setSetting } from '$lib/settings'
import { loadSettings } from '$lib/settings-store'
import { tierClassForUnit } from '$lib/file-explorer/selection/selection-info-utils'

/** Wraps a formatted size string (e.g. "1.0 GB") in a colored span for HTML embedding. */
function colorSize(text: string): string {
  const spaceIndex = text.lastIndexOf(' ')
  const unit = spaceIndex >= 0 ? text.slice(spaceIndex + 1) : ''
  return `<span class="${tierClassForUnit(unit)}">${text}</span>`
}

type AiNotificationState = 'hidden' | 'offer' | 'downloading' | 'installing' | 'ready' | 'starting'

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
  /**
   * True once first-launch onboarding (FDA prompt) has finished. While false, an `Offer` status
   * from the backend is suppressed so the AI toast doesn't pile on top of the FDA modal. Seeded
   * from `loadSettings().isOnboarded` in `initAiState`; flipped by `notifyAiOnboardingComplete`.
   */
  onboarded: boolean
  /**
   * True when the backend reported `Offer` but the toast was suppressed because onboarding wasn't
   * complete. `notifyAiOnboardingComplete` reads this to surface the offer once the gate opens.
   * Cleared by user-driven exits from the offer (`dismiss`, `optOut`, `download`) so the gate
   * doesn't resurrect a decision the user has already made.
   */
  pendingOffer: boolean
}

const aiState = $state<AiStateData>({
  notificationState: 'hidden',
  downloadProgress: null,
  progressText: '',
  modelInfo: null,
  downloadToastUserDismissed: false,
  onboarded: false,
  pendingOffer: false,
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
  aiState.onboarded = false
  aiState.pendingOffer = false
}

/** Marks the downloading toast as user-dismissed for the current download run. */
export function markDownloadToastDismissed(): void {
  aiState.downloadToastUserDismissed = true
}

export async function initAiState(): Promise<() => void> {
  // Don't show toast when provider is off or cloud
  const aiProvider = getSetting('ai.provider')
  if (aiProvider === 'off' || aiProvider === 'cloud') {
    return () => {}
  }

  // Seed the onboarded gate from persisted settings. While `false`, an Offer status stays
  // hidden — the FDA modal owns the screen during first launch. Returning users (isOnboarded
  // already true) skip the gate entirely.
  //
  // Sticky merge instead of plain assignment: `+page.svelte` may have already called
  // `notifyAiOnboardingComplete()` while `loadSettings()` was in flight (legacy fallback paths
  // run no user input gate). A plain assignment would overwrite the hook's `true` back to a
  // stale `false` if disk hadn't synced yet.
  const settings = await loadSettings()
  aiState.onboarded = aiState.onboarded || settings.isOnboarded

  // Fetch model info and status in parallel
  const [status, modelInfo] = await Promise.all([getAiStatus(), getAiModelInfo()])
  aiState.modelInfo = modelInfo
  updateNotificationFromStatus(status)

  const unlistenProgress = await listen<AiDownloadProgress>('ai-download-progress', (event) => {
    aiState.downloadProgress = event.payload
    aiState.progressText = formatProgressText(event.payload)
  })

  const unlistenInstalling = await listen('ai-installing', () => {
    aiState.notificationState = 'installing'
    aiState.downloadProgress = null
  })

  const unlistenComplete = await listen('ai-install-complete', () => {
    aiState.notificationState = 'ready'
    aiState.downloadProgress = null
  })

  // Listen for server starting (shown on app startup when model already downloaded)
  const unlistenStarting = await listen('ai-starting', () => {
    aiState.notificationState = 'starting'
  })

  // Listen for server ready (hides the "starting" notification)
  const unlistenServerReady = await listen('ai-server-ready', () => {
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

export async function handleDownload(): Promise<void> {
  // Ensure provider is set to local when user accepts download
  setSetting('ai.provider', 'local')
  aiState.notificationState = 'downloading'
  aiState.downloadProgress = { bytesDownloaded: 0, totalBytes: 0, speed: 0, etaSeconds: 0 }
  // New download run — clear any previous user-dismissed flag so the toast shows again.
  aiState.downloadToastUserDismissed = false
  try {
    await startAiDownload()
  } catch {
    // On error or cancel, reset to offer state
    aiState.notificationState = 'offer'
    aiState.downloadProgress = null
  }
}

export async function handleCancel(): Promise<void> {
  await cancelAiDownload()
  aiState.notificationState = 'offer'
  aiState.downloadProgress = null
}

export async function handleDismiss(): Promise<void> {
  await dismissAiOffer()
  aiState.notificationState = 'hidden'
}

export async function handleOptOut(): Promise<void> {
  await optOutAi()
  aiState.notificationState = 'hidden'
}

export function handleGotIt(): void {
  aiState.notificationState = 'hidden'
}

function updateNotificationFromStatus(status: AiStatus): void {
  switch (status) {
    case 'available':
      aiState.notificationState = 'hidden' // Already installed, don't show anything
      break
    case 'offer':
      // Suppress the offer until first-launch onboarding is done. The pending flag lets
      // `notifyAiOnboardingComplete` surface it the moment the gate opens.
      if (aiState.onboarded) {
        aiState.notificationState = 'offer'
        aiState.pendingOffer = false
      } else {
        aiState.notificationState = 'hidden'
        aiState.pendingOffer = true
      }
      break
    default:
      aiState.notificationState = 'hidden'
  }
}

/**
 * Marks first-launch onboarding as complete so a deferred AI offer can finally surface.
 * Called from `routes/(main)/+page.svelte` once the FDA prompt closes (Allow or Deny path).
 *
 * The Allow path lands on `isOnboarded: true` via `notifyOnboardingComplete()`, so the next
 * launch reads onboarded=true and skips the gate entirely. The Deny path needs this hook to
 * reveal the offer in the same session.
 */
export function notifyAiOnboardingComplete(): void {
  aiState.onboarded = true
  if (aiState.pendingOffer) {
    aiState.notificationState = 'offer'
    aiState.pendingOffer = false
  }
}

function formatProgressText(progress: AiDownloadProgress): string {
  if (progress.totalBytes === 0) return 'Starting download...'
  const percent = Math.round((progress.bytesDownloaded / progress.totalBytes) * 100)
  const downloaded = colorSize(formatBytes(progress.bytesDownloaded))
  const total = colorSize(formatBytes(progress.totalBytes))
  const speed = colorSize(formatBytes(progress.speed))
  const eta = progress.etaSeconds > 0 ? formatDuration(progress.etaSeconds) : ''
  const etaPart = eta ? ` — ${eta} remaining` : ''
  return `${String(percent)}% — ${downloaded} / ${total} — ${speed}/s${etaPart}`
}
