/**
 * Saying so when a reveal from another app brought Cmdr to the front but couldn't show what it
 * was asked for (#1020). The window is up either way (`src-tauri/src/reveal/delivery.rs`), so
 * without this the person looks at the wrong folder with nothing to say why.
 *
 * The backend names the item and its folder and types the reason; this only words it.
 */

import type { UnlistenFn } from '@tauri-apps/api/event'
import type { RevealNotShown } from '#lib/ipc/bindings.js'
import { tString } from '#lib/intl/messages.svelte.js'
import { onRevealNotShown } from '#lib/tauri-commands/index.js'
import { addToast } from '#lib/ui/toast/index.js'

/** Dedup id, so a second miss replaces the first notice instead of stacking on it. */
export const REVEAL_NOT_SHOWN_TOAST_ID = 'reveal-not-shown'

/** Subscribes for the life of the main window; subscribe before `drainPendingReveals`. */
export function startRevealNotShownNotice(): Promise<UnlistenFn> {
  return onRevealNotShown(showRevealNotShownNotice)
}

/** Exported for its test. */
export function showRevealNotShownNotice({ name, folder, reason }: RevealNotShown): void {
  addToast(revealNotShownMessage({ name, folder, reason }), {
    level: 'warn',
    dismissal: 'transient',
    id: REVEAL_NOT_SHOWN_TOAST_ID,
  })
}

function revealNotShownMessage({ name, folder, reason }: RevealNotShown): string {
  switch (reason) {
    case 'notAnswering':
      return tString('main.revealNotShown.notAnswering', { name })
    case 'folderNotOpened':
      // A folder move names the folder as the item, and "open Photos to show Photos" reads odd.
      return name === folder
        ? tString('main.revealNotShown.folderItselfNotOpened', { name })
        : tString('main.revealNotShown.folderNotOpened', { folder, name })
    case 'notInFolder':
      return tString('main.revealNotShown.notInFolder', { folder, name })
  }
}
