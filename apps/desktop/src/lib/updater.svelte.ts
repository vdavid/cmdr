import { check, type Update } from '@tauri-apps/plugin-updater'
import { relaunch } from '@tauri-apps/plugin-process'
import { getVersion } from '@tauri-apps/api/app'
import { feLog } from './tauri-commands'

const checkIntervalMs = 60 * 60 * 1000 // 60 minutes

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
        feLog(`[updater] Checking for updates (current: v${currentVersion})...`)
        const update = await check()

        if (update !== null) {
            feLog(`[updater] Update available: v${currentVersion} → v${update.version}`)
            updateState.status = 'downloading'
            await update.downloadAndInstall()
            feLog(`[updater] v${update.version} downloaded, restart to apply`)
            updateState.status = 'ready'
            updateState.update = update
        } else {
            feLog(`[updater] v${currentVersion} is up to date`)
            updateState.status = 'idle'
        }
    } catch (error) {
        updateState.status = 'idle'
        updateState.error = error instanceof Error ? error.message : String(error)
        feLog(`[updater] Check failed: ${updateState.error}`)
    }
}

export async function restartToUpdate(): Promise<void> {
    await relaunch()
}

export function startUpdateChecker(): () => void {
    const endpoint = import.meta.env.DEV ? 'localhost:4321' : 'getcmdr.com'
    feLog(`[updater] Started (endpoint: ${endpoint})`)

    // Check immediately on start
    void checkForUpdates()

    // Check periodically
    const intervalId = setInterval(() => {
        void checkForUpdates()
    }, checkIntervalMs)

    // Return cleanup function
    return () => {
        clearInterval(intervalId)
    }
}
