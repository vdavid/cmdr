/**
 * What the tab bar reports to analytics, as pure vocabulary.
 *
 * PII-free by construction: a tab's whole identity is a path, and nothing in this
 * file can see one. What crosses is an action token, a count of open tabs, and two
 * bools (`apps/desktop/src-tauri/src/analytics/CLAUDE.md`).
 *
 * The question the props exist to answer is "do people actually live in tabs, or
 * open one and forget them?", so every event carries how many are open at the
 * moment it fires — the count is the interesting part, and the gesture is the
 * context for it.
 *
 * The emitters live here rather than at each call site so the vocabulary can't
 * drift; `tab-operations.ts` is the one layer every trigger passes through (the
 * tab bar, the File menu, the keyboard, the command palette, and the MCP `tab`
 * tool all funnel into its exports), so its exports are where they're called.
 */

import { trackEvent } from '$lib/tauri-commands'

/** How a tab came to exist. */
export type TabOpenSource = 'new' | 'reopened'

/** How a tab (or a set of them) was asked to close. */
export type TabCloseSource = 'single' | 'others'

/**
 * How an open attempt ended. The two refusals are counted for the same reason
 * `search_cta_offered` exists: a bare success count can't tell "nobody reopens
 * tabs" from "everybody hits the ten-tab cap trying".
 */
export type TabOpenOutcome = 'opened' | 'atCap' | 'nothingToReopen'

/** How a close attempt ended. `lastTab` closes the window instead. */
export type TabCloseOutcome = 'closed' | 'cancelled' | 'lastTab'

/** Which gesture moved the active tab. */
export type TabSwitchMethod = 'cycle' | 'pick'

/**
 * Reports a tab open attempt.
 *
 * `openTabs` is the RAW count, not an `item_count_bucket`: a pane caps at ten
 * tabs, and that bucketing puts the entire range into two values (`1` and
 * `2-10`), which throws the answer away for no privacy gain. Ten possible
 * integers is low cardinality and identifies nobody.
 */
export function reportTabOpened(source: TabOpenSource, outcome: TabOpenOutcome, openTabs: number): void {
  void trackEvent('tab_opened', { source, outcome, open_tabs: openTabs })
}

/** Reports a tab close attempt. `pinned` is whether the target tab was pinned. */
export function reportTabClosed(
  source: TabCloseSource,
  outcome: TabCloseOutcome,
  openTabs: number,
  pinned: boolean,
): void {
  void trackEvent('tab_closed', { source, outcome, open_tabs: openTabs, pinned })
}

/** Reports a move of the active tab. */
export function reportTabSwitched(method: TabSwitchMethod): void {
  void trackEvent('tab_switched', { method })
}

/** Reports a pin toggle. `pinned` is the state the tab ends in. */
export function reportTabPinToggled(pinned: boolean): void {
  void trackEvent('tab_pin_toggled', { pinned })
}
