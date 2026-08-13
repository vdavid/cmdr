// App-level state: MCP pane state, dialog tracking, menu context, window lifecycle

import { invoke } from '@tauri-apps/api/core'
import { commands, type ChildWindowRect, type PaneFileEntry, type PaneState } from '$lib/ipc/bindings'
import type { OperationGate, SoftDialogId } from '$lib/ui/dialog-registry'
import { throwIpcError } from './ipc-types'

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
  // eslint-disable-next-line cmdr/no-raw-tauri-invoke -- generic <R: Runtime> command, excluded from specta bindings (see ipc_collectors.rs)
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

/**
 * Notify backend that a soft (overlay) dialog opened.
 *
 * `SoftDialogId`, not `string`: this is the seam a dialog can slip through
 * untyped (`OnboardingWizard` calls it directly, never via `ModalDialog`), and an
 * unregistered id would leave MCP's "available dialogs" resource blind to it.
 */
export async function notifyDialogOpened(dialogType: SoftDialogId): Promise<void> {
  await commands.notifyDialogOpened(dialogType)
}

/** Notify backend that a soft (overlay) dialog closed. */
export async function notifyDialogClosed(dialogType: SoftDialogId): Promise<void> {
  await commands.notifyDialogClosed(dialogType)
}

/**
 * Register all known soft dialog types with the backend, for the MCP "available
 * dialogs" resource and for the gate that refuses an MCP file operation while a
 * dialog is up.
 *
 * `whileOpen` is declared once, per dialog, in `$lib/ui/dialog-registry.ts`; this
 * ships the verdict across so Rust never keeps a second opinion about it. The call
 * also clears the backend's open-dialog list, since startup means nothing is on
 * screen yet.
 */
export async function registerKnownDialogs(
  dialogs: readonly { id: string; description?: string; whileOpen: OperationGate }[],
): Promise<void> {
  await commands.registerKnownDialogs(
    dialogs.map((d) => ({
      id: d.id,
      description: d.description ?? null,
      blocksOperations: d.whileOpen.blocks,
    })),
  )
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
  // eslint-disable-next-line cmdr/no-raw-tauri-invoke -- generic <R: Runtime> command, excluded from specta bindings (see ipc_collectors.rs)
  await invoke('update_menu_context', { path, filename })
}

/**
 * Activates the menu for the window that just gained focus. On macOS this swaps
 * the app-level menu bar (main ↔ viewer) and enables/disables file-scoped items;
 * on Linux it only toggles the item enabled state (per-window menus already exist).
 *
 * Call with "main" when the main file explorer has focus, "viewer" when a file
 * viewer window has focus, and "other" when Settings or another window has focus.
 */
export async function activateWindowMenu(kind: 'main' | 'viewer' | 'other'): Promise<void> {
  // eslint-disable-next-line cmdr/no-raw-tauri-invoke -- generic over Runtime; not in typed bindings
  await invoke('activate_window_menu', { kind })
}

/**
 * Mirrors the `listing.showHiddenFiles` setting onto the native View menu's
 * CheckMenuItem. Called from `settings-applier.ts` on every change, whoever made
 * it. Does not emit `settings-changed` (that would bounce a menu click straight
 * back at the FE). Safe before the menu is built (no-op if uninitialized).
 *
 * The reverse direction (a menu click, or the MCP `toggle_hidden` tool, which
 * Rust routes through `toggle_hidden_files`) arrives as `settings-changed` and
 * is handled in `routes/(main)/listener-setup.ts`.
 */
export async function syncMenuShowHidden(checked: boolean): Promise<void> {
  // eslint-disable-next-line cmdr/no-raw-tauri-invoke -- generic <R: Runtime> command, excluded from specta bindings (see ipc_collectors.rs)
  await invoke('sync_menu_show_hidden', { checked })
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
  // eslint-disable-next-line cmdr/no-raw-tauri-invoke -- generic <R: Runtime> command, excluded from specta bindings (see ipc_collectors.rs)
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
  // eslint-disable-next-line cmdr/no-raw-tauri-invoke -- generic <R: Runtime> command, excluded from specta bindings (see ipc_collectors.rs)
  await invoke('show_main_window')
}

/**
 * E2E-only: orders the labeled window behind everything without focusing it, so
 * a test run's child windows don't pop in front of the developer's work. No-op
 * outside E2E (the backend gates on `CMDR_E2E_MODE`). The caller is
 * `orderChildWindowToBackInE2e` in `$lib/app-mode`, which has the full rationale.
 */
export async function orderWindowToBack(label: string): Promise<void> {
  // eslint-disable-next-line cmdr/no-raw-tauri-invoke -- generic <R: Runtime> command, excluded from typed bindings (see ipc_collectors.rs)
  await invoke('order_window_to_back', { label })
}

/** Returns the saved rect for a child window label, or `null` if no entry exists. */
export function getChildWindowRect(label: string) {
  return commands.getChildWindowRect(label)
}

/** Saves the rect for a child window label. Called from the move/resize listeners on Settings and Debug. */
export function setChildWindowRect(label: string, rect: ChildWindowRect): Promise<void> {
  return commands.setChildWindowRect(label, rect)
}

/** Updates the menu accelerator for a command, called when a keyboard shortcut is changed. */
export async function updateMenuAccelerator(commandId: string, shortcut: string): Promise<void> {
  const res = await commands.updateMenuAccelerator(commandId, shortcut)
  if (res.status === 'error') throwIpcError(res.error)
}
