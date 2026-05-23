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
  type DateColorsPalette,
  type ThemeMode,
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
  setNetworkEnabled,
  applyRecentSearchesMaxCount,
  applyRecentSelectionsMaxCount,
} from '$lib/tauri-commands'
import { addToast } from '$lib/ui/toast/toast-store.svelte'

const log = getAppLogger('settings-applier')

let initialized = false
let unsubscribe: (() => void) | undefined

/**
 * Last observed value of `advanced.maxLogStorageMb`. Used to detect `0 ↔ non-zero`
 * transitions that require an app restart (the `tauri-plugin-log` plugin has no runtime
 * reconfigure API: dropping / adding the `Folder` target only happens at build time).
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
 * Applies the modified-date age color palette by setting `data-date-colors`
 * on the root element. App.css scopes alternative palettes to this attribute.
 */
function applyDateColors(palette: DateColorsPalette): void {
  document.documentElement.dataset.dateColors = palette
  log.debug('Applied date colors palette: {palette}', { palette })
}

/**
 * Density currently has no CSS-side effect: `--spacing-icon-size` is owned
 * by `app.css` as `calc(16px * var(--font-scale))`, and row height /
 * density-spacing flow through `getRowHeight()` / `getDensitySpacing()`
 * getters in `reactive-settings.svelte.ts` (used for inline styles on
 * virtualized rows). The applier still re-runs `applyDensity()` on
 * `appearance.uiDensity` change so JS getters re-evaluate via the reactive
 * `uiDensity` state in `reactive-settings.svelte.ts`.
 */
function applyDensity(density: UiDensity): void {
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

  // Virtual `.git` portal toggle. Same rationale as above: backend reads at startup,
  // but a re-push keeps dev/hot-reload aligned with whatever the user persisted.
  await setShowVirtualGitPortal(getSetting('fileExplorer.git.showVirtualGitPortal'))

  log.debug('Applied backend settings: debounce={debounce}ms, resolveTimeout={timeout}ms', {
    debounce: debounceMs,
    timeout: resolveTimeoutMs,
  })
}

/**
 * Applies the persisted `theme.mode` setting via Tauri's per-app theme API.
 * Loaded dynamically so we don't pay the import cost on every startup of
 * non-Tauri contexts (tests, SSR) where the API isn't available.
 *
 * `'system'` is signaled by passing `null`, which tells Tauri to follow the
 * OS appearance.
 */
async function applyTheme(mode: ThemeMode): Promise<void> {
  try {
    const { setTheme } = await import('@tauri-apps/api/app')
    await setTheme(mode === 'system' ? null : mode)
    log.debug('Applied theme: {mode}', { mode })
  } catch (error) {
    log.error('Failed to apply theme: {error}', { error })
  }
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

  // Date age color palette
  applyDateColors(getSetting('appearance.dateColors'))

  // Theme (light / dark / system). Must run at startup or windows that open
  // before the user touches Settings will flash the wrong theme.
  void applyTheme(getSetting('theme.mode'))

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
  'network.enabled': (v) => void setNetworkEnabled(v as boolean),
  'search.recentSearches.maxCount': (v) => void applyRecentSearchesMaxCount(v as number),
  'selection.recentSelections.maxCount': (v) => void applyRecentSelectionsMaxCount(v as number),
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
  if (id === 'appearance.dateColors') {
    applyDateColors(value as DateColorsPalette)
    return
  }
  if (id === 'theme.mode') {
    void applyTheme(value as ThemeMode)
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
 * relaunch. A non-zero ↔ non-zero change applies live.
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
