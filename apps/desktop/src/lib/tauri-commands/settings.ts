// Settings commands and AI-related commands

import { Channel, invoke } from '@tauri-apps/api/core'
import { type UnlistenFn } from '@tauri-apps/api/event'
import {
  commands,
  events,
  type RestrictedWindowPersistableSetting,
  type RestrictedWindowSettings,
  type SettingsChanged,
  type SettingValue,
} from '$lib/ipc/bindings'
import { throwIpcError } from './ipc-types'

// ============================================================================
// Settings commands
// ============================================================================

/** Returns the persisted restricted-window settings (the settings synced to the pared-down MCP/E2E window). */
export function getRestrictedWindowSettings(): Promise<RestrictedWindowSettings> {
  return commands.getRestrictedWindowSettings()
}

/**
 * Pushes the FE settings-registry default map to the backend, so error-report
 * manifests don't duplicate defaults between TypeScript and Rust.
 */
export function recordSettingsDefaults(defaults: { [key in string]: SettingValue }): Promise<void> {
  return commands.recordSettingsDefaults(defaults)
}

/** Subscribes to live settings changes pushed from another window (currently just `showHiddenFiles`). */
export function onSettingsChanged(handler: (payload: SettingsChanged) => void): Promise<UnlistenFn> {
  return events.settingsChanged.listen((event) => {
    handler(event.payload)
  })
}

/**
 * Forwards a restricted window's (the viewer's) setting change to the main
 * window, whose `restricted-settings-bridge` persists it. Passthrough: the
 * caller checks the typed `Result` before logging.
 */
export function persistRestrictedWindowSetting(setting: RestrictedWindowPersistableSetting, value: boolean) {
  return commands.persistRestrictedWindowSetting(setting, value)
}

/**
 * Checks if a port is available for binding.
 * @param port - The port number to check
 * @returns True if the port is available
 */
export async function checkPortAvailable(port: number): Promise<boolean> {
  return commands.checkPortAvailable(port)
}

/**
 * Finds an available port starting from the given port.
 * Scans up to 100 ports from the start port.
 * @param startPort - The port to start scanning from
 * @returns Available port number, or null if none found
 */
export async function findAvailablePort(startPort: number): Promise<number | null> {
  return commands.findAvailablePort(startPort)
}

/**
 * Updates the file watcher debounce duration in the Rust backend.
 * This affects newly created watchers; existing watchers keep their original duration.
 * @param debounceMs - Debounce duration in milliseconds
 */
export async function updateFileWatcherDebounce(debounceMs: number): Promise<void> {
  await commands.updateFileWatcherDebounce(debounceMs)
}

/**
 * Updates the Bonjour service resolve timeout in the Rust backend.
 * This affects future service resolutions; ongoing resolutions keep their original timeout.
 * @param timeoutMs - Timeout duration in milliseconds
 */
export async function updateServiceResolveTimeout(timeoutMs: number): Promise<void> {
  await commands.updateServiceResolveTimeout(timeoutMs)
}

/**
 * Enables or disables automatic upgrade of SMB mounts to direct smb2 connections.
 * Pushed live from the settings UI whenever `network.directSmbConnection` changes.
 * @param enabled - True to enable direct SMB connections
 */
export async function setDirectSmbConnection(enabled: boolean): Promise<void> {
  await commands.setDirectSmbConnection(enabled)
}

/**
 * Toggles filtering of macOS safe-save artifacts (`.sb-*` files) in the SMB watcher.
 * Pushed live from the settings UI whenever `advanced.filterSafeSaveArtifacts` changes.
 * @param enabled - True to filter artifacts
 */
export async function setFilterSafeSaveArtifacts(enabled: boolean): Promise<void> {
  await commands.setFilterSafeSaveArtifactsCmd(enabled)
}

/**
 * Updates the SMB concurrency limit used by the batch copy engine.
 * Clamped to `1..=32` in the Rust side. Pushed live from the settings UI
 * whenever `network.smbConcurrency` changes.
 * @param value - Desired concurrency (will be clamped)
 */
