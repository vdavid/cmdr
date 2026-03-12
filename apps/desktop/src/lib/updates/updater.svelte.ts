import { invoke } from '@tauri-apps/api/core'
import { getVersion } from '@tauri-apps/api/app'
import { getSetting, onSpecificSettingChange } from '$lib/settings/settings-store'
import { getAppLogger } from '$lib/logging/logger'
import UpdateToastContent from './UpdateToastContent.svelte'
import { addToast } from '$lib/ui/toast'

const log = getAppLogger('updater')

const isMacOS = navigator.userAgent.includes('Macintosh')

/** Gets the update check interval from settings (in milliseconds) */
function getCheckIntervalMs(): number {
    return getSetting('advanced.updateCheckInterval')
}

/** Metadata returned by the `check_for_update` Tauri command */
interface UpdateInfo {
    version: string
    url: string
    signature: string
}

interface UpdateState {
    status: 'idle' | 'checking' | 'downloading' | 'ready'
    update: UpdateInfo | null
    error: string | null
}

const updateState = $state<UpdateState>({
    status: 'idle',
    update: null,
    error: null,
})

export async function checkForUpdates(): Promise<void> {
    if (updateState.status === 'downloading' || updateState.status === 'ready') {
        return // Don't interrupt ongoing download or ready state
    }

    updateState.status = 'checking'
    updateState.error = null

    try {
        const currentVersion = await getVersion()
        log.debug('Checking for updates (current: v{version})...', { version: currentVersion })

        if (isMacOS) {
            // macOS: custom updater preserves TCC/Full Disk Access permissions
            const update = await invoke<UpdateInfo | null>('check_for_update')

            if (update !== null) {
                log.info('Update available: v{current} -> v{next}', { current: currentVersion, next: update.version })
                updateState.status = 'downloading'
                await invoke('download_update', { url: update.url, signature: update.signature })
                await invoke('install_update')
                log.info('v{version} installed, restart to apply', { version: update.version })
                updateState.status = 'ready'
                updateState.update = update
                addToast(UpdateToastContent, { id: 'update', dismissal: 'persistent' })
            } else {
                log.debug('v{version} is up to date', { version: currentVersion })
                updateState.status = 'idle'
            }
        } else {
            // Non-macOS: delegate to Tauri updater plugin
            const { check } = await import('@tauri-apps/plugin-updater')
            const update = await check()

            if (update) {
                log.info('Update available: v{current} -> v{next}', { current: currentVersion, next: update.version })
                updateState.status = 'downloading'
                await update.downloadAndInstall()
                log.info('v{version} installed, restart to apply', { version: update.version })
                updateState.status = 'ready'
                updateState.update = { version: update.version, url: '', signature: '' }
                addToast(UpdateToastContent, { id: 'update', dismissal: 'persistent' })
            } else {
                log.debug('v{version} is up to date', { version: currentVersion })
                updateState.status = 'idle'
            }
        }
    } catch (error) {
        updateState.status = 'idle'
        updateState.error = error instanceof Error ? error.message : String(error)
        log.error('Check failed: {error}', { error: updateState.error })
    }
}

export function startUpdateChecker(): () => void {
    log.debug('Started')

    // Check immediately on start
    void checkForUpdates()

    // Check periodically using the interval from settings
    let intervalId = setInterval(() => {
        void checkForUpdates()
    }, getCheckIntervalMs())

    // Re-create interval when setting changes
    const unsubscribe = onSpecificSettingChange('advanced.updateCheckInterval', () => {
        clearInterval(intervalId)
        const newInterval = getCheckIntervalMs()
        log.info('Interval changed to {minutes} minutes', { minutes: newInterval / 60000 })
        intervalId = setInterval(() => {
            void checkForUpdates()
        }, newInterval)
    })

    // Return cleanup function
    return () => {
        clearInterval(intervalId)
        unsubscribe()
    }
}
