/**
 * Settings persistence layer - stores and loads settings from disk.
 */

import { load, type Store } from '@tauri-apps/plugin-store'
import { emit, listen, type UnlistenFn } from '@tauri-apps/api/event'
import type { SettingId, SettingsValues } from './types'
import { SettingValidationError } from './types'
import { getDefaultValue, settingsRegistry, validateSettingValue } from './settings-registry'
import { resolveSettingsStorePath } from './settings-store-path'
import { getAppLogger } from '$lib/logging/logger'
import { pluralize } from '$lib/utils/pluralize'
import { commands } from '$lib/ipc/bindings'
import type { SettingValue } from '$lib/ipc/bindings'

const log = getAppLogger('settings')

// Event name for cross-window setting changes
const SETTING_CHANGED_EVENT = 'settings:changed'

interface SettingChangedPayload {
  id: string
  value: unknown
}

// ============================================================================
// Store Configuration
// ============================================================================

const SCHEMA_VERSION = 2

let storeInstance: Store | null = null
let saveTimeout: ReturnType<typeof setTimeout> | null = null
const SAVE_DEBOUNCE_MS = 500

// In-memory cache of settings for synchronous access
// Using Record to allow any setting ID assignment
const settingsCache: Record<string, unknown> = {}
let initialized = false
let crossWindowUnlisten: UnlistenFn | null = null

// ============================================================================
// Initialization
// ============================================================================

async function getStore(): Promise<Store> {
  if (!storeInstance) {
    // Resolve the store path so isolated instances (dev, per-worktree dev, E2E)
    // don't read the real production `settings.json`. See `settings-store-path.ts`.
    const storePath = await resolveSettingsStorePath((e) =>
      log.warn('Could not resolve isolated settings path, using default: {error}', { error: String(e) }),
    )
    log.debug('Creating new store instance for {storeName}', { storeName: storePath })
    // Build defaults from registry
    const defaults: Record<string, unknown> = {}
    for (const def of settingsRegistry) {
      defaults[def.id] = def.default
    }
    // allowed-pluralize-noun: settingsRegistry is a fixed const with many entries.
    log.debug('Loading store with {count} default settings', { count: Object.keys(defaults).length })
    storeInstance = await load(storePath, { defaults, autoSave: false })
    log.debug('Store instance created successfully')
  }
  return storeInstance
}

/**
 * Initialize the settings store. Must be called before using getSetting/setSetting.
 */
export async function initializeSettings(): Promise<void> {
  log.debug('initializeSettings() called, initialized={initialized}', { initialized })

  if (initialized) {
    log.debug('Settings already initialized, returning early')
    return
  }

  log.debug('Starting settings initialization')

  try {
    const store = await getStore()
    log.debug('Got store instance')

    // Check schema version and migrate if needed
    const version = await store.get<number>('_schemaVersion')
    log.debug('Current schema version: {version}, expected: {expected}', { version, expected: SCHEMA_VERSION })

    if (version !== SCHEMA_VERSION) {
      log.info('Schema version mismatch, migrating from {from} to {to}', {
        from: version ?? 0,
        to: SCHEMA_VERSION,
      })
      await migrateSettings(store, version ?? 0)
    }

    // Load all settings into cache
    log.debug('Loading {count} {settingsNoun} from store into cache', {
      count: settingsRegistry.length,
      settingsNoun: pluralize(settingsRegistry.length, 'setting'),
    })
    let loadedCount = 0
    let defaultCount = 0

    for (const def of settingsRegistry) {
      const stored = await store.get<unknown>(def.id)
      if (stored !== null && stored !== undefined) {
        try {
          validateSettingValue(def.id, stored)
          settingsCache[def.id] = stored
          loadedCount++
        } catch {
          // Invalid stored value, will use default
          log.warn('Invalid stored value for {id}, using default', { id: def.id })
          defaultCount++
        }
      } else {
        defaultCount++
      }
    }

    log.debug('Settings loaded: {loaded} from store, {defaults} using defaults', {
      loaded: loadedCount,
      defaults: defaultCount,
    })

    // Listen for cross-window setting changes
    await setupCrossWindowListener()

    // Push the registry's default map to the backend so error-report manifests can
    // resolve `null`-shaped settings against the live registry instead of duplicating
    // defaults in Rust. Best-effort: a failure here only affects manifest resolution,
    // which has hardcoded fallbacks. Don't block init on it.
    void pushSettingsDefaultsToBackend()

    initialized = true
    log.debug('Settings initialization complete')
  } catch (error) {
    log.error('Failed to initialize settings: {error}', { error })
    throw error
  }
}

