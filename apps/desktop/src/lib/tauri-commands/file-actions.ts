// File actions: open, reveal, preview, and context menu commands

import { invoke } from '@tauri-apps/api/core'
import { openPath, openUrl } from '@tauri-apps/plugin-opener'
import { commands } from '$lib/ipc/bindings'
import { throwIpcError } from './ipc-types'

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
 * @param path - Absolute path to the right-clicked file (the "primary" file).
 * @param filename - Name of the right-clicked file.
 * @param isDirectory - Whether the entry is a directory.
 * @param paths - All paths the menu's actions should affect. For a right-click on a non-selected
 *                file, pass `[path]`. For a right-click on a file that's part of a multi-selection,
 *                pass the full selection so "Open with" launches all files at once.
 */
export async function showFileContextMenu(
  path: string,
  filename: string,
  isDirectory: boolean,
  paths: string[],
): Promise<void> {
  // eslint-disable-next-line cmdr/no-raw-tauri-invoke -- not in typed bindings; tracked for follow-up
  await invoke('show_file_context_menu', { path, filename, isDirectory, paths })
}

/**
 * Make a cloud-managed file available offline (download it). macOS only. Talks to the
 * File Provider extension responsible for the file (iCloud Drive, Dropbox, GDrive, etc.).
 */
export async function cloudMakeAvailableOffline(path: string): Promise<void> {
  const res = await commands.cloudMakeAvailableOffline(path)
  if (res.status === 'error') throwIpcError(res.error)
}

/**
 * Evict a cloud-managed file's local copy, leaving a placeholder. Counterpart to
 * `cloudMakeAvailableOffline`.
 */
export async function cloudRemoveDownload(path: string): Promise<void> {
  const res = await commands.cloudRemoveDownload(path)
  if (res.status === 'error') throwIpcError(res.error)
}

/**
 * Shows a native context menu for the breadcrumb path bar.
 * @param shortcut - Frontend shortcut string (e.g. "⌃⌘C"), or empty string if no shortcut is configured.
 */
export async function showBreadcrumbContextMenu(shortcut: string): Promise<void> {
  // eslint-disable-next-line cmdr/no-raw-tauri-invoke -- not in typed bindings; tracked for follow-up
  await invoke('show_breadcrumb_context_menu', { shortcut })
}

/**
 * Show a file in the system file manager (reveal in parent folder).
 * On macOS, reveals in Finder. On Linux, uses the default file manager.
 * @param path - Absolute path to the file.
 */
export async function showInFinder(path: string): Promise<void> {
  const res = await commands.showInFinder(path)
  if (res.status === 'error') throwIpcError(res.error)
}

/**
 * Copy text to clipboard.
 * @param text - Text to copy.
 */
export async function copyToClipboard(text: string): Promise<void> {
  // eslint-disable-next-line cmdr/no-raw-tauri-invoke -- not in typed bindings; tracked for follow-up
  await invoke('copy_to_clipboard', { text })
}

/**
 * Quick Look preview (macOS only).
 * @param path - Absolute path to the file.
 */
export async function quickLook(path: string): Promise<void> {
  const res = await commands.quickLook(path)
  if (res.status === 'error') throwIpcError(res.error)
}

/**
 * Open file info window (macOS only, no-op on other platforms).
 * @param path - Absolute path to the file.
 */
export async function getInfo(path: string): Promise<void> {
  const res = await commands.getInfo(path)
  if (res.status === 'error') throwIpcError(res.error)
}

/**
 * Open file in the system's default text editor.
 * On macOS, uses `open -t`. On Linux, uses `xdg-open`.
 * @param path - Absolute path to the file.
 */
export async function openInEditor(path: string): Promise<void> {
  const res = await commands.openInEditor(path)
  if (res.status === 'error') throwIpcError(res.error)
}
