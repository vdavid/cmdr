// File actions: open, reveal, preview, and context menu commands

import { invoke } from '@tauri-apps/api/core'
import { openUrl } from '@tauri-apps/plugin-opener'
import {
  commands,
  type OpenTerminalError,
  type OpenTerminalOutcome,
  type TerminalAppList,
  type TimedOut,
} from '$lib/ipc/bindings'
import { TypedFailure } from '$lib/ipc/typed-failure'
import { throwIpcError } from './ipc-types'

export type { OpenTerminalError, OpenTerminalOutcome, TerminalApp, TerminalAppList } from '$lib/ipc/bindings'

/**
 * Opens a file with the system's default application.
 *
 * Routes through the backend `openPath` command (not the opener plugin) so the
 * `playwright-e2e` build can record the open instead of launching an external
 * app. Otherwise the E2E suite floods the desktop with orphan TextEdit/Preview
 * windows it can't close. See `src-tauri/src/commands/file_actions.rs`.
 * @param path - Path to the file to open.
 */
export async function openFile(path: string): Promise<void> {
  const res = await commands.openPath(path)
  if (res.status === 'error') throwIpcError(res.error)
}

/**
 * Opens a URL in the system's default browser.
 * @param url - URL to open (like "https://getcmdr.com/renew")
 */
export async function openExternalUrl(url: string): Promise<void> {
  await openUrl(url)
}

/**
 * What the PANE contributes to a file context menu, as opposed to the file that was
 * right-clicked. One object rather than three trailing booleans and a string: they
 * all come from one pane read, and positional flags of the same type are exactly
 * what silently binds to the wrong slot. Every field defaults to the most
 * restrictive answer, so a surface that can't answer says nothing.
 */
export interface PaneContextMenuFacts {
  /**
   * Hides Rename and New folder. `true` for a right-click inside a virtual pane
   * that isn't a real directory (the search-results snapshot pane; see
   * `apps/desktop/src/lib/search/capabilities.ts`).
   */
  restrictDestinationActions?: boolean
  /**
   * The pane's listing id, so a Finder-tag color click can refresh that listing's
   * cache after writing. Omit for a virtual pane with no normal listing; the tag
   * still writes to disk.
   */
  listingId?: string
  /**
   * Whether "Open terminal here" is clickable. It acts on the PANE's folder, not
   * the right-clicked file, so a pane on a phone (or a surface with no folder of
   * its own, like the Search dialog) leaves it out and the item shows greyed.
   */
  canOpenTerminalHere?: boolean
}

/**
 * Shows a native context menu for a file.
 * @param path - Absolute path to the right-clicked file (the "primary" file).
 * @param filename - Name of the right-clicked file.
 * @param isDirectory - Whether the entry is a directory.
 * @param paths - All paths the menu's actions should affect. For a right-click on a non-selected
 *                file, pass `[path]`. For a right-click on a file that's part of a multi-selection,
 *                pass the full selection so "Open with" launches all files at once.
 * @param pane - What the surface the click landed in contributes. See {@link PaneContextMenuFacts}.
 */
