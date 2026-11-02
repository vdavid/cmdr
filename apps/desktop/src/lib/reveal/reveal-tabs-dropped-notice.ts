/**
 * Saying so when a pane's tab limit turned away some of what the person asked to open.
 *
 * The backend works out how many fit, since the tabs open one at a time
 * (`src-tauri/src/reveal/delivery.rs`). Without this, nine folders opening out of a selection
 * of 12 looks complete.
 */

import type { UnlistenFn } from '@tauri-apps/api/event'
import type { RevealTabsDropped } from '#lib/ipc/bindings.js'
import { MAX_TABS_PER_PANE } from '#lib/file-explorer/tabs/tab-state-manager.svelte.js'
import { tString } from '#lib/intl/messages.svelte.js'
import { onRevealTabsDropped } from '#lib/tauri-commands/index.js'
import { addToast } from '#lib/ui/toast/index.js'

/** Dedup id, so a second run replaces the first notice instead of stacking on it. */
export const REVEAL_TABS_DROPPED_TOAST_ID = 'reveal-tabs-dropped'

/** Subscribes for the life of the main window; subscribe before `drainPendingReveals`. */
export function startRevealTabsDroppedNotice(): Promise<UnlistenFn> {
  return onRevealTabsDropped(showRevealTabsDroppedNotice)
}

/** Exported for its test. */
export function showRevealTabsDroppedNotice({ opened, dropped }: RevealTabsDropped): void {
  addToast(tString('main.revealTabsDropped', { opened, dropped, limit: MAX_TABS_PER_PANE }), {
    level: 'warn',
    dismissal: 'transient',
    id: REVEAL_TABS_DROPPED_TOAST_ID,
  })
}
