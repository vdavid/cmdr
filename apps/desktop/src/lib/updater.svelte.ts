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
    if (updateState.status === 'downloading' || updateState.status === 'ready') {
        return // Don't interrupt ongoing download or ready state
    }

    updateState.status = 'checking'
    updateState.error = null

    try {
        const update = await check()

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
    // Skip update checks in dev mode to avoid hitting real endpoint
    if (import.meta.env.DEV) {
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
