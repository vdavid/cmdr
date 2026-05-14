// App-level state: MCP pane state, dialog tracking, menu context, window lifecycle

import { invoke } from '@tauri-apps/api/core'
import { commands, type PaneFileEntry, type PaneState } from '$lib/ipc/bindings'

export type { PaneFileEntry, PaneState }

// ============================================================================
// MCP pane state
// ============================================================================

/**
 * Update left pane state for MCP context tools.
 */
export async function updateLeftPaneState(state: PaneState): Promise<void> {
  await commands.updateLeftPaneState(state)
}

/**
 * Update right pane state for MCP context tools.
 */
export async function updateRightPaneState(state: PaneState): Promise<void> {
  await commands.updateRightPaneState(state)
}

/**
 * Update focused pane for MCP context tools.
 */
export async function updateFocusedPane(pane: 'left' | 'right'): Promise<void> {
  await commands.updateFocusedPane(pane)
}

/** Tab info for MCP state sync. */
export interface McpTabInfo {
  id: string
  path: string
  pinned: boolean
  active: boolean
}

/**
 * Update tab list for a pane (for MCP state reporting).
 */
export async function updatePaneTabs(pane: string, tabs: McpTabInfo[]): Promise<void> {
  await commands.updatePaneTabs(pane, tabs)
}

/** Updates the File menu "Pin tab" / "Unpin tab" label based on active tab state. */
export async function updatePinTabMenu(isPinned: boolean): Promise<void> {
  // eslint-disable-next-line cmdr/no-raw-tauri-invoke -- not in typed bindings; tracked for follow-up
  await invoke('update_pin_tab_menu', { isPinned })
}

/** Enables or disables the Tab menu "Reopen closed tab" item based on whether the focused pane's closed-tab stack has entries. */
export async function setReopenClosedTabEnabled(enabled: boolean): Promise<void> {
  // eslint-disable-next-line cmdr/no-raw-tauri-invoke -- generic over Runtime; not in typed bindings
  await invoke('set_reopen_closed_tab_enabled', { enabled })
}

// ============================================================================
// Dialog tracking
// ============================================================================

/** Notify backend that a soft (overlay) dialog opened. */
export async function notifyDialogOpened(dialogType: string): Promise<void> {
  await commands.notifyDialogOpened(dialogType)
}

/** Notify backend that a soft (overlay) dialog closed. */
export async function notifyDialogClosed(dialogType: string): Promise<void> {
  await commands.notifyDialogClosed(dialogType)
}

/** Register all known soft dialog types with the backend for the MCP "available dialogs" resource. */
export async function registerKnownDialogs(dialogs: readonly { id: string; description?: string }[]): Promise<void> {
  await commands.registerKnownDialogs(dialogs.map((d) => ({ id: d.id, description: d.description ?? null })))
}

// ============================================================================
// Menu context and view settings
// ============================================================================

/**
 * Updates the global menu context (used by app-level File menu).
 * @param path - Absolute path to the file.
 * @param filename - Name of the file.
 */
export async function updateMenuContext(path: string, filename: string): Promise<void> {
  // eslint-disable-next-line cmdr/no-raw-tauri-invoke -- not in typed bindings; tracked for follow-up
  await invoke('update_menu_context', { path, filename })
}

/**
 * Enables or disables file-scoped menu items based on the current context.
 * Call with "explorer" when the main file explorer has focus, "other" when
 * Settings or a file viewer window has focus.
 */
export async function setMenuContext(context: 'explorer' | 'other'): Promise<void> {
  // eslint-disable-next-line cmdr/no-raw-tauri-invoke -- not in typed bindings; tracked for follow-up
  await invoke('set_menu_context', { context })
}

/**
 * Toggle hidden files visibility and sync menu checkbox state.
 * @returns The new state of showHiddenFiles.
 */
export async function toggleHiddenFiles(): Promise<boolean> {
  // eslint-disable-next-line cmdr/no-raw-tauri-invoke -- not in typed bindings; tracked for follow-up
  return invoke<boolean>('toggle_hidden_files')
}

/**
 * Pushes the full View menu state to the backend: which pane is active and the
 * current view mode of each pane. The backend updates check states on all four
 * per-pane items, and migrates the keyboard accelerator (⌘1/⌘2 by default) to
 * the active pane's pair if focus changed.
 *
 * Call on initial mount, focus change, swap, and after any view-mode change
 * (palette, MCP, menu click round-trip).
 */
export async function updateViewModeMenu(
  activePane: 'left' | 'right',
  leftMode: 'full' | 'brief',
  rightMode: 'full' | 'brief',
): Promise<void> {
  // eslint-disable-next-line cmdr/no-raw-tauri-invoke -- not in typed bindings; tracked for follow-up
  await invoke('update_view_mode_menu', { activePane, leftMode, rightMode })
}

// ============================================================================
// Window lifecycle
// ============================================================================

/**
 * Shows the main window.
 * Should be called when the frontend is ready to avoid white flash.
 */
export async function showMainWindow(): Promise<void> {
  // eslint-disable-next-line cmdr/no-raw-tauri-invoke -- not in typed bindings; tracked for follow-up
  await invoke('show_main_window')
}
