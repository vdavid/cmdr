/**
 * Calls to prioritize_dir and cancel_nav_priority for on-demand micro-scans.
 * Silently ignores errors (indexing may not be initialized).
 */

import { invoke } from '@tauri-apps/api/core'

/** Request a priority micro-scan for a directory. */
export async function prioritizeDir(path: string, priority: 'user_selected' | 'current_dir'): Promise<void> {
    try {
        await invoke('prioritize_dir', { path, priority })
    } catch {
        // Silently ignore -- indexing may not be initialized
    }
}

/** Cancel current-directory micro-scans (called on navigate-away). */
export async function cancelNavPriority(path: string): Promise<void> {
    try {
        await invoke('cancel_nav_priority', { path })
    } catch {
        // Silently ignore -- indexing may not be initialized
    }
}
