/**
 * Get info: ask Finder for its Get Info window, or say plainly why it didn't open.
 *
 * Rust reads macOS's stored Automation answer first, so a user who once said "Don't
 * Allow" gets a toast with the way back instead of a key press that does nothing.
 * The first-ever press needs nothing from here: macOS shows its own prompt.
 */

import { addToast } from '$lib/ui/toast'
import { tString } from '$lib/intl/messages.svelte'
import { asGetInfoError, getInfo } from '$lib/tauri-commands'
import FinderAutomationOffToastContent from './FinderAutomationOffToastContent.svelte'

/** One id, so pressing Get info again replaces the toast rather than stacking it. */
const TOAST_ID = 'get-info'

/**
 * Opens Finder's Get Info window for `path`, or tells the user why it can't.
 *
 * Never throws: every refusal becomes a toast, and the caller is a fire-and-forget
 * command handler with nobody to hand a rejection to.
 */
export async function openGetInfoOrExplain(path: string): Promise<void> {
  try {
    await getInfo(path)
  } catch (error) {
    const refusal = asGetInfoError(error)
    if (refusal?.type === 'automationDenied') {
      addToast(FinderAutomationOffToastContent, { id: TOAST_ID, level: 'warn', dismissal: 'persistent' })
      return
    }
    addToast(tString('commands.handler.getInfo.notReached'), { id: TOAST_ID, level: 'error' })
  }
}
