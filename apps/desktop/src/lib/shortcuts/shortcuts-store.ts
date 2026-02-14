/**
 * Shortcuts persistence layer - stores custom keyboard shortcuts.
 */

import { load, type Store } from '@tauri-apps/plugin-store'
import { invoke } from '@tauri-apps/api/core'
import { commands } from '$lib/commands/command-registry'
import { getAppLogger } from '$lib/logger'

const log = getAppLogger('shortcuts')

// ============================================================================
// Store configuration
// ============================================================================

const STORE_NAME = 'shortcuts.json'
const SCHEMA_VERSION = 1

let storeInstance: Store | null = null

// In-memory cache of custom shortcuts
const customShortcuts = new Map<string, string[]>()
let initialized = false

// ============================================================================
// Initialization
// ============================================================================

async function getStore(): Promise<Store> {
    if (!storeInstance) {
        storeInstance = await load(STORE_NAME, {
            defaults: { _schemaVersion: SCHEMA_VERSION },
            autoSave: false,
        })
    }
    return storeInstance
}

/**
 * Initialize the shortcuts store. Must be called before using other functions.
 */
export async function initializeShortcuts(): Promise<void> {
    if (initialized) {
        log.debug('Shortcuts already initialized, skipping')
        return
    }

    log.debug('Initializing shortcuts store')

    const store = await getStore()

    // Check schema version and migrate if needed
    const version = await store.get<number>('_schemaVersion')
    if (version !== undefined && version !== SCHEMA_VERSION) {
        log.info('Migrating shortcuts from version {version}', { version })
        await migrateShortcuts(store, version)
    }

    // Clear in-memory cache and load fresh from store
    customShortcuts.clear()

    // Load custom shortcuts from top-level keys (format: shortcut:commandId)
    // This is similar to how settings-store stores values
    const keys = await store.keys()
    const shortcutKeys = keys.filter((k) => k.startsWith('shortcut:'))

    for (const key of shortcutKeys) {
        const commandId = key.replace('shortcut:', '')
        const shortcuts = await store.get<string[]>(key)
        if (shortcuts && shortcuts.length > 0) {
            customShortcuts.set(commandId, shortcuts)
        }
    }

    if (customShortcuts.size > 0) {
        log.info('Loaded {count} custom shortcuts: {ids}', {
            count: customShortcuts.size,
            ids: [...customShortcuts.keys()].join(', '),
        })
    } else {
        log.debug('No custom shortcuts found in store')
    }

    initialized = true

    // Sync menu accelerators with loaded custom shortcuts
    await syncMenuAccelerators()
}

/**
 * Sync all custom shortcuts to menu accelerators.
 * Called at initialization to ensure menu reflects persisted shortcuts.
 */
async function syncMenuAccelerators(): Promise<void> {
    // Commands that have corresponding menu items
    const menuCommands = ['view.fullMode', 'view.briefMode']

    for (const commandId of menuCommands) {
        // Only update if there's a custom shortcut
        if (customShortcuts.has(commandId)) {
            log.debug('Syncing menu accelerator for {commandId}', { commandId })
            await updateMenuAccelerator(commandId)
        }
    }
}

/**
 * Force save immediately. Should be called before window/page unload.
 * Note: Since shortcuts now save immediately (no debouncing), this is a no-op
 * but kept for API compatibility.
 */
export function flushPendingSave(): Promise<void> {
    // Shortcuts save immediately now, so nothing to flush
    log.debug('flushPendingSave called (no-op since saves are immediate)')
    return Promise.resolve()
}

/**
 * Migrate shortcuts from older schema versions.
 */
async function migrateShortcuts(store: Store, fromVersion: number): Promise<void> {
    // Currently no migrations needed
    if (fromVersion < 1) {
        // Future migrations would go here
    }

    await store.set('_schemaVersion', SCHEMA_VERSION)
    await store.save()
}

// ============================================================================
// Core API
// ============================================================================

/**
 * Get all custom shortcuts as a record.
 */
export function getCustomShortcuts(): Record<string, string[]> {
    return Object.fromEntries(customShortcuts)
}

/**
 * Get effective shortcuts for a command (custom if set, otherwise defaults).
 * Always returns a copy to prevent mutation of the original arrays.
 */
export function getEffectiveShortcuts(commandId: string): string[] {
    const custom = customShortcuts.get(commandId)
    if (custom) {
        return [...custom]
    }

    const command = commands.find((c) => c.id === commandId)
    return [...(command?.shortcuts ?? [])]
}

/**
 * Get default shortcuts for a command from the registry.
 * Always returns a copy to prevent mutation of the original arrays.
 */
export function getDefaultShortcuts(commandId: string): string[] {
    const command = commands.find((c) => c.id === commandId)
    return [...(command?.shortcuts ?? [])]
}

/**
 * Check if a command's shortcuts have been modified from defaults.
 */
export function isShortcutModified(commandId: string): boolean {
    return customShortcuts.has(commandId)
}

/**
 * Check if shortcuts array matches defaults and clean up if so.
 */