export async function setSmbConcurrency(value: number): Promise<void> {
  await commands.setSmbConcurrencyCmd(value)
}

/**
 * Turns LLM call logging on or off. When on, every AI request and response is written to
 * `{app data dir}/llm-logs/` for debugging (local only, never transmitted). Pushed live from
 * the settings UI whenever `advanced.logLlmCalls` changes; runtime-toggleable, no restart.
 * @param enabled - True to log LLM calls
 */
export async function setLogLlmCalls(enabled: boolean): Promise<void> {
  await commands.setLogLlmCalls(enabled)
}

/**
 * Updates the in-RAM log-storage cap and eagerly prunes excess archived log files.
 *
 * `tauri-plugin-log` has no runtime reconfigure API, so the rotation strategy the plugin
 * was built with survives until app restart. Lowering the cap still takes immediate visual
 * effect through the eager prune; `0 ↔ non-zero` transitions and raising the cap beyond
 * the previously baked-in value require a restart (the settings UI toasts a notice).
 *
 * @param value - Cap in MB. `0` disables log storage entirely.
 */
export async function setMaxLogStorageMb(value: number): Promise<void> {
  const res = await commands.setMaxLogStorageMb(value)
  if (res.status === 'error') throwIpcError(res.error)
}

/**
 * Enables or disables the Flow B error-report auto-dispatcher.
 *
 * Pushed live from the settings UI whenever `updates.errorReports` changes. Default off
 * (opt-in by design: Flow B sends a small log snippet on user-visible errors without
 * per-event consent). Flipping this off doesn't tear down an in-flight debounce window;
 * the next user-visible error after disable simply doesn't enqueue.
 *
 * @param value - True to enable auto-send.
 */
export async function setErrorReportsEnabled(value: boolean): Promise<void> {
  await commands.setErrorReportsEnabled(value)
}

/**
 * Enables or disables the virtual `.git` portal. When off, navigating into `.git` shows the raw
 * on-disk contents instead of the branches/tags/commits virtual folders.
 *
 * Pushed live from the settings UI whenever `fileExplorer.git.showVirtualGitPortal` changes.
 *
 * @param enabled - True to keep the portal active, false to fall back to raw `.git` listings.
 */
export async function setShowVirtualGitPortal(enabled: boolean): Promise<void> {
  await commands.setShowVirtualGitPortal(enabled)
}

// ============================================================================
// MCP server commands
// ============================================================================

/**
 * Result of an MCP server lifecycle transition. Mirrors the Rust `McpServerOutcome` enum
 * (`apps/desktop/src-tauri/src/mcp/server.rs`); the wire shape is pinned by the
 * `mcp_server_outcome_json_shape` Rust test. Discriminated on `kind` so callers branch on a
 * typed tag, never a message string (AGENTS.md § "No string-matching state classification").
 * Hand-maintained because `set_mcp_*` are generic over `R: Runtime` and excluded from the
 * specta bindings (see `lib/ipc/CLAUDE.md` § Excluded commands).
 */
export type McpServerOutcome =
  | { kind: 'running'; port: number }
  | { kind: 'stopped' }
  | { kind: 'portInUse'; requested: number }

/**
 * Starts or stops the MCP server. Pass the current port so it binds correctly on enable.
 * On enable, a busy port yields `{ kind: 'portInUse' }` and leaves the server as it was.
 */
export async function setMcpEnabled(enabled: boolean, port: number): Promise<McpServerOutcome> {
  // eslint-disable-next-line cmdr/no-raw-tauri-invoke -- generic command (<R: Runtime>), excluded from specta bindings
  return invoke<McpServerOutcome>('set_mcp_enabled', { enabled, port })
}