export async function showFileContextMenu(
  path: string,
  filename: string,
  isDirectory: boolean,
  paths: string[],
  pane: PaneContextMenuFacts = {},
): Promise<void> {
  // eslint-disable-next-line cmdr/no-raw-tauri-invoke -- generic <R: Runtime> command, excluded from specta bindings (see the `ipc.rs` manifest)
  await invoke('show_file_context_menu', {
    path,
    filename,
    isDirectory,
    paths,
    pane: {
      restrictDestinationActions: pane.restrictDestinationActions ?? false,
      listingId: pane.listingId ?? '',
      canOpenTerminalHere: pane.canOpenTerminalHere ?? false,
    },
  })
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
 *
 * Pass `ejectVolumeId` + `ejectVolumeName` when the breadcrumb represents an
 * ejectable volume — the menu will include an "Eject ({name})" item that emits
 * a `volume-context-action` event on click (subscribe via `onVolumeContextAction`).
 * Pass both or neither; one without the other is treated as no eject target.
 *
 * @param shortcut - Frontend shortcut string for "Copy path" (e.g. "⌘⌥C"), or empty.
 * @param ejectVolumeId - Volume to eject when the user clicks the eject item.
 * @param ejectVolumeName - Display name for the "Eject ({name})" label.
 */
export async function showBreadcrumbContextMenu(
  shortcut: string,
  ejectVolumeId?: string,
  ejectVolumeName?: string,
): Promise<void> {
  // eslint-disable-next-line cmdr/no-raw-tauri-invoke -- generic <R: Runtime> command, excluded from specta bindings (see the `ipc.rs` manifest)
  await invoke('show_breadcrumb_context_menu', {
    shortcut,
    ejectVolumeId: ejectVolumeId ?? null,
    ejectVolumeName: ejectVolumeName ?? null,
  })
}

/**
 * Shows the native context menu for a row in the volume-selector dropdown.
 *
 * A favorite row gets `Rename` + `Remove`; an ejectable volume row gets `Eject ({name})`.
 * The picked action arrives via the `volume-context-action` event (`onVolumeContextAction`),
 * the same path as the breadcrumb eject item.
 *
 * @param volumeId - Target row's id (the `fav-…` switcher id for a favorite).
 * @param volumeName - Target row's display name (used in the eject label / rename seed).
 * @param isFavorite - True for a favorite row (Rename / Remove); false for a volume row.
 * @param isEjectable - True when the volume row can be ejected (adds the Eject item).
 */
export async function showVolumeRowContextMenu(
  volumeId: string,
  volumeName: string,
  isFavorite: boolean,
  isEjectable: boolean,
): Promise<void> {
  // eslint-disable-next-line cmdr/no-raw-tauri-invoke -- generic <R: Runtime> command, excluded from specta bindings (see the `ipc.rs` manifest)
  await invoke('show_volume_row_context_menu', {
    volumeId,
    volumeName,
    isFavorite,
    isEjectable,
  })
}

/**
 * Shows the minimal `..` parent-row context menu (just "Add to favorites").
 * The full file context menu doesn't fit `..`, so this is its own one-item menu.
 * @param parentPath - The directory the `..` row points at; favorited on click.
 */
export async function showParentRowContextMenu(parentPath: string): Promise<void> {
  // eslint-disable-next-line cmdr/no-raw-tauri-invoke -- generic over <R: Runtime>, not in typed bindings
  await invoke('show_parent_row_context_menu', { parentPath })
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
  // eslint-disable-next-line cmdr/no-raw-tauri-invoke -- generic <R: Runtime> command, excluded from specta bindings (see the `ipc.rs` manifest)
  await invoke('copy_to_clipboard', { text })
}

/**
 * Open the native Quick Look panel on the given path (macOS only).
 * No-op on volumes without local-fs access (MTP etc.) and on non-macOS.
 * @param path - Absolute path to the file under the cursor.
 * @param volumeId - Volume id of the path. Backend uses this to gate non-local volumes.
 */
export async function quickLookOpen(path: string, volumeId: string): Promise<void> {
  const res = await commands.quickLookOpen(path, volumeId)
  if (res.status === 'error') throwIpcError(res.error)
}

/**
 * Retarget an open Quick Look panel to a new path (macOS only). No-op when the panel isn't
 * currently open. Used by the cursor-follow `$effect` in the file pane.
 */
export async function quickLookSetPath(path: string, volumeId: string): Promise<void> {
  const res = await commands.quickLookSetPath(path, volumeId)
  if (res.status === 'error') throwIpcError(res.error)
}

/**
 * Close the Quick Look panel (macOS only). No-op when not open.
 * The backend also emits `quick-look-closed` when the panel is dismissed by ✕ or Esc;
 * the frontend listens for that event in `quick-look-state.svelte.ts`.
 */
export async function quickLookClose(): Promise<void> {
  const res = await commands.quickLookClose()
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

/**
 * The terminal apps installed on this Mac, plus which one `appChoice` names.
 *
 * `appChoice` is the stored `behavior.openTerminalHereApp` value: the frontend
 * owns the settings store, so it hands the choice down rather than having Rust
 * read it back. `chosenId` comes back `null` when that app has been uninstalled.
 *
 * Cheap enough to ask on every render (one LaunchServices lookup per known app),
 * so nothing caches it and there's no refresh button.
 * @param appChoice - The stored choice: a bundle id, or an absolute `.app` path.
 */
export async function listTerminalApps(appChoice: string): Promise<TimedOut<TerminalAppList>> {
  return await commands.listTerminalApps(appChoice)
}

/** A launch that never started, still carrying the backend's typed reason. */
export class OpenTerminalFailure extends TypedFailure<OpenTerminalError> {
  constructor(failure: OpenTerminalError) {
    super(failure, `open terminal here refused: ${failure.type}`)
    this.name = 'OpenTerminalFailure'
  }
}

/** The typed refusal behind a caught value, or `null` when it isn't one. */
export function asOpenTerminalError(error: unknown): OpenTerminalError | null {
  return error instanceof OpenTerminalFailure ? error.failure : null
}

/**
 * Opens `path` in the terminal app `appChoice` names.
 *
 * Resolves to an OUTCOME, not a bare success: the chosen app may have been
 * uninstalled (Terminal opened instead), and the volume may hand out no path a
 * shell can reach (nothing opened). Throws {@link OpenTerminalFailure} only when
 * the launch itself couldn't be attempted.
 *
 * @param path - The folder the pane resolved (the cursor's folder, or its own).
 * @param volumeId - The volume that path came from; the path-less refusal keys on it.
 * @param appChoice - The stored choice: a bundle id, or an absolute `.app` path.
 */
export async function openTerminalHere(
  path: string,
  volumeId: string,
  appChoice: string,
): Promise<OpenTerminalOutcome> {
  const res = await commands.openTerminalHere(path, volumeId, appChoice)
  if (res.status === 'error') throw new OpenTerminalFailure(res.error)
  return res.data
}

/**
 * What to call the app `appChoice` names, or `null` when Cmdr carries no name for
 * it. A table lookup, which is all that's left once the app is uninstalled: the
 * "that app is gone" toast needs the name at exactly the moment there's no bundle
 * to read one from.
 */
export async function terminalAppDisplayName(appChoice: string): Promise<string | null> {
  return await commands.terminalAppDisplayName(appChoice)
}
