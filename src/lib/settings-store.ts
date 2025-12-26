// Settings persistence for user preferences

import { load } from '@tauri-apps/plugin-store'
import type { Store } from '@tauri-apps/plugin-store'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'

const STORE_NAME = 'settings.json'

export interface Settings {
    showHiddenFiles: boolean
}

const DEFAULT_SETTINGS: Settings = {
    showHiddenFiles: true,
}

let storeInstance: Store | null = null

async function getStore(): Promise<Store> {
    if (!storeInstance) {
        storeInstance = await load(STORE_NAME)
    }
    return storeInstance
}

/**
 * Loads user settings from persistent storage.
 * Returns defaults if store is unavailable.
 */
export async function loadSettings(): Promise<Settings> {
    try {
        const store = await getStore()
        const showHiddenFiles = await store.get('showHiddenFiles')
        return {
            showHiddenFiles: typeof showHiddenFiles === 'boolean' ? showHiddenFiles : DEFAULT_SETTINGS.showHiddenFiles,
        }
    } catch {
        // If store fails, return defaults
        return DEFAULT_SETTINGS
    }
}

/**
 * Saves user settings to persistent storage.
 */
export async function saveSettings(settings: Partial<Settings>): Promise<void> {
    try {
        const store = await getStore()
        if (settings.showHiddenFiles !== undefined) {
            await store.set('showHiddenFiles', settings.showHiddenFiles)
            await store.save()
        }
    } catch {
        // Silently fail - persistence is nice-to-have
    }
}

/**
 * Subscribes to settings changes emitted from the backend menu.
 * Returns an unlisten function to clean up the subscription.
 */
export async function subscribeToSettingsChanges(callback: (settings: Partial<Settings>) => void): Promise<UnlistenFn> {
    return listen<Partial<Settings>>('settings-changed', (event) => {
        callback(event.payload)
    })
}
