/**
 * An open dialog's claim on a native menu command whose accelerator is one of the
 * dialog's own keys: the Multi-rename sheet's F2 (open its Presets menu) is also
 * File > Rename's accelerator.
 *
 * Whether AppKit hands such a key to the webview first or runs the menu item first isn't
 * settled for every key (`routes/(main)/DETAILS.md` § Native-menu and input-focus
 * interactions), and a disabled item's accelerator still fires. So the dialog answers
 * its key on both roads: its own keydown, and this claim, which the dispatch core runs
 * for the command's MENU road ahead of the dialog gate (`command-dispatch.ts`). With
 * one destination, the order stops mattering; the dialog drops the second half of a
 * double fire itself.
 *
 * One claim per command, the newest winning; a dialog claims on mount and releases on
 * destroy.
 */

import type { CommandId } from './command-ids'

const claims = new Map<CommandId, () => void>()

/** Sends `commandId`'s menu road to `run` until the returned release is called. */
export function claimMenuCommand(commandId: CommandId, run: () => void): () => void {
  claims.set(commandId, run)
  return () => {
    // A newer claim (a remount before the old destroy ran) stays.
    if (claims.get(commandId) === run) claims.delete(commandId)
  }
}

/** Runs `commandId`'s claim, if a dialog holds one. `true` when it did. */
export function runMenuClaim(commandId: CommandId): boolean {
  const run = claims.get(commandId)
  if (!run) return false
  run()
  return true
}
