/**
 * Reactive controller for the Enter-behavior popup (`lib/ui/Menu` shown when an
 * archive/bundle set to Ask is opened). Holds the menu's open/anchor/highlight
 * `$state` and orchestrates a choice — browse, open, or deep-link to Settings —
 * so FilePane only renders `<Menu>` bound to this and calls `openFor` from its
 * navigate fork. Pure decision logic stays in `archive-enter-policy.ts`; item
 * building and anchoring in `enter-menu.ts`.
 *
 * Focus note: the menu is portaled to `document.body` (escaping the explorer's
 * `onfocusin` guard, which would otherwise yank focus off a `role="menu"` inside
 * the container). On close we call `restoreFocus` so the explorer container
 * regains DOM focus and keyboard routing resumes.
 */

import type { FileEntry } from '$lib/file-explorer/types'
import type { MenuItem } from '$lib/ui/menu-types'
import { openSettingsWindow } from '$lib/settings/settings-window'
import type { EnterAction } from './archive-enter-policy'
import { buildEnterMenuItems, enterMenuAnchor, enterMenuHighlight, type EnterMenuChoice } from './enter-menu'

export interface EnterMenuDeps {
  /** The pane's root element, for anchoring the menu at the cursor row. */
  getPaneElement: () => HTMLElement | null
  /** Step into the archive/bundle like a folder. */
  browse: (entry: FileEntry) => void
  /** Hand the archive/bundle to its default app (LaunchServices). */
  open: (entry: FileEntry) => void
  /** Return DOM focus to the explorer container after the menu closes. */
  restoreFocus: () => void
}

export interface EnterMenuController {
  readonly open: boolean
  readonly anchorPoint: { x: number; y: number } | null
  readonly highlight: EnterMenuChoice
  readonly items: MenuItem[]
  /** Open the popup for an entry; `action` (the resolved policy) picks the lead row. */
  openFor: (entry: FileEntry, action: EnterAction) => void
  onOpenChange: (open: boolean) => void
  onSelect: (value: string) => void
}

export function createEnterMenu(deps: EnterMenuDeps): EnterMenuController {
  let open = $state(false)
  let anchorPoint = $state<{ x: number; y: number } | null>(null)
  let highlight = $state<EnterMenuChoice>('browse')
  // Not reactive: read only inside `onSelect`, never rendered.
  let pendingEntry: FileEntry | null = null

  return {
    get open() {
      return open
    },
    get anchorPoint() {
      return anchorPoint
    },
    get highlight() {
      return highlight
    },
    get items() {
      // Rebuilt per open so a live locale switch is reflected (cheap: three items).
      return buildEnterMenuItems()
    },
    openFor(entry, action) {
      pendingEntry = entry
      anchorPoint = enterMenuAnchor(deps.getPaneElement())
      highlight = enterMenuHighlight(action)
      open = true
    },
    onOpenChange(next) {
      open = next
      // Escape, outside-click, or post-select close: hand focus back to the pane.
      if (!next) deps.restoreFocus()
    },
    onSelect(value) {
      const entry = pendingEntry
      open = false
      if (!entry) return
      if (value === 'browse') deps.browse(entry)
      else if (value === 'open') deps.open(entry)
      else if (value === 'configure') void openSettingsWindow(['Behavior', 'Archives'])
    },
  }
}
