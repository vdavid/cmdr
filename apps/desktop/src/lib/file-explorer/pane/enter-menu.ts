/**
 * Presentation glue for the Enter-behavior popup (the Browse | Open | Configure
 * menu shown when an archive/bundle is set to Ask). The decision logic is the pure
 * `archive-enter-policy.ts`; this file only builds the menu's items, resolves where
 * to anchor it, and picks the row highlighted on open.
 */

import type { MenuItem } from '$lib/ui/menu-types'
import { tString } from '$lib/intl/messages.svelte'
import type { EnterAction } from './archive-enter-policy'

/** The menu's item values (also what `onSelect` emits). */
export type EnterMenuChoice = 'browse' | 'open' | 'configure'

/** The three rows, in order: browse, open, then the settings deep-link. */
export function buildEnterMenuItems(): MenuItem[] {
  return [
    { value: 'browse', label: tString('fileExplorer.archiveEnterMenu.browse') },
    { value: 'open', label: tString('fileExplorer.archiveEnterMenu.open') },
    { value: 'configure', label: tString('fileExplorer.archiveEnterMenu.configure') },
  ]
}

/**
 * The row highlighted when the menu opens. When a format resolves to `open` we
 * lead with Open; otherwise we lead with Browse (the headline action and the safe
 * default for an `ask` format, which has no browse/open preference of its own).
 */
export function enterMenuHighlight(action: EnterAction): EnterMenuChoice {
  return action === 'open' ? 'open' : 'browse'
}

/**
 * A viewport point to anchor the menu at: just below the left edge of the cursor
 * row (found via its `aria-selected` marker within the pane). Falls back to the
 * pane's center, then to `null` (Ark then anchors at the origin) — both only when
 * the row can't be located, which shouldn't happen for a real Enter.
 */
export function enterMenuAnchor(paneEl: HTMLElement | null): { x: number; y: number } | null {
  const row = paneEl?.querySelector('[aria-selected="true"]')
  if (row) {
    const rect = row.getBoundingClientRect()
    return { x: rect.left + 16, y: rect.bottom }
  }
  if (paneEl) {
    const rect = paneEl.getBoundingClientRect()
    return { x: rect.left + rect.width / 2, y: rect.top + rect.height / 2 }
  }
  return null
}
