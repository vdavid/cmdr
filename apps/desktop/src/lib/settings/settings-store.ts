/**
 * Settings persistence layer - stores and loads settings from disk.
 */

import { load, type Store } from '@tauri-apps/plugin-store'
import { emit, listen, type UnlistenFn } from '@tauri-apps/api/event'
import type { SettingId, SettingsValues } from './types'
import { SettingValidationError } from './types'
import { getDefaultValue, settingsRegistry, validateSettingValue } from './settings-registry'
import { getAppLogger } from '$lib/logger'

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

const STORE_NAME = 'settings-v2.json'
const SCHEMA_VERSION = 1

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
        log.debug('Creating new store instance for {storeName}', { storeName: STORE_NAME })
        // Build defaults from registry
        const defaults: Record<string, unknown> = {}
        for (const def of settingsRegistry) {
            defaults[def.id] = def.default
        }
        log.debug('Loading store with {count} default settings', { count: Object.keys(defaults).length })
        storeInstance = await load(STORE_NAME, { defaults, autoSave: false })
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

    log.info('Starting settings initialization')

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
        log.debug('Loading {count} settings from store into cache', { count: settingsRegistry.length })
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

        log.info('Settings loaded: {loaded} from store, {defaults} using defaults', {
            loaded: loadedCount,
            defaults: defaultCount,
        })

        // Listen for cross-window setting changes
        await setupCrossWindowListener()

        initialized = true
        log.info('Settings initialization complete')
    } catch (error) {
        log.error('Failed to initialize settings: {error}', { error })
        throw error
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
    // Currently no migrations needed, just set version
    if (fromVersion < 1) {
        // Future migrations would go here
        // Example: rename old keys, convert formats, etc.
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
        log.warn('Settings not initialized, returning default for {id}', { id })
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
 */
export function setSetting<K extends SettingId>(id: K, value: SettingsValues[K]): void {
    log.debug('setSetting({id}, {value})', { id, value })

    // Validate the value
    validateSettingValue(id, value)

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
 * Reset all settings to their default values.
 */
export async function resetAllSettings(): Promise<void> {
    for (const def of settingsRegistry) {
        settingsCache[def.id] = def.default
    }

    // Clear the store
    const store = await getStore()
    await store.clear()
    await store.set('_schemaVersion', SCHEMA_VERSION)
    await store.save()

    // Notify all listeners
    for (const def of settingsRegistry) {
        notifyListeners(def.id as SettingId, def.default as SettingsValues[SettingId])
    }
}

/**
 * Check if a setting has been modified from its default value.
 */
export function isModified(id: SettingId): boolean {
    const current = getSetting(id)
    const defaultVal = getDefaultValue(id)
    return current !== defaultVal
}

/**
 * Get all setting values as a plain object.
 */
export function getAllSettings(): Partial<SettingsValues> {
    return { ...settingsCache } as Partial<SettingsValues>
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
