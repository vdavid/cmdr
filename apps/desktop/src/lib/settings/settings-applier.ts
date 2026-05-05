/**
 * Settings applier - applies settings changes to the UI and Rust backend in real-time.
 * Updates CSS variables, DOM properties, and syncs backend configurations when settings change.
 */

import {
  getSetting,
  onSettingChange,
  initializeSettings,
  type UiDensity,
  type SizeColorsPalette,
  densityMappings,
} from '$lib/settings'
import { getAppLogger, setVerboseLogging } from '$lib/logging/logger'
import {
  updateFileWatcherDebounce,
  updateServiceResolveTimeout,
  setIndexingEnabled,
  setMtpEnabled,
  setDiskSpaceThreshold,
  setDirectSmbConnection,
  setFilterSafeSaveArtifacts,
  setSmbConcurrency,
  setMaxLogStorageMb,
  setErrorReportsEnabled,
  setShowVirtualGitPortal,
} from '$lib/tauri-commands'
import { addToast } from '$lib/ui/toast/toast-store.svelte'

const log = getAppLogger('settings-applier')

let initialized = false
let unsubscribe: (() => void) | undefined

/**
 * Last observed value of `advanced.maxLogStorageMb`. Used to detect `0 ↔ non-zero`
 * transitions that require an app restart (the `tauri-plugin-log` plugin has no runtime
 * reconfigure API — dropping / adding the `Folder` target only happens at build time).
 */
let lastMaxLogStorageMb: number | undefined

/**
 * Applies the size-tier color palette by setting `data-size-colors` on the
 * root element. App.css scopes alternative palettes to this attribute.
 */
function applySizeColors(palette: SizeColorsPalette): void {
  document.documentElement.dataset.sizeColors = palette
  log.debug('Applied size colors palette: {palette}', { palette })
}

/**
 * Applies UI density settings to CSS custom properties.
 */
function applyDensity(density: UiDensity): void {
  const values = densityMappings[density]
  document.documentElement.style.setProperty('--row-height', `${String(values.rowHeight)}px`)
  document.documentElement.style.setProperty('--icon-size', `${String(values.iconSize)}px`)
  document.documentElement.style.setProperty('--density-spacing', `${String(values.spacing)}px`)
  log.debug('Applied density: {density}', { density })
}

/**
 * Applies Rust backend settings that need to be synced on startup.
 */
async function applyBackendSettings(): Promise<void> {
  // File watcher debounce
  const debounceMs = getSetting('advanced.fileWatcherDebounce')
  await updateFileWatcherDebounce(debounceMs)

  // Service resolve timeout
  const resolveTimeoutMs = getSetting('advanced.serviceResolveTimeout')
  await updateServiceResolveTimeout(resolveTimeoutMs)

  // Error-report auto-dispatcher (Flow B). The backend reads the same value at startup
  // from settings.json, but pushing it here makes the dev/hot-reload path consistent
  // and survives the (rare) case where the file's been edited out-of-band.
  await setErrorReportsEnabled(getSetting('updates.errorReports'))

  // Virtual `.git` portal toggle. Same rationale as above — backend reads at startup,
  // but a re-push keeps dev/hot-reload aligned with whatever the user persisted.
  await setShowVirtualGitPortal(getSetting('fileExplorer.git.showVirtualGitPortal'))

  log.debug('Applied backend settings: debounce={debounce}ms, resolveTimeout={timeout}ms', {
    debounce: debounceMs,
    timeout: resolveTimeoutMs,
  })
}

/**
 * Applies all settings that affect the UI.
 */
function applyAllSettings(): void {
  // UI Density
  const density = getSetting('appearance.uiDensity')
  applyDensity(density)

  // Size-tier color palette
  applySizeColors(getSetting('appearance.sizeColors'))

  // Backend settings (async, fire-and-forget for startup)
  void applyBackendSettings()

  log.debug('Applied all settings')
}

/**
 * Lookup table of straightforward "push-to-backend" setting wirings. Each entry
 * is a fire-and-forget call mapping the setting value through to the Rust
 * side. Settings with branching logic (like `advanced.maxLogStorageMb`) live
 * in `handleSettingChange` directly so the table stays simple.
 */
