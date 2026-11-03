/**
 * The once-ever "What just happened?" toast for a key that ran its command instead of
 * typing into the quick filter (the typing burst, `quick-filter-typing.ts`).
 *
 * Space selects, `*` inverts the selection, and so on, once the person paused. Someone
 * who meant to type the character sees a command run instead, so this says why and
 * how to type it. It hides by itself after 10 seconds.
 *
 * Seen-flag stamped when the toast goes UP, like `quick-filter-intro.ts`, and visible
 * in Settings > Advanced > Hints and warnings, where turning it off brings the toast back.
 */

import { addToast } from '#lib/ui/toast/index.js'
import { getSetting, setSetting } from '#lib/settings/index.js'
import { getNumberFormatter } from '#lib/intl/number-format.js'
import { normalizeKeyName, toDisplayShortcut } from '#lib/shortcuts/key-capture.js'
import QuickFilterTypingHintToastContent from './QuickFilterTypingHintToastContent.svelte'

/** Dedup id, so the two panes can never stack two. */
export const QUICK_FILTER_TYPING_HINT_TOAST_ID = 'quick-filter-typing-hint'

const TOAST_TIMEOUT_MS = 10_000

/** Raises the toast for `char` unless it was ever shown. */
export function maybeShowQuickFilterTypingHint(char: string, windowMs: number): void {
  if (getSetting('fileExplorer.quickFilterTypingHintSeen')) return
  setSetting('fileExplorer.quickFilterTypingHintSeen', true)
  const seconds = windowMs / 1000
  addToast(QuickFilterTypingHintToastContent, {
    level: 'info',
    dismissal: 'transient',
    timeoutMs: TOAST_TIMEOUT_MS,
    id: QUICK_FILTER_TYPING_HINT_TOAST_ID,
    props: {
      // A bare space would show as nothing, so keys go by their display name (`Space`).
      keyLabel: toDisplayShortcut(normalizeKeyName(char)),
      seconds,
      secondsText: getNumberFormatter({ maximumFractionDigits: 1 }).format(seconds),
    },
  })
}
