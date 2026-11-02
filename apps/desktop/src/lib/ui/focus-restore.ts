/**
 * Putting focus back where it was when an overlay (a dialog, a menu, the command palette,
 * a toast) closes.
 *
 * Every overlay used to remember `document.activeElement` and call a bare `.focus()` on it.
 * In the main window that element is the focused pane's file list, and a bare `.focus()`
 * there goes wrong two ways: it lacks `preventScroll` (the listbox is taller than its
 * scroller, so WebKit scrolls the view off the cursor), and the remembered list may belong to
 * the pane that's no longer focused (the palette's "Switch pane", ⌥F1 → `Tab`), leaving DOM
 * focus and the store on different panes. So focus that started inside the registered HOME
 * (the explorer) goes back through the home's own refocus, which picks the focused pane's
 * list; anything else is focused directly.
 *
 * ❗ Focus that already moved into another open menu or dialog stays there: closing the old
 * volume chooser after `Tab` re-opened it on the other pane would otherwise steal focus from
 * the new one (2026-10-15 VoiceOver spike: the moved chooser stayed open but unfocused).
 */

import { isFocusInOpenMenu } from './menu-controller.svelte'

/** The window's keyboard home: the explorer in the main window. */
export interface FocusHome {
  root: () => HTMLElement | undefined
  /** Puts focus home (the focused pane's list, else the explorer container). */
  refocus: () => void
  /**
   * Places inside the home where focus that a command put there stays when an overlay closes:
   * pane chrome, so "Move focus to the pane controls" run from the palette isn't undone by
   * the palette's own restore a moment later.
   */
  keepsFocus?: (element: Element) => boolean
}

/**
 * Where focus was when an overlay opened: the home as a whole, a home control that keeps focus
 * (pane chrome: Escape on the menu its volume button opened goes back to that button, the
 * menu-button pattern), or one element outside the home.
 */
export type FocusOrigin =
  | { kind: 'home' }
  | { kind: 'home-control'; element: HTMLElement }
  | { kind: 'element'; element: HTMLElement }

let home: FocusHome | null = null

/** Registers the window's home; returns the unregister. One home per window. */
export function registerFocusHome(next: FocusHome): () => void {
  home = next
  return () => {
    if (home === next) home = null
  }
}

/** Puts focus home; false when this window registered none. */
export function focusHome(): boolean {
  if (!home) return false
  home.refocus()
  return true
}

/**
 * Whether focus has nowhere useful to be: on `<body>` (its element unmounted under it) or on
 * the home's root itself. Keys reach no pane from `<body>`, so a list that mounts or unmounts
 * puts focus home when this holds (`file-explorer/views/list-focus.svelte.ts`). Never true in
 * a window without a home.
 */
export function isFocusAdrift(): boolean {
  if (!home) return false
  const active = document.activeElement
  return active === null || active === document.body || active === home.root()
}

/**
 * Whether focus may follow the panes when something other than the person moved them: a tool switched
 * or swapped them, or a cursor landed. True only while focus is the explorer's to move, on its lists or
 * adrift. ❌ Never away from the rail, a dialog, or a text field: someone mid-sentence would type into a
 * pane, where Enter opens a file and F8 trashes one, both as themselves.
 */
export function focusMayFollowThePanes(): boolean {
  if (!home) return false
  if (isFocusAdrift()) return true
  const active = document.activeElement
  if (!(active instanceof HTMLElement) || !home.root()?.contains(active)) return false
  return !active.matches('input, textarea, select, [contenteditable]') && !home.keepsFocus?.(active)
}

/** Remembers where `active` (default: the focused element) sits, for `restoreFocus`. */
export function captureFocusOrigin(active: Element | null = document.activeElement): FocusOrigin | null {
  if (!(active instanceof HTMLElement) || active === document.body) return null
  if (home?.root()?.contains(active)) {
    return home.keepsFocus?.(active) ? { kind: 'home-control', element: active } : { kind: 'home' }
  }
  return { kind: 'element', element: active }
}

/**
 * Whether focus already sits in an open menu or dialog other than `leaving` (the overlay
 * closing now), in the toast stack ("Move focus to the notifications" put it there), or on a
 * home element that keeps it (`FocusHome.keepsFocus`).
 */
function focusMovedOn(leaving: Element | null | undefined): boolean {
  const active = document.activeElement
  if (!active || active === document.body || leaving?.contains(active)) return false
  return (
    isFocusInOpenMenu() ||
    active.closest('[role="dialog"], [role="alertdialog"], [data-toast-stack]') !== null ||
    (home?.keepsFocus?.(active) ?? false)
  )
}

/**
 * Puts focus back at `origin`. Pass `leaving` (the closing overlay's own element) when focus
 * may still be inside it as it closes, so it doesn't count as "another" dialog.
 */
export function restoreFocus(origin: FocusOrigin | null, options: { leaving?: Element | null } = {}): void {
  if (!origin || focusMovedOn(options.leaving)) return
  if (origin.kind === 'home' || (origin.kind === 'home-control' && !origin.element.isConnected)) {
    focusHome()
    return
  }
  if (origin.element.isConnected) origin.element.focus()
}