/**
 * Send the registry's default values to the backend's `record_settings_defaults`
 * command. The backend uses this map in `ResolvedSettings::from_settings` to keep
 * manifest defaults in sync with the registry. Silently swallows errors; the Rust
 * side has hardcoded fallbacks for every field it reads.
 */
async function pushSettingsDefaultsToBackend(): Promise<void> {
  try {
    const defaults: Record<string, SettingValue> = {}
    for (const def of settingsRegistry) {
      const value = def.default
      // SettingValue is untagged on the wire: boolean | number | string.
      // Values of other types (arrays, objects) are silently skipped; the Rust
      // side has hardcoded fallbacks and the lookup_* helpers only support these three.
      if (typeof value === 'boolean' || typeof value === 'number' || typeof value === 'string') {
        defaults[def.id] = value
      }
    }
    await commands.recordSettingsDefaults(defaults)
  } catch (err) {
    log.warn('Failed to push settings defaults to backend: {err}', { err })
  }
}

/**
 * Set up listener for setting changes from other windows.
 */
async function setupCrossWindowListener(): Promise<void> {
  if (crossWindowUnlisten) {
    return // Already listening
  }

  log.debug('Setting up cross-window settings listener')

  crossWindowUnlisten = await listen<SettingChangedPayload>(SETTING_CHANGED_EVENT, (event) => {
    const { id, value } = event.payload
    log.debug('Received cross-window setting change: {id}', { id })

    // Update our cache without re-emitting (to avoid loops)
    settingsCache[id] = value

    // Notify local listeners
    notifyListeners(id as SettingId, value as SettingsValues[SettingId])
  })

  log.debug('Cross-window settings listener ready')
}

/**
 * Migrate settings from older schema versions.
 */
async function migrateSettings(store: Store, fromVersion: number): Promise<void> {
  if (fromVersion < 1) {
    // No-op placeholder for the original schema.
  }

  if (fromVersion < 2) {
    // `appearance.dateColors` renamed its "no coloring" value from `off` to
    // `none` to match `appearance.sizeColors`.
    const dateColors = await store.get<string>('appearance.dateColors')
    if (dateColors === 'off') {
      await store.set('appearance.dateColors', 'none')
    }
  }

  await store.set('_schemaVersion', SCHEMA_VERSION)
  await store.save()
}

// ============================================================================
// Core API
// ============================================================================

/**
 * Get a setting value. Returns the default if not set.
 * Must call initializeSettings() first.
 */
export function getSetting<K extends SettingId>(id: K): SettingsValues[K] {
  if (!initialized) {
    log.debug('Settings not initialized, returning default for {id}', { id })
    return getDefaultValue(id)
  }

  const cached = settingsCache[id]
  if (cached !== undefined) {
    return cached as SettingsValues[K]
  }

  return getDefaultValue(id)
}

/**
 * Set a setting value. Validates against constraints before storing.
 * Throws SettingValidationError if invalid.
 *
 * Idempotent: when `value` strictly equals the currently-cached value, the
 * call returns after validation without scheduling a save, notifying
 * listeners, or emitting the cross-window event. This avoids redundant work
 * for the (common) case of writing the same value twice — e.g. a settings UI
 * onChange that fires for any click, or test setup/teardown that resets a
 * setting back to its already-current value. The cascade for `network.enabled`
 * alone fires 14 `network-host-lost` events through the FE event loop on a
 * real toggle, so the redundant call used to be heavy enough to occasionally
 * starve a concurrent `mcp_round_trip` waiting on `mcp-response`.
 *
 * `===` is the right comparator here: every registered setting is a primitive
 * (`boolean | number | string`) or a pinned-shape JSON object that callers
 * replace by reference when they mutate, so same-reference always means
 * no-change. If you add a setting that requires deep-equality, narrow the
 * comparison here instead of dropping the guard.
 */
