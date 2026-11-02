/**
 * The ONE way the explorer puts DOM focus back home: on the focused pane's file list, which
 * names its cursor row through `aria-activedescendant` so a screen reader follows the cursor,
 * or on the explorer container when that pane shows no list (the servers hub, an error, a
 * connect view). Key routing never depends on which of the two holds focus: keys bubble to
 * the container's handlers and go to the store's focused pane either way.
 *
 * ❌ Never focus a file row: rows unmount under virtualization, and focus on a removed node
 * drops to `<body>`, where no pane key arrives. Lists focus with `preventScroll` (the listbox
 * is taller than its scroller, so a plain `.focus()` scrolls the view off the cursor).
 */

import { focusMayFollowThePanes } from '#lib/ui/focus-restore.js'
import type { FilePaneAPI } from './types'

export interface ActivePaneFocusDeps {
  getPaneRef: (pane: 'left' | 'right') => FilePaneAPI | undefined
  getFocusedPane: () => 'left' | 'right'
  getContainerElement: () => HTMLElement | undefined
}

/** Focuses the focused pane's list, else the explorer container. */
export function focusActivePane(deps: ActivePaneFocusDeps): void {
  if (deps.getPaneRef(deps.getFocusedPane())?.focusList() === true) return
  deps.getContainerElement()?.focus()
}

/**
 * `focusActivePane` for when the panes moved and nobody at the keyboard said to take focus there: a
 * tool's `switch_pane`, `swap_panes`, or `move_cursor`, or a load finishing on its own. Focus stays put
 * wherever the person has it (`focusMayFollowThePanes`). The store's focused pane still moves.
 */
export function followFocusedPane(deps: ActivePaneFocusDeps): void {
  if (focusMayFollowThePanes()) focusActivePane(deps)
}