const passthroughBackendHandlers: Partial<Record<string, (value: unknown) => void>> = {
  'developer.verboseLogging': (v) => void setVerboseLogging(v as boolean),
  'advanced.fileWatcherDebounce': (v) => void updateFileWatcherDebounce(v as number),
  'advanced.serviceResolveTimeout': (v) => void updateServiceResolveTimeout(v as number),
  'indexing.enabled': (v) => void setIndexingEnabled(v as boolean),
  'fileOperations.mtpEnabled': (v) => void setMtpEnabled(v as boolean),
  'advanced.diskSpaceChangeThreshold': (v) => void setDiskSpaceThreshold(v as number),
  'network.directSmbConnection': (v) => void setDirectSmbConnection(v as boolean),
  'advanced.filterSafeSaveArtifacts': (v) => void setFilterSafeSaveArtifacts(v as boolean),
  'network.smbConcurrency': (v) => void setSmbConcurrency(v as number),
  'updates.errorReports': (v) => void setErrorReportsEnabled(v as boolean),
  'fileExplorer.git.showVirtualGitPortal': (v) => void setShowVirtualGitPortal(v as boolean),
}

/**
 * Handles setting changes and applies them to the UI or backend.
 *
 * MCP server (`developer.mcpEnabled`, `developer.mcpPort`) is handled by
 * `McpServerSection.svelte` in the settings window directly, not here, to
 * avoid double-firing across windows. Date/time and file-size formats are
 * read on-demand so they don't need a hook.
 */
function handleSettingChange(id: string, value: unknown): void {
  log.debug('Setting changed: {id} = {value}', { id, value })

  if (id === 'appearance.uiDensity') {
    applyDensity(value as UiDensity)
    return
  }
  if (id === 'appearance.sizeColors') {
    applySizeColors(value as SizeColorsPalette)
    return
  }
  if (id === 'advanced.maxLogStorageMb') {
    applyMaxLogStorageMb(value as number)
    return
  }
  const handler = passthroughBackendHandlers[id]
  if (handler) handler(value)
}

/**
 * Pushes a new log-storage cap to the backend and toasts a "restart required"
 * notice for `0 ↔ non-zero` transitions. The rotation strategy and target list
 * are baked in at app start, so those transitions only take full effect after
 * relaunch — a non-zero ↔ non-zero change applies live.
 */
function applyMaxLogStorageMb(newValue: number): void {
  const oldValue = lastMaxLogStorageMb
  lastMaxLogStorageMb = newValue
  void setMaxLogStorageMb(newValue)
  if (oldValue !== undefined && (oldValue === 0) !== (newValue === 0)) {
    addToast('Restart Cmdr to apply the log storage change.', {
      level: 'info',
      dismissal: 'transient',
      id: 'max-log-storage-restart',
    })
  }
}

/**
 * Initialize the settings applier.
 * Call this once on app startup in the main window.
 */
export async function initSettingsApplier(): Promise<void> {
  if (initialized) {
    log.debug('Settings applier already initialized')
    return
  }

  log.debug('Initializing settings applier')

  try {
    // Ensure settings store is initialized
    await initializeSettings()

    // Seed the last-observed log-storage cap so the first change event can distinguish
    // `0 ↔ non-zero` transitions from routine cap changes.
    lastMaxLogStorageMb = getSetting('advanced.maxLogStorageMb')

    // Apply current settings
    applyAllSettings()

    // Subscribe to future changes
    unsubscribe = onSettingChange(handleSettingChange)
    initialized = true

    log.debug('Settings applier initialized successfully')
  } catch (error) {
    log.error('Failed to initialize settings applier: {error}', { error })
  }
}

/**
 * Cleanup the settings applier.
 */
export function cleanupSettingsApplier(): void {
  if (unsubscribe) {
    unsubscribe()
    unsubscribe = undefined
  }
  initialized = false
  log.debug('Settings applier cleaned up')
}
