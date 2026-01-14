import { check, type Update } from '@tauri-apps/plugin-updater'
import { relaunch } from '@tauri-apps/plugin-process'

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
    // eslint-disable-next-line no-console
    console.log('[updater] checkForUpdates called, current status:', updateState.status)

    if (updateState.status === 'downloading' || updateState.status === 'ready') {
        return // Don't interrupt ongoing download or ready state
    }

    updateState.status = 'checking'
    updateState.error = null

    try {
        // eslint-disable-next-line no-console
        console.log('[updater] Checking for updates...')
        const update = await check()
        // eslint-disable-next-line no-console
        console.log('[updater] Check result:', update)

        if (update !== null) {
            updateState.status = 'downloading'
            await update.downloadAndInstall()
            updateState.status = 'ready'
            updateState.update = update
        } else {
            updateState.status = 'idle'
        }
    } catch (error) {
        updateState.status = 'idle'
        updateState.error = error instanceof Error ? error.message : String(error)
        // eslint-disable-next-line no-console
        console.error('Update check failed:', error)
    }
}

export async function restartToUpdate(): Promise<void> {
    await relaunch()
}

export function startUpdateChecker(): () => void {
    // eslint-disable-next-line no-console
    console.log('[updater] startUpdateChecker called, DEV mode:', import.meta.env.DEV)

    // Skip update checks in dev mode to avoid hitting real endpoint
    if (import.meta.env.DEV) {
        // eslint-disable-next-line no-console
        console.log('[updater] Skipping update check in dev mode')
        return () => {}
    }

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
