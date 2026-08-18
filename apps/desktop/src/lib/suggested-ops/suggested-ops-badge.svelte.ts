/**
 * What the status corner shows: how many suggestions are waiting.
 *
 * Its own module rather than part of the dialog's state, because the badge is mounted for the
 * whole session while the dialog's state only exists while a review is open. Folding the two
 * would make the corner depend on a store nothing populates until the user opens the dialog,
 * which is how you ship an indicator that sits at zero forever.
 *
 * **Subscribed, never polled.** `suggestions-changed` carries the counts, so being told is the
 * only thing that moves this. One read at startup seeds it, because a suggestion made in a
 * previous session is waiting before any event fires.
 */

import type { UnlistenFn } from '@tauri-apps/api/event'
import { getAppLogger } from '$lib/logging/logger'
import { listSuggestedOps, onSuggestionsChanged } from '$lib/tauri-commands'

const log = getAppLogger('suggestedOps')

interface BadgeState {
  /** Groups waiting on the user. The badge hides at zero. */
  pendingGroupCount: number
  /** Ops those groups hold between them, for the tooltip. */
  pendingOpCount: number
}

export const suggestedOpsBadge = $state<BadgeState>({ pendingGroupCount: 0, pendingOpCount: 0 })

let unlisten: UnlistenFn | null = null

/**
 * Seed the badge and subscribe. Call once at app init.
 *
 * The seed matters on its own: suggestions have no expiry, so a group proposed last week is
 * waiting before this session emits anything.
 */
export async function startSuggestedOpsBadge(): Promise<void> {
  if (unlisten) return
  unlisten = await onSuggestionsChanged((payload) => {
    suggestedOpsBadge.pendingGroupCount = payload.pendingGroupCount
    suggestedOpsBadge.pendingOpCount = payload.pendingOpCount
  })
  await seedFromStore()
}

/** Read the waiting set once, for the counts no event will announce. */
async function seedFromStore(): Promise<void> {
  try {
    const sweeps = await listSuggestedOps()
    const groups = sweeps.flatMap((sweep) => sweep.groups)
    suggestedOpsBadge.pendingGroupCount = groups.length
    suggestedOpsBadge.pendingOpCount = groups.reduce((total, group) => total + group.liveOpCount, 0)
  } catch (e) {
    // A badge that can't be seeded stays at zero rather than guessing. The next event corrects
    // it, and nothing the user does depends on this number being right to the second.
    log.warn("Couldn't seed the suggestions badge: {error}", { error: String(e) })
  }
}

export function stopSuggestedOpsBadge(): void {
  if (!unlisten) return
  unlisten()
  unlisten = null
}
