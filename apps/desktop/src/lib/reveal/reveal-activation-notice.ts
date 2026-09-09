/**
 * Saying so, once, the first time a reveal from another app actually lands here.
 *
 * ❗ Cause and effect are days apart in this feature: someone switches it on,
 * then a week later clicks "Show in folder" in Chrome and an app they didn't
 * invoke jumps in front of them. Without this the first landing reads as Cmdr
 * misbehaving. After the first one it reads as the feature working, so the
 * notice is once-ever.
 *
 * ❌ Not a nudge: it explains something the person set up themselves, so it takes
 * no part in the shared nudge cooldown (`$lib/nudges/`) and answers to its own
 * `behavior.revealActivationNoticeSeen` flag.
 */

import type { UnlistenFn } from '@tauri-apps/api/event'
import { addToast } from '$lib/ui/toast'
import { getSetting, setSetting } from '$lib/settings'
import { onRevealDelivered } from '$lib/tauri-commands'
import { getAppLogger } from '$lib/logging/logger'
import RevealActivationToastContent from './RevealActivationToastContent.svelte'

const log = getAppLogger('reveal')

/** Dedup id, so two reveals in the same second can't stack two notices. */
export const REVEAL_ACTIVATION_TOAST_ID = 'reveal-activation'

/**
 * Long enough to read two sentences and reach for the link, short enough that
 * the pane the reveal just landed in isn't covered while someone works in it.
 * ❗ Transient, ❌ never persistent: nothing here needs an answer.
 */
export const REVEAL_ACTIVATION_TOAST_MS = 10_000

/**
 * Subscribes for the life of the main window. The listener costs nothing on a
 * machine where reveals never come, and the flag is read per event rather than
 * at startup, so the notice can't be armed by a stale snapshot.
 *
 * The backend only announces a reveal that actually moved a pane, and a
 * cold-launch reveal is parked until the frontend drains it, so this survives
 * the cold path as long as it's subscribed before that drain
 * (`routes/(main)/window-services.ts`, phase 2).
 */
export function startRevealActivationNotice(): Promise<UnlistenFn> {
  return onRevealDelivered(showRevealActivationNoticeOnce)
}

/** Raises the notice the first time, then never again. Exported for its test. */
export function showRevealActivationNoticeOnce(): void {
  if (getSetting('behavior.revealActivationNoticeSeen')) return
  // Spent as the toast goes UP, matching every other once-ever notice: a crash
  // between the two costs one reading, where the other order risks repeating it.
  setSetting('behavior.revealActivationNoticeSeen', true)
  log.info('A reveal landed here for the first time; saying so once')
  addToast(RevealActivationToastContent, {
    level: 'info',
    dismissal: 'transient',
    timeoutMs: REVEAL_ACTIVATION_TOAST_MS,
    id: REVEAL_ACTIVATION_TOAST_ID,
  })
}
