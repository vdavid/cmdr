/**
 * The one time the proactive agent interrupts: it noticed something and staged a change for
 * the user to look at.
 *
 * ## Why a toast, and only here
 *
 * A wake runs on its own. The status corner shows one thinking and the suggestions badge shows
 * what is waiting, and both are things the user has to look at to see. When the agent has
 * actually PROPOSED something, that badge going from nothing to something is easy to miss, and
 * the whole feature is worthless if nobody notices. So: one toast, dismissible, gated by
 * `askCmdr.wakeToast`, and nothing at all for a wake that stayed quiet (the backend never emits
 * for zero).
 *
 * ⚠️ **Auto-dismissing on purpose**, unlike the operation-failure toast. Nothing is lost when
 * this one goes away: the proposals sit in the suggestions badge until the user reviews them.
 * A persistent toast for something already surfaced elsewhere is just a thing to close.
 *
 * ⚠️ **Main window only.** `agent-wake-staged` reaches every window, and the settings window
 * would otherwise raise its own copy over its own content. `routes/(main)/window-services.ts`
 * is what scopes it.
 */

import type { UnlistenFn } from '@tauri-apps/api/event'
import { getSetting } from '$lib/settings'
import { onAgentWakeStaged } from '$lib/tauri-commands'
import { addToast } from '$lib/ui/toast'
import WakeStagedToastContent from './WakeStagedToastContent.svelte'

/** Keeps a run of wakes from pushing unrelated toasts off the screen. */
export const WAKE_TOAST_GROUP = 'agent-wake-staged'

/** How many staged-wake toasts can stand at once. Past this the oldest goes: they all say the
 *  same thing, and the suggestions badge is the surface that promises completeness. */
const MAX_IN_GROUP = 2

const toastId = (conversationId: number): string => `agent-wake-staged:${String(conversationId)}`

/**
 * Raise the toast for one staged wake, unless the user turned it off.
 *
 * ⚠️ The setting is read HERE rather than at subscribe time: a user who turns it off while a
 * wake is thinking must not be interrupted by the one already in flight.
 */
export function announceStagedWake(conversationId: number, proposals: number): void {
  if (!getSetting('askCmdr.wakeToast')) return
  addToast(WakeStagedToastContent, {
    id: toastId(conversationId),
    level: 'info',
    toastGroup: WAKE_TOAST_GROUP,
    maxInGroup: MAX_IN_GROUP,
    props: { toastId: toastId(conversationId), conversationId, proposals },
  })
}

let unlisten: UnlistenFn | null = null

/** Start listening. Call once at app init, from the main window only. */
export async function startWakeToast(): Promise<void> {
  if (unlisten) return
  unlisten = await onAgentWakeStaged((staged) => {
    announceStagedWake(staged.conversationId, staged.proposals)
  })
}

export function stopWakeToast(): void {
  if (!unlisten) return
  unlisten()
  unlisten = null
}
