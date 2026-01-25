/**
 * Shortcuts persistence layer - stores custom keyboard shortcuts.
 * See docs/specs/shortcut-settings.md §6 for specification.
 */

import { load, type Store } from '@tauri-apps/plugin-store'
import { commands } from '$lib/commands/command-registry'

// ============================================================================
// Store configuration
// ============================================================================

const STORE_NAME = 'shortcuts.json'
const SCHEMA_VERSION = 1
const SAVE_DEBOUNCE_MS = 500

let storeInstance: Store | null = null
let saveTimeout: ReturnType<typeof setTimeout> | null = null

// In-memory cache of custom shortcuts
const customShortcuts: Record<string, string[]> = {}
let initialized = false

// ============================================================================
// Initialization
// ============================================================================

async function getStore(): Promise<Store> {
    if (!storeInstance) {
        storeInstance = await load(STORE_NAME, {
            defaults: { _schemaVersion: SCHEMA_VERSION, shortcuts: {} },
            autoSave: false,
        })
    }
    return storeInstance
}

/**
 * Initialize the shortcuts store. Must be called before using other functions.
 */
export async function initializeShortcuts(): Promise<void> {
    if (initialized) return

    const store = await getStore()

    // Check schema version and migrate if needed
    const version = await store.get<number>('_schemaVersion')
    if (version !== undefined && version !== SCHEMA_VERSION) {
        await migrateShortcuts(store, version)
    }

    // Load custom shortcuts
    const stored = await store.get<Record<string, string[]>>('shortcuts')
    if (stored) {
        Object.assign(customShortcuts, stored)
    }

    initialized = true
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
    return { ...customShortcuts }
}

/**
 * Get effective shortcuts for a command (custom if set, otherwise defaults).
 */
export function getEffectiveShortcuts(commandId: string): string[] {
    if (commandId in customShortcuts) {
        return [...customShortcuts[commandId]]
    }

    const command = commands.find((c) => c.id === commandId)
    return command?.shortcuts ?? []
}

/**
 * Get default shortcuts for a command from the registry.
 */
export function getDefaultShortcuts(commandId: string): string[] {
    const command = commands.find((c) => c.id === commandId)
    return command?.shortcuts ?? []
}

/**
 * Check if a command's shortcuts have been modified from defaults.
 */
export function isShortcutModified(commandId: string): boolean {
    return commandId in customShortcuts
}

/**
 * Set a specific shortcut for a command at an index.
 */
export function setShortcut(commandId: string, index: number, shortcut: string): void {
    const current = getEffectiveShortcuts(commandId)

    if (index >= 0 && index < current.length) {
        current[index] = shortcut
    } else if (index === current.length) {
        current.push(shortcut)
    }

    customShortcuts[commandId] = current
    scheduleSave()
    notifyListeners(commandId)
}

/**
 * Add a new shortcut to a command.
 */
export function addShortcut(commandId: string, shortcut: string): void {
    const current = getEffectiveShortcuts(commandId)
    current.push(shortcut)
    customShortcuts[commandId] = current
    scheduleSave()
    notifyListeners(commandId)
}

/**
 * Remove a shortcut from a command at an index.
 */
export function removeShortcut(commandId: string, index: number): void {
    const current = getEffectiveShortcuts(commandId)

    if (index >= 0 && index < current.length) {
        current.splice(index, 1)
        customShortcuts[commandId] = current
        scheduleSave()
        notifyListeners(commandId)
    }
}

/**
 * Reset a command's shortcuts to defaults.
 */
export function resetShortcut(commandId: string): void {
    if (commandId in customShortcuts) {
        // eslint-disable-next-line @typescript-eslint/no-dynamic-delete
        delete customShortcuts[commandId]
        scheduleSave()
        notifyListeners(commandId)
    }
}

/**
 * Reset all shortcuts to defaults.
 */
export async function resetAllShortcuts(): Promise<void> {
    const modifiedIds = Object.keys(customShortcuts)

    // Clear all customizations
    for (const key of Object.keys(customShortcuts)) {
        // eslint-disable-next-line @typescript-eslint/no-dynamic-delete
        delete customShortcuts[key]
    }

    // Save immediately
    const store = await getStore()
    await store.set('shortcuts', {})
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
    try {
        const store = await getStore()
        await store.set('shortcuts', customShortcuts)
        await store.set('_schemaVersion', SCHEMA_VERSION)
        await store.save()
    } catch (error) {
        // eslint-disable-next-line no-console
        console.error('Failed to save shortcuts:', error)
        // Retry once
        try {
            const store = await getStore()
            await store.save()
        } catch (retryError) {
            // eslint-disable-next-line no-console
            console.error('Retry failed:', retryError)
        }
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
            // eslint-disable-next-line no-console
            console.error('Shortcut change listener error:', error)
        }
    }
}

// ============================================================================
// Utility: Force save (for testing)
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
