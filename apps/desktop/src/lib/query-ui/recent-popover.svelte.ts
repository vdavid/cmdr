/**
 * Controller for the query field's recent-items dropdown.
 *
 * Recent items are the field's own dropdown (`RecentItemsPopover` anchored to the pill),
 * not a footer strip. Openers: the field's chevron, `⌘H`, and ArrowDown in the field when
 * there's no result list to walk. Picking a row LOADS the entry and closes; it doesn't run
 * it (the user presses Enter when they're ready).
 *
 * The controller exists so QueryDialog holds one object instead of an open flag plus four
 * focus-restore functions. The focus rules are the whole reason it isn't a plain boolean:
 * see `close()` vs `closeAndFocus()` below.
 */

import { tick } from 'svelte'

export interface RecentPopoverDeps<E> {
  /** Puts the caret back in the query input. */
  focusInput: () => void
  /** The pill frame the dropdown anchors to. */
  getAnchor: () => HTMLElement | undefined
  /**
   * Loads the picked entry into the consumer's state. Must NOT run it: an AI entry that
   * ran itself would spend the user's money on a keystroke they meant as navigation.
   */
  onActivate: (entry: E) => void
}

/**
 * Every member is a function PROPERTY, not a method: the dialog hands them straight to
 * `RecentItemsPopover` and `QueryBar` as callbacks, and method signatures trip
 * `@typescript-eslint/unbound-method`.
 */
export interface RecentPopoverController<E> {
  /** Whether the dropdown is showing. Reactive: safe to read from a template or `$derived`. */
  readonly isOpen: boolean
  open: () => void
  /** Closes on a mouse/Escape path, restoring focus only if nothing else claimed it. */
  close: () => void
  /** Closes on a keyboard path, putting the caret straight back in the field. */
  closeAndFocus: () => void
  toggle: () => void
  /** A row was picked: load it, hand `⏎` back to "run-search", and return focus. */
  pick: (entry: E) => void
}

export function createRecentPopover<E>(deps: RecentPopoverDeps<E>): RecentPopoverController<E> {
  let visible = $state(false)

  /**
   * The focus call waits a tick: while the popover is still mounted its own focus trap
   * would pull focus straight back.
   */
  function closeAndFocus(): void {
    visible = false
    void tick().then(() => {
      deps.focusInput()
    })
  }

  return {
    get isOpen() {
      return visible
    },
    open: () => {
      visible = true
    },
    /**
     * `Popover`'s Escape path calls `onClose()` and then `anchor.focus()`; the anchor is the
     * pill frame (a `<div>`), which isn't focusable, so without this the focus would fall to
     * the document. Click-outside must NOT be stolen though, so the refocus is deferred one
     * frame and only fires when nothing else has claimed focus by then.
     */
    close: () => {
      visible = false
      requestAnimationFrame(() => {
        const active = document.activeElement
        if (active === null || active === document.body || active === deps.getAnchor()) deps.focusInput()
      })
    },
    closeAndFocus,
    toggle: () => {
      if (visible) closeAndFocus()
      else visible = true
    },
    pick: (entry: E) => {
      deps.onActivate(entry)
      closeAndFocus()
    },
  }
}
