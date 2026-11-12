/**
 * Tells the native menu which commands the dispatch core would refuse right now: every command
 * declared `BLOCKED_BY_DIALOGS`, while a dialog, an explorer overlay, or the command palette is up in
 * the main window, and none once it's gone.
 *
 * ⚠️ For the regular items this is CHROME, never the guard: a disabled item's accelerator still
 * fires, and the dispatch core refuses those commands itself (`dialog-command-gate.ts`). The two
 * check items (show hidden files, the view modes) are the exception. They toggle themselves before
 * the frontend hears of the click, so Rust reverts a refused one from this same list
 * (`menu/menu_handlers.rs`).
 *
 * The text-input-only commands (cut, copy, paste, select all) are never on the list: the menu can't
 * see focus, and those items forward the native edit actions in Settings and the viewer.
 *
 * The same bit also takes the bar's mnemonics away while something is up (`setMenuMnemonicsSuspended`):
 * on Linux GTK takes a bar title's `Alt+<letter>` before the page sees it, so the search dialog's
 * `Alt+F` would open the File menu. Not chrome: without it those keys never reach the dialog.
 *
 * Main window only, for the same reason as `menu-operation-gate.svelte.ts`: one writer.
 */

import { commands } from '#lib/commands/index.js'
import { setCommandsRefusedOverDialog, setMenuMnemonicsSuspended } from '#lib/tauri-commands/index.js'
import { getAppLogger } from '#lib/logging/logger.js'
import type { DialogsOnScreen } from './command-dispatch-context'

const log = getAppLogger('mainListeners')

/** The commands a dialog refuses outright, in registry order. */
const REFUSED_OVER_DIALOGS: readonly string[] = commands
  .filter((command) => command.whileDialogOpen.runs === 'never')
  .map((command) => command.id)

let stopSync: (() => void) | null = null

/**
 * Starts pushing the refused commands to the native menu, and returns the stop function.
 * Idempotent: calling it twice keeps the first scope rather than stacking a second one (HMR
 * re-runs the window's startup).
 */
export function startMenuDialogGate(getDialogsOnScreen: () => DialogsOnScreen): () => void {
  if (stopSync) return stopSync

  const dispose = $effect.root(() => {
    let lastSent: boolean | null = null
    $effect(() => {
      const { dialogOpen, paletteOpen } = getDialogsOnScreen()
      const somethingOpen = dialogOpen || paletteOpen
      if (somethingOpen === lastSent) return
      lastSent = somethingOpen
      void setCommandsRefusedOverDialog(somethingOpen ? [...REFUSED_OVER_DIALOGS] : []).catch((err: unknown) => {
        // Nothing to recover: the items stay as they were, and the dispatch core refuses the
        // commands anyway. Worth a line, since a silent failure here looks like a menu bug.
        log.warn("Couldn't update which menu items a dialog greys out: {error}", { error: err })
      })
      // The bit itself, sent on its own: what the mask follows is "something modal is up", which
      // the refused list only mirrors while `REFUSED_OVER_DIALOGS` happens to be non-empty.
      void setMenuMnemonicsSuspended(somethingOpen).catch((err: unknown) => {
        log.warn("Couldn't switch the menu bar's mnemonics for a dialog: {error}", { error: err })
      })
    })
  })

  stopSync = () => {
    dispose()
    stopSync = null
  }
  return stopSync
}
