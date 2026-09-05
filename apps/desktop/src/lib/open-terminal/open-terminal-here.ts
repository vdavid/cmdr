/**
 * Running "Open terminal here": the first-use picker, the launch, and the three
 * things that can come back other than a plain success.
 *
 * The command handler resolves the folder (`terminal-target.ts`) and hands it
 * here, so this module never touches a pane. Everything it decides on its own is
 * in the pure `first-use-pick.ts`; what's left is the IPC and the wording.
 */

import { asOpenTerminalError, listTerminalApps, openTerminalHere, terminalAppDisplayName } from '$lib/tauri-commands'
import { addToast } from '$lib/ui/toast'
import { tString } from '$lib/intl/messages.svelte'
import { TERMINAL_APP_BUNDLE_ID } from '$lib/settings/sections/terminal-app-options'
import { decideFirstUsePick } from './first-use-pick'
import {
  getTerminalAppChoice,
  getTerminalHintSeen,
  markTerminalHintSeen,
  setTerminalAppChoice,
} from './terminal-app-setting'
import OpenTerminalHintToastContent from './OpenTerminalHintToastContent.svelte'
import TerminalAppMissingToastContent from './TerminalAppMissingToastContent.svelte'

/** What the command handler resolved from the focused pane. */
export interface OpenTerminalRequest {
  /** The folder to open, or `null` when this pane has none a shell can reach. */
  folder: string | null
  /** The pane's volume id, so Rust can take the same path-less reading again. */
  volumeId: string
}

/**
 * Both hint toasts share a one-slot group: they say the same kind of thing about
 * the same setting, so a second one replaces the first rather than stacking.
 */
const TOAST_GROUP = 'open-terminal-here'

/** Wider than the 360 default, so the hint's two sentences don't run to five lines. */
const TOAST_WIDTH_PX = 400

/**
 * Opens the folder, dealing with everything that can happen on the way.
 *
 * Never throws: every outcome the user should know about becomes a toast, and the
 * caller is a fire-and-forget command handler with nobody to hand a rejection to.
 */
export async function openTerminalHereForFolder({ folder, volumeId }: OpenTerminalRequest): Promise<void> {
  if (folder === null) {
    // The pane's own gate said no. The menu item is already greyed out here; this
    // covers the shortcut and the palette, which reach the command anyway.
    showMessage('commands.handler.openTerminalHere.noPath', 'info')
    return
  }

  const appChoice = await pickAppForThisRun()

  try {
    const outcome = await openTerminalHere(folder, volumeId, appChoice)
    if (outcome === 'app_missing_opened_terminal_instead') {
      await reportMissingApp(appChoice)
    } else if (outcome === 'not_a_local_path') {
      // Rust took the same reading and disagreed: the pane's volume kind still says
      // local, but the mount behind it is gone (a share that went away).
      showMessage('commands.handler.openTerminalHere.noPath', 'info')
    }
  } catch (error) {
    const refusal = asOpenTerminalError(error)
    showMessage(
      refusal?.type === 'timedOut'
        ? 'commands.handler.openTerminalHere.timedOut'
        : 'commands.handler.openTerminalHere.launchRefused',
      'error',
    )
  }
}

/**
 * The app this run launches, running the first-use picker when the hint is still
 * unspent.
 *
 * The list query only happens while the hint is due: after that the stored choice
 * is the whole answer, and asking macOS which terminals exist would buy nothing.
 */
async function pickAppForThisRun(): Promise<string> {
  const storedChoice = getTerminalAppChoice()
  if (getTerminalHintSeen()) return storedChoice

  const list = await listTerminalApps(storedChoice)
  const pick = decideFirstUsePick({ storedChoice, hintSeen: false, apps: list.data.apps })

  if (pick.persistChoice) setTerminalAppChoice(pick.appChoice)
  if (pick.markSeen) markTerminalHintSeen()
  if (pick.showHint) {
    addToast(OpenTerminalHintToastContent, {
      level: 'info',
      dismissal: 'persistent',
      widthPx: TOAST_WIDTH_PX,
      toastGroup: TOAST_GROUP,
      maxInGroup: 1,
    })
  }
  return pick.appChoice
}

/**
 * Says which app went missing and puts the setting back to Terminal, so the next
 * run opens the same thing without saying it again.
 */
async function reportMissingApp(appChoice: string): Promise<void> {
  // Asked BEFORE the reset, while the setting still names the app that's gone.
  const appName = await terminalAppDisplayName(appChoice)
  setTerminalAppChoice(TERMINAL_APP_BUNDLE_ID)
  addToast(TerminalAppMissingToastContent, {
    level: 'info',
    dismissal: 'persistent',
    props: { appName },
    widthPx: TOAST_WIDTH_PX,
    toastGroup: TOAST_GROUP,
    maxInGroup: 1,
  })
}

/** A plain one-line toast, grouped with the rest so a burst can't stack. */
function showMessage(key: Parameters<typeof tString>[0], level: 'info' | 'error'): void {
  addToast(tString(key), { level, toastGroup: TOAST_GROUP, maxInGroup: 1 })
}
