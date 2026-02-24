/**
 * Pure utility functions for path navigation logic.
 * Extracted from DualPaneExplorer.svelte to improve modularity.
 *
 * All pathExists calls use frontend timeouts to prevent hangs on slow/dead network mounts.
 * The Rust backend also enforces a 2-second timeout per pathExists call.
 */

import { pathExists } from '$lib/tauri-commands'
import { getLastUsedPathForVolume } from '$lib/app-status-store'
import { DEFAULT_VOLUME_ID } from '$lib/tauri-commands'

export interface OtherPaneState {
    otherPaneVolumeId: string
    otherPanePath: string
}

/** Races a promise against a timeout, returning the fallback if it doesn't resolve in time. */
export function withTimeout<T>(promise: Promise<T>, ms: number, fallback: T): Promise<T> {
    return Promise.race([
        promise,
        new Promise<T>((resolve) =>
            setTimeout(() => {
                resolve(fallback)
            }, ms),
        ),
    ])
}

/**
 * Determines which path to navigate to when switching volumes.
 * Runs checks in parallel with 500ms frontend timeouts per check.
 * Priority order:
 * 1. Favorite path (if targetPath !== volumePath)
 * 2. Other pane's path (if the other pane is on the same volume)
 * 3. Stored lastUsedPath for this volume
 * 4. Default: ~ for main volume, volume root for others
 */
export async function determineNavigationPath(
    volumeId: string,
    volumePath: string,
    targetPath: string,
    otherPane: OtherPaneState,
): Promise<string> {
    const pathExistsTimeoutMs = 500

    // User navigated to a favorite — go to the favorite's path directly
    if (targetPath !== volumePath) {
        return targetPath
    }

    // Run both checks in parallel with timeouts
    const [otherPaneValid, lastUsedResult] = await Promise.all([
        otherPane.otherPaneVolumeId === volumeId
            ? withTimeout(pathExists(otherPane.otherPanePath), pathExistsTimeoutMs, false)
            : Promise.resolve(false),
        getLastUsedPathForVolume(volumeId).then((p) =>
            p ? withTimeout(pathExists(p), pathExistsTimeoutMs, false).then((ok) => (ok ? p : null)) : null,
        ),
    ])

    if (otherPaneValid) return otherPane.otherPanePath
    if (lastUsedResult) return lastUsedResult

    // Default: ~ for main volume (root), volume path for others
    return volumeId === DEFAULT_VOLUME_ID ? '~' : volumePath
}

/**
 * Resolves a path to a valid existing path by walking up the parent tree.
 * Each step has a 1-second timeout to prevent hanging on dead mounts.
 * Fallback chain: parent tree → user home (~) → filesystem root (/).
 * Returns null if even the root doesn't exist (volume unmounted).
 */
export async function resolveValidPath(targetPath: string): Promise<string | null> {
    const stepTimeoutMs = 1000

    let path = targetPath
    while (path !== '/' && path !== '') {
        if (await withTimeout(pathExists(path), stepTimeoutMs, false)) {
            return path
        }
        // Go to parent
        const lastSlash = path.lastIndexOf('/')
        path = lastSlash > 0 ? path.substring(0, lastSlash) : '/'
    }
    // Try user home before falling back to root (~ is expanded by the backend)
    if (await withTimeout(pathExists('~'), stepTimeoutMs, false)) {
        return '~'
    }
    // Check root
    if (await withTimeout(pathExists('/'), stepTimeoutMs, false)) {
        return '/'
    }
    return null
}
