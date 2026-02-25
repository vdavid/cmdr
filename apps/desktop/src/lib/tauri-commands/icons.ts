// Icon fetching and cache management

import { invoke } from '@tauri-apps/api/core'

/**
 * Gets icon data URLs for the requested icon IDs.
 * @param iconIds - Array of icon IDs like "ext:jpg", "dir", "symlink"
 * @param useAppIconsForDocuments - Whether to use app icons as fallback for documents
 * @returns Map of icon_id -> base64 WebP data URL
 */
export async function getIcons(iconIds: string[], useAppIconsForDocuments: boolean): Promise<Record<string, string>> {
    return invoke<Record<string, string>>('get_icons', { iconIds, useAppIconsForDocuments })
}

/**
 * Refreshes icons for a directory listing.
 * Fetches icons in parallel for directories (by path) and extensions.
 * @param directoryPaths - Array of directory paths to fetch icons for
 * @param extensions - Array of file extensions (without dot)
 * @param useAppIconsForDocuments - Whether to use app icons as fallback for documents
 * @returns Map of icon_id -> base64 WebP data URL
 */
export async function refreshDirectoryIcons(
    directoryPaths: string[],
    extensions: string[],
    useAppIconsForDocuments: boolean,
): Promise<Record<string, string>> {
    return invoke<Record<string, string>>('refresh_directory_icons', {
        directoryPaths,
        extensions,
        useAppIconsForDocuments,
    })
}

/**
 * Clears cached extension icons.
 * Called when the "use app icons for documents" setting changes.
 */
export async function clearExtensionIconCache(): Promise<void> {
    await invoke('clear_extension_icon_cache')
}

/**
 * Clears cached directory icons (`dir`, `symlink-dir`, `path:*`).
 * Called when the system theme or accent color changes.
 */
export async function clearDirectoryIconCache(): Promise<void> {
    await invoke('clear_directory_icon_cache')
}
