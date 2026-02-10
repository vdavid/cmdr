/**
 * Pure utility functions for path navigation logic.
 * Extracted from DualPaneExplorer.svelte to improve modularity.
 */

import { pathExists } from '$lib/tauri-commands'
import { getLastUsedPathForVolume } from '$lib/app-status-store'
import { DEFAULT_VOLUME_ID } from '$lib/tauri-commands'

export interface OtherPaneState {
    otherPaneVolumeId: string
    otherPanePath: string
}

/**
 * Determines which path to navigate to when switching volumes.
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
    // User navigated to a favorite - go to the favorite's path directly
    if (targetPath !== volumePath) {
        return targetPath
    }

    // If the other pane is on the same volume, use its path (allows copying paths between panes)
    if (otherPane.otherPaneVolumeId === volumeId && (await pathExists(otherPane.otherPanePath))) {
        return otherPane.otherPanePath
    }

    // Look up the last used path for this volume
    const lastUsedPath = await getLastUsedPathForVolume(volumeId)
    if (lastUsedPath && (await pathExists(lastUsedPath))) {
        return lastUsedPath
    }

    // Default: ~ for main volume (root), volume path for others
    if (volumeId === DEFAULT_VOLUME_ID) {
        return '~'
    }
    return volumePath
}

/**
 * Resolves a path to a valid existing path by walking up the parent tree.
 * Fallback chain: parent tree → user home (~) → filesystem root (/).
 * Returns null if even the root doesn't exist (volume unmounted).
 */
export async function resolveValidPath(targetPath: string): Promise<string | null> {
    let path = targetPath
    while (path !== '/' && path !== '') {
        if (await pathExists(path)) {
            return path
        }
        // Go to parent
        const lastSlash = path.lastIndexOf('/')
        path = lastSlash > 0 ? path.substring(0, lastSlash) : '/'
    }
    // Try user home before falling back to root (~ is expanded by the backend)
    if (await pathExists('~')) {
        return '~'
    }
    // Check root
    if (await pathExists('/')) {
        return '/'
    }
    return null
}
