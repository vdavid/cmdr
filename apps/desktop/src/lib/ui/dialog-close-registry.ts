/**
 * A self-maintaining registry of the close function for each currently-mounted soft
 * dialog, keyed by its `SoftDialogId`. `ModalDialog` and `QueryDialog` register their
 * `onclose` on mount and unregister on destroy, so the map always reflects what's open
 * and closable without any central per-dialog wiring.
 *
 * The MCP `dialog` tool's generic `close` action drives this: the backend emits
 * `mcp-close-dialog { id }`, the main-window router calls `closeDialogById(id)`, and the
 * matching dialog's own close primitive runs (unmounting it, which fires
 * `notifyDialogClosed` → the backend's `SoftDialogTracker` → the tool's ack). A dialog
 * that renders without an `onclose` (a mid-flow sheet with no dismiss affordance) simply
 * isn't registered, so `closeDialogById` returns `false` and the tool reports an honest
 * failure rather than silently closing something it can't.
 */

import type { SoftDialogId } from './dialog-registry'

const closers = new Map<SoftDialogId, () => void>()

/** Register a dialog's close function. Called by `ModalDialog` / `QueryDialog` on mount. */
export function registerDialogClose(id: SoftDialogId, close: () => void): void {
  closers.set(id, close)
}

/**
 * Unregister a dialog's close function on destroy. Only removes the entry if it's still
 * the one being unregistered, so a rapid remount (HMR, or reopen before the old destroy
 * ran) can't have the outgoing instance evict the incoming one's registration.
 */
export function unregisterDialogClose(id: SoftDialogId, close: () => void): void {
  if (closers.get(id) === close) closers.delete(id)
}

/**
 * Close the dialog with `id` via its registered close function. Returns `true` if a close
 * ran, `false` if no such dialog is currently open/closable (the caller reports the honest
 * failure). Never throws: a close primitive's own error is swallowed so a broken dialog
 * can't wedge the router.
 */
export function closeDialogById(id: string): boolean {
  const close = closers.get(id as SoftDialogId)
  if (!close) return false
  try {
    close()
  } catch {
    // The dialog's own close threw; treat as "attempted" — the backend ack (or its
    // timeout) is the source of truth for whether it actually closed.
  }
  return true
}
