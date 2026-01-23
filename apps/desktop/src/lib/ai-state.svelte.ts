import { listen } from '@tauri-apps/api/event'
import {
    cancelAiDownload,
    dismissAiOffer,
    formatBytes,
    formatDuration,
    getAiStatus,
    startAiDownload,
    type AiDownloadProgress,
    type AiStatus,
} from './tauri-commands'

type AiNotificationState = 'hidden' | 'offer' | 'downloading' | 'installing' | 'ready'

interface AiStateData {
    notificationState: AiNotificationState
    downloadProgress: AiDownloadProgress | null
    progressText: string
}

const aiState = $state<AiStateData>({
    notificationState: 'hidden',
    downloadProgress: null,
    progressText: '',
})

export function getAiState(): AiStateData {
    return aiState
}

export async function initAiState(): Promise<() => void> {
    const status = await getAiStatus()
    updateNotificationFromStatus(status)

    const unlistenProgress = await listen<AiDownloadProgress>('ai-download-progress', (event) => {
        aiState.downloadProgress = event.payload
        aiState.progressText = formatProgressText(event.payload)
    })

    const unlistenComplete = await listen('ai-install-complete', () => {
        aiState.notificationState = 'ready'
        aiState.downloadProgress = null
    })

    return () => {
        unlistenProgress()
        unlistenComplete()
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
