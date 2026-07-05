/**
 * Reactive controller for the Enter-behavior popup (`lib/ui/Menu` shown when an
 * archive/bundle set to Ask is opened). Holds the menu's open/anchor/highlight
 * `$state` and orchestrates a choice — browse, open, or deep-link to Settings —
 * so FilePane only renders `<Menu>` bound to this and calls `openFor` from its
 * navigate fork. Pure decision logic stays in `archive-enter-policy.ts`; item
 * building and anchoring in `enter-menu.ts`.
 *
 * Keyboard model: the popup (`lib/ui/Menu`) is portaled to `document.body` and
 * focuses itself on mount, so it owns the keyboard and its keys can't race back to
 * the pane. Its `onKeydown` calls `handleKey` here — arrows move the highlight,
 * Enter/Space select, Escape closes. The menu owns rendering, positioning, pointer
 * selection, and outside-click dismissal; `setHighlighted` syncs the highlight on
 * pointer hover.
 */

import type { FileEntry } from '$lib/file-explorer/types'
import type { MenuItem } from '$lib/ui/menu-types'
import { openSettingsWindow } from '$lib/settings/settings-window'
import type { EnterAction } from './archive-enter-policy'
import { buildEnterMenuItems, enterMenuAnchor, enterMenuHighlight } from './enter-menu'

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
  readonly highlighted: string | null
  readonly items: MenuItem[]
  /** Open the popup for an entry; `action` (the resolved policy) picks the lead row. */
  openFor: (entry: FileEntry, action: EnterAction) => void
  onOpenChange: (open: boolean) => void
  onSelect: (value: string) => void
  /** Sync the highlight on pointer hover (from Ark). */
  setHighlighted: (value: string | null) => void
  /** Route a keydown while the menu is open; returns true when it consumed the key. */
  handleKey: (event: KeyboardEvent) => boolean
  /** Detach the document listener (call from the host's teardown). */
  dispose: () => void
}

export function createEnterMenu(deps: EnterMenuDeps): EnterMenuController {
  let open = $state(false)
  let anchorPoint = $state<{ x: number; y: number } | null>(null)
  let highlighted = $state<string | null>(null)
  // Not reactive: read only inside `select`, never rendered.
  let pendingEntry: FileEntry | null = null

  // A document-level capture listener, live only while the menu is open, catches
  // keydowns regardless of where focus landed (the pane, or the portaled menu) and
  // routes them to `handleKey`, which stops them from reaching the pane's own
  // navigation. This is what makes keyboard nav deterministic — focus timing (the
  // menu autofocuses on mount) can't race the keys.
  let keyListenerAttached = false
  function onDocumentKeydown(event: KeyboardEvent): void {
    controller.handleKey(event)
  }
  function attachKeyListener(): void {
    if (keyListenerAttached || typeof document === 'undefined') return
    document.addEventListener('keydown', onDocumentKeydown, true)
    keyListenerAttached = true
  }
  function detachKeyListener(): void {
    if (!keyListenerAttached || typeof document === 'undefined') return
    document.removeEventListener('keydown', onDocumentKeydown, true)
    keyListenerAttached = false
  }

  function close(): void {
    open = false
    detachKeyListener()
    deps.restoreFocus()
  }

  function select(value: string): void {
    const entry = pendingEntry
    open = false
    detachKeyListener()
    deps.restoreFocus()
    if (!entry) return
    if (value === 'browse') deps.browse(entry)
    else if (value === 'open') deps.open(entry)
    else if (value === 'configure') void openSettingsWindow(['Behavior', 'Archives'])
  }

  const controller: EnterMenuController = {
    get open() {
      return open
    },
    get anchorPoint() {
      return anchorPoint
    },
    get highlighted() {
      return highlighted
    },
    get items() {
      // Rebuilt per open so a live locale switch is reflected (cheap: three items).
      return buildEnterMenuItems()
    },
    openFor(entry, action) {
      pendingEntry = entry
      anchorPoint = enterMenuAnchor(deps.getPaneElement())
      highlighted = enterMenuHighlight(action)
      open = true
      attachKeyListener()
    },
    onOpenChange(next) {
      const wasOpen = open
      open = next
      // Outside-click close: detach and hand focus back to the pane, on the transition.
      if (wasOpen && !next) {
        detachKeyListener()
        deps.restoreFocus()
      }
    },
    onSelect(value) {
      // Pointer selection. Keyboard selection goes through `handleKey`.
      select(value)
    },
    setHighlighted(value) {
      if (value !== null) highlighted = value
    },
    handleKey(event) {
      if (!open) return false
      const values = buildEnterMenuItems().map((item) => item.value)
      if (values.length === 0) return false
      const idx = values.indexOf(highlighted ?? '')
      const clampMove = (next: number) => {
        event.preventDefault()
        event.stopPropagation()
        highlighted = values[Math.max(0, Math.min(values.length - 1, next))]
        return true
      }
      switch (event.key) {
        case 'ArrowDown':
          return clampMove(idx < 0 ? 0 : idx + 1)
        case 'ArrowUp':
          return clampMove(idx < 0 ? values.length - 1 : idx - 1)
        case 'Home':
          return clampMove(0)
        case 'End':
          return clampMove(values.length - 1)
        case 'Enter':
        case ' ':
          event.preventDefault()
          event.stopPropagation()
          if (highlighted !== null) select(highlighted)
          return true
        case 'Escape':
          event.preventDefault()
          event.stopPropagation()
          close()
          return true
        default:
          return false
      }
    },
    dispose() {
      detachKeyListener()
    },
  }
  return controller
}
