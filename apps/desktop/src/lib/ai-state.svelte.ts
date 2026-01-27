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
} from './tauri-commands'

type AiNotificationState = 'hidden' | 'offer' | 'downloading' | 'installing' | 'ready' | 'starting'

interface AiStateData {
    notificationState: AiNotificationState
    downloadProgress: AiDownloadProgress | null
    progressText: string
    modelInfo: AiModelInfo | null
}

const aiState = $state<AiStateData>({
    notificationState: 'hidden',
    downloadProgress: null,
    progressText: '',
    modelInfo: null,
})

export function getAiState(): AiStateData {
    return aiState
}

export async function initAiState(): Promise<() => void> {
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
    aiState.notificationState = 'downloading'
    aiState.downloadProgress = { bytesDownloaded: 0, totalBytes: 0, speed: 0, etaSeconds: 0 }
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
            aiState.notificationState = 'offer'
            break
        default:
            aiState.notificationState = 'hidden'
    }
}

function formatProgressText(progress: AiDownloadProgress): string {
    if (progress.totalBytes === 0) return 'Starting download...'
    const percent = Math.round((progress.bytesDownloaded / progress.totalBytes) * 100)
    const downloaded = formatBytes(progress.bytesDownloaded)
    const total = formatBytes(progress.totalBytes)
    const speed = formatBytes(progress.speed)
    const eta = progress.etaSeconds > 0 ? formatDuration(progress.etaSeconds) : ''
    const etaPart = eta ? ` — ${eta} remaining` : ''
    return `${String(percent)}% — ${downloaded} / ${total} — ${speed}/s${etaPart}`
}