/**
 * Restarts the running MCP server on a new port (zero-downtime). No-op (`{ kind: 'stopped' }`)
 * if the server isn't running; a busy port yields `{ kind: 'portInUse' }` and keeps the
 * server on its current port.
 */
export async function setMcpPort(port: number): Promise<McpServerOutcome> {
  // eslint-disable-next-line cmdr/no-raw-tauri-invoke -- generic command (<R: Runtime>), excluded from specta bindings
  return invoke<McpServerOutcome>('set_mcp_port', { port })
}

/** Returns whether the MCP server is currently running. */
export async function getMcpRunning(): Promise<boolean> {
  return commands.getMcpRunning()
}

/** Returns the port the MCP server is actually listening on, or null if not running. */
export async function getMcpPort(): Promise<number | null> {
  return commands.getMcpPort()
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
  const res = await commands.setIndexingEnabled(enabled)
  if (res.status === 'error') throwIpcError(res.error)
}

/**
 * Live-applies the master "Index image contents" toggle (`mediaIndex.enabled`) to the
 * backend `media_index` enrichment scheduler. Enabling clears any prior memory-watchdog
 * stop so enrichment resumes on the next scan completion; disabling makes the scheduler
 * no-op. No restart needed.
 */
export async function setImageIndexEnabled(enabled: boolean): Promise<void> {
  await commands.setImageIndexEnabled(enabled)
}

/**
 * Starts the drive indexer after the user makes their Full Disk Access decision.
 *
 * At launch, the backend skips auto-starting the indexer when the FDA choice is
 * `notAskedYet` and the OS reports FDA as not granted. Otherwise, recursively
 * scanning from `/` triggers macOS native permission popups (iCloud, Photos, etc.)
 * that stack on top of the in-app FDA modal.
 *
 * Call this after the user clicks "Deny" so indexing starts within the same
 * session. The "Allow" path needs no call: the user restarts the app, and the
 * launch-time gate passes via the OS check.
 *
 * Idempotent: a no-op when indexing is already running or initializing.
 */
export async function startIndexingAfterFdaDecision(): Promise<void> {
  const res = await commands.startIndexingAfterFdaDecision()
  if (res.status === 'error') throwIpcError(res.error)
}

/** Index directory stats returned by the batch lookup. */
export interface DirStats {
  path: string
  recursiveSize: number
  recursivePhysicalSize: number
  recursiveFileCount: number
  recursiveDirCount: number
  /** `true` if the subtree contains any symlinks (whose content is omitted from the recursive size). */
  recursiveHasSymlinks: boolean
  /**
   * `true` while the indexer still has unprocessed writes affecting this
   * directory or a descendant (a big delete/copy in flight), so its recursive
   * size is mid-update. Drives the per-row "size updating" hourglass. Optional
   * so test fixtures and pre-field callers stay valid; the backend always sets
   * it, and consumers treat absent as `false`.
   */
  recursiveSizePending?: boolean
  /**
   * `true` when `recursiveSize` is an exact total, `false` when it's a lower
   * bound (some subtree was never listed). Derived backend-side from the
   * subtree's coverage; the UI renders an exact size when `true`, a `≥` lower
   * bound (or `—` when size is 0) when `false`. Optional so test fixtures and
   * pre-field callers stay valid; absent is treated as `true` (exact).
   */
  recursiveSizeComplete?: boolean
  /**
   * `true` when the exact `recursiveSize` is accurate-but-stale (computed at an
   * older volume epoch than the current one). Only meaningful when
   * `recursiveSizeComplete` is `true`; drives the muted "stale" treatment.
   * Optional; absent is treated as `false` (fresh).
   */
  recursiveSizeStale?: boolean
}

/**
 * Fetches index stats for a batch of directory paths.
 * Returns one entry per input path (null if the path has no index data yet).
 */
export async function getDirStatsBatch(paths: string[]): Promise<(DirStats | null)[]> {
  const res = await commands.getDirStatsBatch(paths)
  if (res.status === 'error') throwIpcError(res.error)
  return res.data
}

