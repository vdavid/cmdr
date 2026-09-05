/**
 * Running "Open terminal here": the first-use picker, the launch, and the three
 * things that can come back other than a plain success.
 *
 * The command handler resolves the folder (`terminal-target.ts`) and hands it
 * here, so this module never touches a pane. Everything it decides on its own is
 * in the pure `first-use-pick.ts`; what's left is the IPC and the wording.
 */

import { asOpenTerminalError, listTerminalApps, openTerminalHere, terminalAppDisplayName } from '$lib/tauri-commands'
import { addToast, dismissToast } from '$lib/ui/toast'
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
 * One id for everything this action says, so a second word replaces the first
 * rather than stacking: they are all about the same folder and the same setting.
 *
 * ❗ An id, ❌ never `toastGroup` + `maxInGroup: 1`. A group already full of
 * PERSISTENT toasts drops the INCOMING one instead of evicting anything
 * (`ui/toast/toast-store.svelte.ts` `makeRoomForNewToast`), and both toasts here
 * are persistent, so "your terminal app is gone" would go unsaid whenever the
 * first-use hint was still on screen, while the setting reset behind it.
 * `status-corner/CLAUDE.md` carries the same warning for its own group.
 */
const TOAST_ID = 'open-terminal-here'

/** Wider than the 360 default, so the hint's two sentences don't run to five lines. */
const TOAST_WIDTH_PX = 400

/**
 * Says one thing, retiring whatever this action said before.
 *
 * The dismiss is what makes the replacement total: `addToast`'s own same-id path
 * swaps the content but keeps the first toast's `props`, `dismissal`, and width,
 * which is the wrong body for a different message.
 */
function replaceToast(content: Parameters<typeof addToast>[0], options: Parameters<typeof addToast>[1]): void {
  dismissToast(TOAST_ID)
  addToast(content, { ...options, id: TOAST_ID })
}

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
    replaceToast(OpenTerminalHintToastContent, {
      level: 'info',
      dismissal: 'persistent',
      widthPx: TOAST_WIDTH_PX,
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
  replaceToast(TerminalAppMissingToastContent, {
    level: 'info',
    dismissal: 'persistent',
    props: { appName },
    widthPx: TOAST_WIDTH_PX,
  })
}

/** A plain one-line toast, retiring whatever this action said before. */
function showMessage(key: Parameters<typeof tString>[0], level: 'info' | 'error'): void {
  replaceToast(tString(key), { level })
}
