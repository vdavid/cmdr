// Settings commands and AI-related commands

import { invoke } from '@tauri-apps/api/core'

// ============================================================================
// Settings commands
// ============================================================================

/**
 * Checks if a port is available for binding.
 * @param port - The port number to check
 * @returns True if the port is available
 */
export async function checkPortAvailable(port: number): Promise<boolean> {
    return invoke<boolean>('check_port_available', { port })
}

/**
 * Finds an available port starting from the given port.
 * Scans up to 100 ports from the start port.
 * @param startPort - The port to start scanning from
 * @returns Available port number, or null if none found
 */
export async function findAvailablePort(startPort: number): Promise<number | null> {
    return invoke<number | null>('find_available_port', { startPort })
}

/**
 * Updates the file watcher debounce duration in the Rust backend.
 * This affects newly created watchers; existing watchers keep their original duration.
 * @param debounceMs - Debounce duration in milliseconds
 */
export async function updateFileWatcherDebounce(debounceMs: number): Promise<void> {
    await invoke('update_file_watcher_debounce', { debounceMs })
}

/**
 * Updates the Bonjour service resolve timeout in the Rust backend.
 * This affects future service resolutions; ongoing resolutions keep their original timeout.
 * @param timeoutMs - Timeout duration in milliseconds
 */
export async function updateServiceResolveTimeout(timeoutMs: number): Promise<void> {
    await invoke('update_service_resolve_timeout', { timeoutMs })
}

// ============================================================================
// AI commands
// ============================================================================

export type AiStatus = 'unavailable' | 'offer' | 'downloading' | 'installing' | 'available'

export interface AiDownloadProgress {
    bytesDownloaded: number
    totalBytes: number
    speed: number
    etaSeconds: number
}

/** Information about the current AI model. */
export interface AiModelInfo {
    id: string
    displayName: string
    sizeBytes: number
    /** Human-readable size (e.g., "4.3 GB") */
    sizeFormatted: string
}

/** Returns the current AI subsystem status. */
export async function getAiStatus(): Promise<AiStatus> {
    return invoke<AiStatus>('get_ai_status')
}

/** Returns information about the current AI model. */
export async function getAiModelInfo(): Promise<AiModelInfo> {
    return invoke<AiModelInfo>('get_ai_model_info')
}

/** Starts downloading the AI model and inference runtime. */
export async function startAiDownload(): Promise<void> {
    await invoke('start_ai_download')
}

/** Cancels an in-progress AI download. */
export async function cancelAiDownload(): Promise<void> {
    await invoke('cancel_ai_download')
}

/** Dismisses the AI offer notification for 7 days. */
export async function dismissAiOffer(): Promise<void> {
    await invoke('dismiss_ai_offer')
}

/** Uninstalls the AI model and binary, resets state. */
export async function uninstallAi(): Promise<void> {
    await invoke('uninstall_ai')
}

/** Permanently opts out of AI features. Can be re-enabled in settings. */
export async function optOutAi(): Promise<void> {
    await invoke('opt_out_ai')
}

/** Re-enables AI features after opting out. */
export async function optInAi(): Promise<void> {
    await invoke('opt_in_ai')
}

/** Returns whether the user has opted out of AI features. */
export async function isAiOptedOut(): Promise<boolean> {
    return invoke<boolean>('is_ai_opted_out')
}

/** Gets AI-generated folder name suggestions for the current directory. */
export async function getFolderSuggestions(
    listingId: string,
    currentPath: string,
    includeHidden: boolean,
): Promise<string[]> {
    try {
        return await invoke<string[]>('get_folder_suggestions', { listingId, currentPath, includeHidden })
    } catch {
        return []
    }
}
