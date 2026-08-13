/**
 * Greys out the native menu items that would START a file operation while the
 * main window can't take one: a dialog is up, or the Ask Cmdr composer has focus.
 *
 * ⚠️ This is CHROME, never the guard. Two reasons it can't be:
 *
 * - A disabled item's accelerator still fires (`menu/CLAUDE.md`), so F5 reaches the
 *   frontend whatever the File menu looks like.
 * - The menu is one of several ways in. MCP is refused in Rust
 *   (`mcp/executor/mod.rs`), the command entry points refuse in
 *   `pane/operation-start-gate.ts`, and the start itself refuses in
 *   `pane/dialog-state.svelte.ts`. Those are the guard; this only stops the app
 *   from offering something it would then turn down.
 *
 * Main window only. The viewer runs its own copy of the dialog set, and its two
 * sheets don't block operations anyway, but a second writer would still let one
 * window clobber the other's verdict — so the sync starts here, from main-window
 * startup, and nowhere else.
 */

import { setFileOperationsBlocked } from '$lib/tauri-commands'
import { blockingSoftDialog } from '$lib/ui/open-dialogs.svelte'
import { explorerState } from '$lib/file-explorer/pane/explorer-state.svelte'
import { getAppLogger } from '$lib/logging/logger'

const log = getAppLogger('fileExplorer')

let stopSync: (() => void) | null = null

/**
 * Starts pushing the blocked state to the native menu, and returns the stop
 * function. Idempotent: calling it twice keeps the first scope rather than
 * stacking a second one (HMR re-runs main-window startup).
 */
export function startMenuOperationGate(): () => void {
  if (stopSync) return stopSync

  // A standalone reactive scope, because this belongs to the window rather than to
  // any component: `DualPaneExplorer` would be the only alternative host, and it's
  // the file we're told not to grow.
  const dispose = $effect.root(() => {
    let lastSent: boolean | null = null
    $effect(() => {
      // Ask Cmdr is not a dialog, so it never reaches the open-dialog set. It
      // counts only while it has FOCUS, ❌ not while it's merely visible: the rail
      // is docked next to the panes most of the time, and blocking on visibility
      // would take copy away from anyone who leaves it open.
      const blocked = blockingSoftDialog() !== null || explorerState.getRailFocused()
      if (blocked === lastSent) return
      lastSent = blocked
      void setFileOperationsBlocked(blocked).catch((err: unknown) => {
        // Nothing to recover: the items stay as they were, and every real guard is
        // elsewhere. Worth a line, since a silent failure here looks like a bug in
        // the menu.
        log.warn("Couldn't update the File menu's enabled state: {error}", { error: err })
      })
    })
  })

  stopSync = () => {
    dispose()
    stopSync = null
  }
  return stopSync
}