function cleanupIfMatchesDefaults(commandId: string): void {
    const current = customShortcuts.get(commandId)
    if (!current) return

    const defaults = getDefaultShortcuts(commandId)

    // Check if they match (same length and same values in same order)
    const matches = current.length === defaults.length && current.every((shortcut, i) => shortcut === defaults[i])

    if (matches) {
        customShortcuts.delete(commandId)
    }
}

/**
 * Set a specific shortcut for a command at an index.
 */
export function setShortcut(commandId: string, index: number, shortcut: string): void {
    log.debug('setShortcut({commandId}, {index}, {shortcut})', { commandId, index, shortcut })
    const current = getEffectiveShortcuts(commandId)

    if (index >= 0 && index < current.length) {
        current[index] = shortcut
    } else if (index === current.length) {
        current.push(shortcut)
    }

    customShortcuts.set(commandId, current)
    cleanupIfMatchesDefaults(commandId)
    // Save immediately (no debounce) since shortcut changes are rare user actions
    // and we need persistence before the Settings window might close
    void saveToStore()
    notifyListeners(commandId)
}

/**
 * Add a new shortcut to a command.
 */
export function addShortcut(commandId: string, shortcut: string): void {
    const current = getEffectiveShortcuts(commandId)
    current.push(shortcut)
    customShortcuts.set(commandId, current)
    cleanupIfMatchesDefaults(commandId)
    // Save immediately for reliable persistence
    void saveToStore()
    notifyListeners(commandId)
}

/**
 * Remove a shortcut from a command at an index.
 */
export function removeShortcut(commandId: string, index: number): void {
    const current = getEffectiveShortcuts(commandId)

    if (index >= 0 && index < current.length) {
        current.splice(index, 1)
        customShortcuts.set(commandId, current)
        cleanupIfMatchesDefaults(commandId)
        // Save immediately for reliable persistence
        void saveToStore()
        notifyListeners(commandId)
    }
}

/**
 * Reset a command's shortcuts to defaults.
 */
export function resetShortcut(commandId: string): void {
    if (customShortcuts.has(commandId)) {
        customShortcuts.delete(commandId)
        // Save immediately for reliable persistence
        void saveToStore()
        notifyListeners(commandId)
    }
}

/**
 * Reset all shortcuts to defaults.
 */
export async function resetAllShortcuts(): Promise<void> {
    const modifiedIds = [...customShortcuts.keys()]

    // Clear all customizations
    customShortcuts.clear()

    // Delete all shortcut keys from store
    const store = await getStore()
    const keys = await store.keys()
    for (const key of keys) {
        if (key.startsWith('shortcut:')) {
            await store.delete(key)
        }
    }
    await store.set('_schemaVersion', SCHEMA_VERSION)
    await store.save()

    // Notify listeners for all modified commands
    for (const id of modifiedIds) {
        notifyListeners(id)
    }
}

// ============================================================================
// Persistence
// ============================================================================

async function saveToStore(): Promise<void> {
    try {
        const store = await getStore()
        log.debug('Saving shortcuts: {shortcuts}', { shortcuts: JSON.stringify(Object.fromEntries(customShortcuts)) })

        // Store each command's shortcuts at top level (like settings-store does)
        // This avoids potential issues with nested objects
        for (const [commandId, shortcuts] of customShortcuts) {
            await store.set(`shortcut:${commandId}`, shortcuts)
        }
        await store.set('_schemaVersion', SCHEMA_VERSION)
        await store.save()
        log.debug('Shortcuts saved successfully')
    } catch (error) {
        log.error('Failed to save shortcuts: {error}', { error })
    }
}

// ============================================================================
// Change listeners
// ============================================================================

type ShortcutChangeListener = (commandId: string) => void

const listeners = new Set<ShortcutChangeListener>()

/**
 * Subscribe to shortcut changes.
 */
export function onShortcutChange(listener: ShortcutChangeListener): () => void {
    listeners.add(listener)
    return () => listeners.delete(listener)
}

function notifyListeners(commandId: string): void {
    for (const listener of listeners) {
        try {
            listener(commandId)
        } catch (error) {
            log.error('Shortcut change listener error: {error}', { error })
        }
    }

    // Update menu accelerator for commands that have menu items
    void updateMenuAccelerator(commandId)
}

/**
 * Update the menu accelerator for a command.
 * Called automatically when shortcuts change.
 * Only affects commands that have corresponding menu items.
 */
async function updateMenuAccelerator(commandId: string): Promise<void> {
    // Only certain commands have menu items with accelerators
    const menuCommands = ['view.fullMode', 'view.briefMode']
    if (!menuCommands.includes(commandId)) return

    try {
        const shortcuts = getEffectiveShortcuts(commandId)
        // Use the first shortcut for the menu accelerator (menus only show one)
        const shortcut = shortcuts[0] ?? ''
        await invoke('update_menu_accelerator', { commandId, shortcut })
    } catch (error) {
        log.error('Failed to update menu accelerator: {error}', { error })
    }
}

// ============================================================================
// Utility: Force save (for testing)
// ============================================================================

/**
 * Force an immediate save to disk. Used for testing.
 */
export async function forceSave(): Promise<void> {
    await saveToStore()
}
