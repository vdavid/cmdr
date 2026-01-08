// Settings persistence for user preferences

import { load } from '@tauri-apps/plugin-store'
import type { Store } from '@tauri-apps/plugin-store'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'

const STORE_NAME = 'settings.json'

export type FullDiskAccessChoice = 'allow' | 'deny' | 'notAskedYet'

export interface Settings {
    showHiddenFiles: boolean
    fullDiskAccessChoice: FullDiskAccessChoice
}

const DEFAULT_SETTINGS: Settings = {
    showHiddenFiles: true,
    fullDiskAccessChoice: 'notAskedYet',
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
        const fullDiskAccessChoice = await store.get('fullDiskAccessChoice')

        const validChoices: FullDiskAccessChoice[] = ['allow', 'deny', 'notAskedYet']
        return {
            showHiddenFiles: typeof showHiddenFiles === 'boolean' ? showHiddenFiles : DEFAULT_SETTINGS.showHiddenFiles,
            fullDiskAccessChoice: validChoices.includes(fullDiskAccessChoice as FullDiskAccessChoice)
                ? (fullDiskAccessChoice as FullDiskAccessChoice)
                : DEFAULT_SETTINGS.fullDiskAccessChoice,
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
        }
        if (settings.fullDiskAccessChoice !== undefined) {
            await store.set('fullDiskAccessChoice', settings.fullDiskAccessChoice)
        }
        await store.save()
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
