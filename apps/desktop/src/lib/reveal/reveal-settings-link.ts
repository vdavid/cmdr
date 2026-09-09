/**
 * The one way back to the "Show in Finder" switch, shared by everything that
 * points at it.
 *
 * The anchor id is knowledge the card and its callers both need, so it lives
 * here rather than being spelled out at either end.
 */

import { getAppLogger } from '$lib/logging/logger'

const log = getAppLogger('reveal')

/**
 * DOM id of the "Show in Finder" card in Settings.
 *
 * ❗ Not `settingAnchorId(...)`: the switch is an OS-backed row, and there's no
 * `SettingId` to derive an anchor out of. See `settings/DETAILS.md` § OS-backed
 * rows. So the card carries this id itself.
 */
export const REVEAL_HANDLER_ANCHOR_ID = 'settings-reveal-handler'

/**
 * Deep-links to **Settings > Behavior > Navigation & file ops**, scrolled to the
 * "Show in Finder" card.
 *
 * ❗ The activation notice offers this and ❌ never a "Turn it off" button:
 * switching this back should cost one more click, in the place the setting
 * actually lives, so nobody undoes it by reflex from a toast.
 *
 * ❗ `settings-window` is imported LAZILY, ❌ never statically. `RevealHandlerCard`
 * imports the anchor id above at module scope, and a static import would drag
 * the command registry into the card's module graph — where `file-list.ts` calls
 * `isMacOS()` at eval time and blows up under a test that mocks it. Same reason
 * `ShortcutChip` reaches for `openShortcutCustomization` this way.
 *
 * Swallows a failure into a log line: a window that won't open is not worth
 * throwing out of a toast button, and the toast has already said its piece.
 */
export async function openSettingsToRevealHandler(): Promise<void> {
  try {
    const { openSettingsWindow } = await import('$lib/settings/settings-window')
    await openSettingsWindow('reveal-toast', ['Behavior', 'Navigation & file ops'], REVEAL_HANDLER_ANCHOR_ID)
  } catch (err) {
    log.warn('Could not open Settings from the reveal notice: {err}', { err: String(err) })
  }
}