// ============================================================================
// System memory
// ============================================================================

/** System RAM breakdown in bytes. Categories are non-overlapping and sum to `totalBytes`. */
export interface SystemMemoryInfo {
  totalBytes: number
  /** Wired + compressor-occupied memory (kernel, drivers; can't be freed). */
  wiredBytes: number
  /** App memory: active + inactive - purgeable (process memory the user can free by quitting apps). */
  appBytes: number
  /** Free: free + purgeable + speculative (available for new allocations). */
  freeBytes: number
}

/** Returns system RAM breakdown for the RAM gauge. */
export async function getSystemMemoryInfo(): Promise<SystemMemoryInfo> {
  return commands.getSystemMemoryInfo()
}

// ============================================================================
// AI commands
// ============================================================================

export type AiStatus = 'unavailable' | 'offer' | 'downloading' | 'installing' | 'available'

// The model-download progress payload is now generated by `tauri-specta`
// (the `ai-download-progress` typed event). Keep the `AiDownloadProgress`
// name as a re-export alias so existing consumers' imports stay stable.
export type { DownloadProgress as AiDownloadProgress } from '$lib/ipc/bindings'

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
  return commands.getAiStatus()
}

/** Returns information about the current AI model. */
export async function getAiModelInfo(): Promise<AiModelInfo> {
  return commands.getAiModelInfo()
}

/** Starts downloading the AI model and inference runtime. */
export async function startAiDownload(): Promise<void> {
  // eslint-disable-next-line cmdr/no-raw-tauri-invoke -- generic <R: Runtime> command, excluded from specta bindings (see ipc_collectors.rs)
  await invoke('start_ai_download')
}

/** Cancels an in-progress AI download. */
export async function cancelAiDownload(): Promise<void> {
  await commands.cancelAiDownload()
}

/** Uninstalls the AI model and binary, resets state. */
export async function uninstallAi(): Promise<void> {
  await commands.uninstallAi()
}

/** Returns the full runtime status of the AI subsystem. */
export async function getAiRuntimeStatus(): Promise<AiRuntimeStatus> {
  return commands.getAiRuntimeStatus()
}

/** Pushes AI config to the backend. Triggers server start if provider is local + model installed. */
export async function configureAi(
  provider: string,
  contextSize: number,
  cloudApiKey: string,
  cloudBaseUrl: string,
  cloudModel: string,
  cloudRequiresApiKey: boolean,
): Promise<void> {
  // eslint-disable-next-line cmdr/no-raw-tauri-invoke -- generic <R: Runtime> command, excluded from specta bindings (see ipc_collectors.rs)
  await invoke('configure_ai', { provider, contextSize, cloudApiKey, cloudBaseUrl, cloudModel, cloudRequiresApiKey })
}

/** Stops the local llama-server without uninstalling. */
export async function stopAiServer(): Promise<void> {
  await commands.stopAiServer()
}

/** Starts the local llama-server with the given context size. */
export async function startAiServer(ctxSize: number): Promise<void> {
  // eslint-disable-next-line cmdr/no-raw-tauri-invoke -- generic <R: Runtime> command, excluded from specta bindings (see ipc_collectors.rs)
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
  return commands.checkAiConnection(baseUrl, apiKey)
}

// ============================================================================
// AI API key storage (OS secret store, not settings.json)
// ============================================================================

/** Stores the API key for a cloud provider in the OS secret store (macOS Keychain etc.). */
export async function saveAiApiKey(providerId: string, apiKey: string): Promise<void> {
  const res = await commands.saveAiApiKey(providerId, apiKey)
  if (res.status === 'error') throwIpcError(res.error)
}

/** Returns the stored API key for a cloud provider, or '' if none is stored. */
export async function getAiApiKey(providerId: string): Promise<string> {
  const res = await commands.getAiApiKey(providerId)
  if (res.status === 'error') throwIpcError(res.error)
  return res.data
}

