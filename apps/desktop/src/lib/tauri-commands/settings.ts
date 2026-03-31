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
// MCP server commands
// ============================================================================

/** Starts or stops the MCP server. Pass the current port so it binds correctly on enable. */
export async function setMcpEnabled(enabled: boolean, port: number): Promise<void> {
  await invoke('set_mcp_enabled', { enabled, port })
}

/** Restarts the MCP server on a new port. No-op if the server isn't currently running. */
export async function setMcpPort(port: number): Promise<void> {
  await invoke('set_mcp_port', { port })
}

/** Returns whether the MCP server is currently running. */
export async function getMcpRunning(): Promise<boolean> {
  return invoke<boolean>('get_mcp_running')
}

/** Returns the port the MCP server is actually listening on, or null if not running. */
export async function getMcpPort(): Promise<number | null> {
  return invoke<number | null>('get_mcp_port')
}

// ============================================================================
// Indexing commands
// ============================================================================

/**
 * Toggles drive indexing on or off.
 * When enabled: starts scanning (resumes from existing DB if available).
 * When disabled: stops all scans and watchers; DB stays on disk.
 */
export async function setIndexingEnabled(enabled: boolean): Promise<void> {
  await invoke('set_indexing_enabled', { enabled })
}

/** Index directory stats returned by the batch lookup. */
export interface DirStats {
  path: string
  recursiveSize: number
  recursivePhysicalSize: number
  recursiveFileCount: number
  recursiveDirCount: number
}

/**
 * Fetches index stats for a batch of directory paths.
 * Returns one entry per input path (null if the path has no index data yet).
 */
export async function getDirStatsBatch(paths: string[]): Promise<(DirStats | null)[]> {
  return invoke<(DirStats | null)[]>('get_dir_stats_batch', { paths })
}

// ============================================================================
// System memory
// ============================================================================

/** System RAM breakdown in bytes. Categories are non-overlapping and sum to `totalBytes`. */
export interface SystemMemoryInfo {
  totalBytes: number
  /** Wired + compressor-occupied memory (kernel, drivers — can't be freed). */
  wiredBytes: number
  /** App memory: active + inactive - purgeable (process memory the user can free by quitting apps). */
  appBytes: number
  /** Free: free + purgeable + speculative (available for new allocations). */
  freeBytes: number
}

/** Returns system RAM breakdown for the RAM gauge. */
export async function getSystemMemoryInfo(): Promise<SystemMemoryInfo> {
  return invoke<SystemMemoryInfo>('get_system_memory_info')
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
  /** Human-readable size (like "4.3 GB") */
  sizeFormatted: string
  /** Bytes per token for KV cache (used for memory estimation) */
  kvBytesPerToken: number
  /** Base memory overhead in bytes (model weights + compute buffers) */
  baseOverheadBytes: number
}

/** Runtime status of the AI subsystem. */
export interface AiRuntimeStatus {
  serverRunning: boolean
  serverStarting: boolean
  pid: number | null
  port: number | null
  modelInstalled: boolean
  modelName: string
  modelSizeBytes: number
  modelSizeFormatted: string
  downloadInProgress: boolean
  localAiSupported: boolean
  kvBytesPerToken: number
  baseOverheadBytes: number
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

/** Returns the full runtime status of the AI subsystem. */
export async function getAiRuntimeStatus(): Promise<AiRuntimeStatus> {
  return invoke<AiRuntimeStatus>('get_ai_runtime_status')
}

/** Pushes AI config to the backend. Triggers server start if provider is local + model installed. */
export async function configureAi(
  provider: string,
  contextSize: number,
  openaiApiKey: string,
  openaiBaseUrl: string,
  openaiModel: string,
): Promise<void> {
  await invoke('configure_ai', { provider, contextSize, openaiApiKey, openaiBaseUrl, openaiModel })
}

/** Stops the local llama-server without uninstalling. */
export async function stopAiServer(): Promise<void> {
  await invoke('stop_ai_server')
}

/** Starts the local llama-server with the given context size. */
export async function startAiServer(ctxSize: number): Promise<void> {
  await invoke('start_ai_server', { ctxSize })
}

/** Result of checking connectivity to an AI API endpoint. */
export interface AiConnectionCheckResult {
  connected: boolean
  authError: boolean
  models: string[]
  error: string | null
}

/** Checks connectivity to an AI API endpoint by calling GET {baseUrl}/models. */
export async function checkAiConnection(baseUrl: string, apiKey: string): Promise<AiConnectionCheckResult> {
  return invoke<AiConnectionCheckResult>('check_ai_connection', { baseUrl, apiKey })
}

// ============================================================================
// E2E test support
// ============================================================================

/**
 * Returns the CMDR_E2E_START_PATH env var when the automation feature is enabled.
 * Returns null when the feature is disabled or the env var is not set.
 */
export async function getE2eStartPath(): Promise<string | null> {
  try {
    return await invoke<string | null>('get_e2e_start_path')
  } catch {
    return null
  }
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
