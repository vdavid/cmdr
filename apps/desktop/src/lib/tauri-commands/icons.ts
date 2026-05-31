// Icon fetching and cache management

import type { TimedOut } from './ipc-types'
import { commands } from '$lib/ipc/bindings'

/**
 * Gets icon data URLs for the requested icon IDs.
 * @param iconIds - Array of icon IDs like "ext:jpg", "dir", "symlink"
 * @param useAppIconsForDocuments - Whether to use app icons as fallback for documents
 * @returns Map of icon_id -> base64 WebP data URL, with timeout flag
 */
export async function getIcons(
  iconIds: string[],
  useAppIconsForDocuments: boolean,
): Promise<TimedOut<Record<string, string>>> {
  return commands.getIcons(iconIds, useAppIconsForDocuments)
}

/**
 * Refreshes icons for a directory listing.
 * Fetches icons in parallel for directories (by path) and extensions.
 * @param directoryPaths - Array of directory paths to fetch icons for
 * @param extensions - Array of file extensions (without dot)
 * @param useAppIconsForDocuments - Whether to use app icons as fallback for documents
 * @returns Map of icon_id -> base64 WebP data URL, with timeout flag
 */
export async function refreshDirectoryIcons(
  directoryPaths: string[],
  extensions: string[],
  useAppIconsForDocuments: boolean,
): Promise<TimedOut<Record<string, string>>> {
  return commands.refreshDirectoryIcons(directoryPaths, extensions, useAppIconsForDocuments)
}

/**
 * Detects which of the given VISIBLE directory paths carry a Finder custom-icon
 * flag, returning the `path:{dir}` icon id for each. Called for visible directory
 * rows so the bulk listing never pays the `getxattr` per entry. Feed the result
 * into `getIcons` / `prefetchIcons` to fetch the real icons.
 * @param directoryPaths - Directory paths of the visible rows
 * @returns `path:{dir}` ids for the folders that have a custom icon, with timeout flag
 */
export async function getCustomFolderIconIds(directoryPaths: string[]): Promise<TimedOut<string[]>> {
  return commands.getCustomFolderIconIds(directoryPaths)
}

/**
 * Clears cached extension icons.
 * Called when the "use app icons for documents" setting changes.
 */
export async function clearExtensionIconCache(): Promise<void> {
  await commands.clearExtensionIconCache()
}

/**
 * Clears cached directory icons (`dir`, `symlink-dir`, `path:*`, `pkg:*`, `special:*`)
 * from both the in-memory cache and the on-disk warm tier.
 * Called when the system theme or accent color changes.
 */
export async function clearDirectoryIconCache(): Promise<void> {
  await commands.clearDirectoryIconCache()
}
