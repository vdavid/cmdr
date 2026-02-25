// File actions: open, reveal, preview, and context menu commands

import { invoke } from '@tauri-apps/api/core'
import { openPath, openUrl } from '@tauri-apps/plugin-opener'

/**
 * Opens a file with the system's default application.
 * @param path - Path to the file to open.
 */
export async function openFile(path: string): Promise<void> {
    await openPath(path)
}

/**
 * Opens a URL in the system's default browser.
 * @param url - URL to open (like "https://getcmdr.com/renew")
 */
export async function openExternalUrl(url: string): Promise<void> {
    await openUrl(url)
}

/**
 * Shows a native context menu for a file.
 * @param path - Absolute path to the file.
 * @param filename - Name of the file.
 * @param isDirectory - Whether the entry is a directory.
 */
export async function showFileContextMenu(path: string, filename: string, isDirectory: boolean): Promise<void> {
    await invoke('show_file_context_menu', { path, filename, isDirectory })
}

/**
 * Show a file in Finder (reveal in parent folder).
 * @param path - Absolute path to the file.
 */
export async function showInFinder(path: string): Promise<void> {
    await invoke('show_in_finder', { path })
}

/**
 * Copy text to clipboard.
 * @param text - Text to copy.
 */
export async function copyToClipboard(text: string): Promise<void> {
    await invoke('copy_to_clipboard', { text })
}

/**
 * Quick Look preview (macOS only).
 * @param path - Absolute path to the file.
 */
export async function quickLook(path: string): Promise<void> {
    await invoke('quick_look', { path })
}

/**
 * Open Get Info window in Finder (macOS only).
 * @param path - Absolute path to the file.
 */
export async function getInfo(path: string): Promise<void> {
    await invoke('get_info', { path })
}

/**
 * Open file in the system's default text editor (macOS only).
 * Uses `open -t` which opens the file in the default text editor.
 * @param path - Absolute path to the file.
 */
export async function openInEditor(path: string): Promise<void> {
    await invoke('open_in_editor', { path })
}