/** Removes the stored API key for a cloud provider. Idempotent. */
export async function deleteAiApiKey(providerId: string): Promise<void> {
  const res = await commands.deleteAiApiKey(providerId)
  if (res.status === 'error') throwIpcError(res.error)
}

/** Returns true if an API key is stored for the provider. */
export async function hasAiApiKey(providerId: string): Promise<boolean> {
  return commands.hasAiApiKey(providerId)
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
    return await commands.getE2eStartPath()
  } catch {
    return null
  }
}

/**
 * Returns true when the running binary is under an E2E harness
 * (`CMDR_E2E_MODE=1`). Returns false when not, or when the backend isn't
 * reachable (non-Tauri context, like a Vitest run).
 */
export async function isE2eMode(): Promise<boolean> {
  try {
    return await commands.isE2eMode()
  } catch {
    return false
  }
}

/**
 * Returns true when `CMDR_FORCE_ONBOARDING` is set in the running binary's environment.
 * The frontend uses this to bypass the `isOnboarded` gate and surface the onboarding wizard
 * regardless of persisted state. Returns false when not set, or when the backend isn't
 * reachable (non-Tauri context, like a Vitest run).
 */
export async function isForceOnboarding(): Promise<boolean> {
  try {
    return await commands.isForceOnboarding()
  } catch {
    return false
  }
}

/** Gets AI-generated folder name suggestions for the current directory. */
export async function getFolderSuggestions(
  listingId: string,
  currentPath: string,
  includeHidden: boolean,
): Promise<string[]> {
  try {
    const res = await commands.getFolderSuggestions(listingId, currentPath, includeHidden)
    if (res.status === 'error') return []
    return res.data
  } catch {
    return []
  }
}

/** Wire-format event for streaming folder suggestions. Mirrors the Rust enum. */
export type SuggestionStreamEvent =
  | { type: 'suggestion'; name: string }
  | { type: 'done' }
  | { type: 'cancelled' }
  | { type: 'failed' }

/** Handle returned by `streamFolderSuggestions`. */
export interface FolderSuggestionsStream {
  /** Resolves when the backend command returns (after Done/Cancelled/Failed has been delivered). */
  promise: Promise<void>
  /** Cancels the in-flight stream. Idempotent; safe to call after natural completion. */
  cancel: () => Promise<void>
}

/**
 * Streams folder name suggestions, calling `onEvent` for each event from the backend.
 *
 * The backend always resolves the IPC promise to `void`; all signaling (suggestions,
 * completion, cancellation, failure) goes through the channel. Cancel via the returned
 * handle, not by ignoring the promise: Tauri 2's `Channel::send` is fire-and-forget,
 * so the backend cannot detect frontend abandonment without the explicit cancel command.
 */
export function streamFolderSuggestions(
  listingId: string,
  currentPath: string,
  includeHidden: boolean,
  onEvent: (event: SuggestionStreamEvent) => void,
): FolderSuggestionsStream {
  const requestId = crypto.randomUUID()
  const channel = new Channel<SuggestionStreamEvent>()
  channel.onmessage = onEvent
  // eslint-disable-next-line cmdr/no-raw-tauri-invoke -- streaming Channel<T> not specta-friendly yet; tracked for follow-up
  const promise = invoke('stream_folder_suggestions', {
    requestId,
    listingId,
    currentPath,
    includeHidden,
    onEvent: channel,
  }).then(
    () => undefined,
    () => undefined, // Tauri command is contracted to return Ok(()), but webview teardown can reject; swallow.
  )
  const cancel = async (): Promise<void> => {
    try {
      // eslint-disable-next-line cmdr/no-raw-tauri-invoke -- streaming Channel<T> not specta-friendly yet; tracked for follow-up
      await invoke('cancel_folder_suggestions', { requestId })
    } catch {
      // Idempotent: entry may already be gone.
    }
  }
  return { promise, cancel }
}