export function setSetting<K extends SettingId>(id: K, value: SettingsValues[K]): void {
  log.debug('setSetting({id}, {value})', { id, value })

  // Validate the value
  validateSettingValue(id, value)

  // Idempotency: skip the cascade when nothing actually changed.
  if (settingsCache[id] === value) {
    log.debug('setSetting({id}): unchanged, skipping notify+save+emit', { id })
    return
  }

  // Update cache immediately for synchronous access
  settingsCache[id] = value

  // Debounced save to disk
  scheduleSave()

  // Notify local listeners
  notifyListeners(id, value)

  // Emit cross-window event so other windows get the update
  void emit(SETTING_CHANGED_EVENT, { id, value } satisfies SettingChangedPayload)
  log.debug('Emitted cross-window setting change event for {id}', { id })
}

/**
 * Reset a setting to its default value.
 */
export function resetSetting(id: SettingId): void {
  const defaultValue = getDefaultValue(id)
  setSetting(id, defaultValue)
}

/**
 * Check if a setting has been modified from its default value.
 */
export function isModified(id: SettingId): boolean {
  const current = getSetting(id)
  const defaultVal = getDefaultValue(id)
  return current !== defaultVal
}

// ============================================================================
// Persistence
// ============================================================================

function scheduleSave(): void {
  if (saveTimeout) {
    clearTimeout(saveTimeout)
  }

  saveTimeout = setTimeout(() => {
    void saveToStore().finally(() => {
      saveTimeout = null
    })
  }, SAVE_DEBOUNCE_MS)
}

async function saveToStore(): Promise<void> {
  log.debug('saveToStore() called')

  try {
    const store = await getStore()

    // Only save non-default values to keep the file small
    let savedCount = 0
    let removedCount = 0

    for (const def of settingsRegistry) {
      const id = def.id as SettingId
      const value = settingsCache[id]
      const defaultValue = def.default

      if (value !== undefined && value !== defaultValue) {
        await store.set(id, value)
        savedCount++
      } else {
        // Remove from store if it's the default
        await store.delete(id)
        removedCount++
      }
    }

    await store.set('_schemaVersion', SCHEMA_VERSION)
    await store.save()
    log.info('Settings saved: {saved} non-default values, {removed} reset to default', {
      saved: savedCount,
      removed: removedCount,
    })
  } catch (error) {
    log.error('Failed to save settings: {error}', { error })
    // Retry once
    try {
      log.debug('Retrying save...')
      const store = await getStore()
      await store.save()
      log.info('Retry save succeeded')
    } catch (retryError) {
      log.error('Retry save failed: {error}', { error: retryError })
      // Could show a toast here in the future
    }
  }
}

// ============================================================================
// Change Listeners
// ============================================================================

type SettingChangeListener<K extends SettingId = SettingId> = (id: K, value: SettingsValues[K]) => void

const listeners = new Set<SettingChangeListener>()
const specificListeners = new Map<SettingId, Set<SettingChangeListener>>()

/**
 * Subscribe to all setting changes.
 */
export function onSettingChange(listener: SettingChangeListener): () => void {
  listeners.add(listener)
  return () => listeners.delete(listener)
}

/**
 * Subscribe to changes for a specific setting.
 */
export function onSpecificSettingChange<K extends SettingId>(
  id: K,
  listener: (id: K, value: SettingsValues[K]) => void,
): () => void {
  let set = specificListeners.get(id)
  if (!set) {
    set = new Set()
    specificListeners.set(id, set)
  }
  set.add(listener as SettingChangeListener)
  return () => set.delete(listener as SettingChangeListener)
}

function notifyListeners<K extends SettingId>(id: K, value: SettingsValues[K]): void {
  // Notify global listeners
  for (const listener of listeners) {
    try {
      listener(id, value)
    } catch (error) {
      log.error('Setting change listener error: {error}', { error })
    }
  }

  // Notify specific listeners
  const specific = specificListeners.get(id)
  if (specific) {
    for (const listener of specific) {
      try {
        listener(id, value)
      } catch (error) {
        log.error('Setting change listener error: {error}', { error })
      }
    }
  }
}

// ============================================================================
// Utility: Force Save (for testing)
// ============================================================================

/**
 * Force an immediate save to disk. Used for testing.
 */
export async function forceSave(): Promise<void> {
  if (saveTimeout) {
    clearTimeout(saveTimeout)
    saveTimeout = null
  }
  await saveToStore()
}

// ============================================================================
// Export validation error for external use
// ============================================================================

export { SettingValidationError }
