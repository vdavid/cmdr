/**
 * Event listeners for index directory updates.
 * When the index computes or refreshes dir_stats for directories,
 * this module notifies affected panes to re-fetch their visible range.
 */

import { listen, type UnlistenFn } from '$lib/tauri-commands'

/**
 * Listen for `index-dir-updated` events and call `onDirUpdated` with the
 * updated directory paths. Each pane can then check if any path is a child
 * of its current directory and re-fetch if needed.
 *
 * Returns an unlisten function for cleanup.
 */
export async function initIndexEvents(onDirUpdated: (paths: string[]) => void): Promise<UnlistenFn> {
    return listen<{ paths: string[] }>('index-dir-updated', (event) => {
        onDirUpdated(event.payload.paths)
    })
}
