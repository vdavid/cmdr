import { check, type Update } from '@tauri-apps/plugin-updater'
import { relaunch } from '@tauri-apps/plugin-process'
import { getVersion } from '@tauri-apps/api/app'
import { getSetting, onSpecificSettingChange } from '$lib/settings/settings-store'
import { getAppLogger } from '$lib/logger'

const log = getAppLogger('updater')

/** Gets the update check interval from settings (in milliseconds) */
function getCheckIntervalMs(): number {
    return getSetting('advanced.updateCheckInterval')
}

interface UpdateState {
    status: 'idle' | 'checking' | 'downloading' | 'ready'
    update: Update | null
    error: string | null
}

const updateState = $state<UpdateState>({
    status: 'idle',
    update: null,
    error: null,
})

export function getUpdateState(): UpdateState {
    return updateState
}

export async function checkForUpdates(): Promise<void> {
    if (updateState.status === 'downloading' || updateState.status === 'ready') {
        return // Don't interrupt ongoing download or ready state
    }

    updateState.status = 'checking'
    updateState.error = null

    try {
        const currentVersion = await getVersion()
        log.info('Checking for updates (current: v{version})...', { version: currentVersion })
        const update = await check()

        if (update !== null) {
            log.info('Update available: v{current} -> v{next}', { current: currentVersion, next: update.version })
            updateState.status = 'downloading'
            await update.downloadAndInstall()
            log.info('v{version} downloaded, restart to apply', { version: update.version })
            updateState.status = 'ready'
            updateState.update = update
        } else {
            log.info('v{version} is up to date', { version: currentVersion })
            updateState.status = 'idle'
        }
    } catch (error) {
        updateState.status = 'idle'
        updateState.error = error instanceof Error ? error.message : String(error)
        log.error('Check failed: {error}', { error: updateState.error })
    }
}

export async function restartToUpdate(): Promise<void> {
    await relaunch()
}

export function startUpdateChecker(): () => void {
    const endpoint = import.meta.env.DEV ? 'localhost:4321' : 'getcmdr.com'
    log.info('Started (endpoint: {endpoint})', { endpoint })

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
