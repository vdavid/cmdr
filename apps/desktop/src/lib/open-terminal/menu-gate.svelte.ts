/**
 * Greys out the File menu's "Open terminal here" while the focused pane sits
 * somewhere no shell can `cd` into: a phone over MTP or ADB, the network browser,
 * a search-results snapshot.
 *
 * ⚠️ This is CHROME, never the guard, for the same two reasons the operation gate
 * isn't one (`routes/(main)/menu-operation-gate.svelte.ts`): a disabled item's
 * accelerator still fires, and the palette reaches the command anyway. The real
 * refusals are the command handler (which words the hint) and Rust's own
 * `not_a_local_path`, which re-reads the volume at launch time and so also catches
 * a share whose mount went away between the push and the keystroke.
 *
 * Main window only: it's the only window with panes, and a second writer would let
 * one window clobber the other's verdict.
 */

import { setOpenTerminalHereEnabled } from '$lib/tauri-commands'
import { getFocusedPaneVolumeId } from '$lib/file-explorer/pane/focused-pane-reads'
import { capabilitiesFor } from '$lib/file-explorer/pane/volume-capabilities'
import { getAppLogger } from '$lib/logging/logger'
import { canOpenTerminalIn } from './terminal-target'

const log = getAppLogger('fileExplorer')

let stopSync: (() => void) | null = null

/**
 * Starts pushing the focused pane's verdict to the native menu, and returns the
 * stop function. Idempotent: a second call keeps the first scope rather than
 * stacking another (HMR re-runs main-window startup).
 */
export function startOpenTerminalMenuGate(): () => void {
  if (stopSync) return stopSync

  // A standalone reactive scope: this belongs to the window, not to a component,
  // and `DualPaneExplorer` is the file we're told not to grow.
  const dispose = $effect.root(() => {
    let lastSent: boolean | null = null
    $effect(() => {
      // The VOLUME's kind, never `capabilitiesForPane`: an archive pane's
      // kind-from-path would hide the drive the archive lives on, and that drive is
      // what decides whether the containing folder is reachable.
      const enabled = canOpenTerminalIn(capabilitiesFor(getFocusedPaneVolumeId()).kind)
      if (enabled === lastSent) return
      lastSent = enabled
      void setOpenTerminalHereEnabled(enabled).catch((err: unknown) => {
        // Nothing to recover: the item stays as it was, and both real refusals are
        // elsewhere. Worth a line, because a silent failure here looks like a bug in
        // the menu.
        log.warn("Couldn't update the File menu's Open terminal here item: {error}", { error: err })
      })
    })
  })

  stopSync = () => {
    dispose()
    stopSync = null
  }
  return stopSync
}
