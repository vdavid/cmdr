/**
 * What the first quick filter toast's buttons do (`quick-filter-intro.ts` raises
 * it). Apart from the raise so `QuickFilterIntroToastContent.svelte` can import
 * them without importing itself back.
 */

import { dismissToast } from '$lib/ui/toast'
import { setSetting } from '$lib/settings'
import { settingAnchorId } from '$lib/settings/settings-window'
import { getAppLogger } from '$lib/logging/logger'

const log = getAppLogger('fileExplorer')

/** Dedup id of the toast, so the two panes can never stack two. */
export const QUICK_FILTER_INTRO_TOAST_ID = 'quick-filter-intro'

/** "Switch to Jump": typing jumps from now on, and the pane's filter clears with the mode. */
export function switchToJumpMode(): void {
  setSetting('fileExplorer.typeToJump.mode', 'jump')
  dismissToast(QUICK_FILTER_INTRO_TOAST_ID)
}

/** "Settings…": the typing-mode row in Settings > Appearance > Listing. */
export async function openTypingModeSettings(): Promise<void> {
  dismissToast(QUICK_FILTER_INTRO_TOAST_ID)
  try {
    const { openSettingsWindow } = await import('$lib/settings/settings-window')
    await openSettingsWindow(
      'quick-filter-toast',
      ['Appearance', 'Listing'],
      settingAnchorId('fileExplorer.typeToJump.mode'),
    )
  } catch (err) {
    log.warn("Couldn't open Settings from the quick filter toast: {err}", { err: String(err) })
  }
}
