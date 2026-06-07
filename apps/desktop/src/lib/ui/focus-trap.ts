/**
 * `use:trapFocus` — keeps keyboard focus inside a modal container while it's mounted.
 *
 * Every dialog-role overlay in the app uses this action. It does three jobs:
 *
 * 1. **Tab wrapping**: Tab on the last tabbable wraps to the first; Shift+Tab on the
 *    first wraps to the last. The tabbable list is queried fresh on every keypress, so
 *    dialogs whose controls mount and unmount (the onboarding wizard, filter popovers)
 *    stay trapped without re-registration.
 * 2. **Leak guard**: if focus lands outside the container anyway (a programmatic
 *    `.focus()` from pane code finishing an async refresh, for example), it gets pulled
 *    back to the last-focused element inside the container.
 * 3. **Escape fallback**: if an Escape keydown fires while focus is outside the
 *    container — the broken state the leak guard exists for — `onEscape` runs so the
 *    user can always close the dialog from the keyboard. When focus is inside (the
 *    healthy state), this action stays out of the way and the dialog's own Escape
 *    handler works as usual.
 *
 * Traps stack: when several are mounted (a filter-chip popover inside the search
 * dialog), only the most recently mounted one enforces. Closing the top trap hands
 * enforcement back to the one below, which is what gives nested popovers their
 * "Esc closes only the popover" semantics for the leaked-focus path.
 *
 * All listeners run in the capture phase, so an inner component calling
 * `stopPropagation()` on keydown (which every dialog does to shield the file explorer)
 * can't starve the trap.
 */

import type { ActionReturn } from 'svelte/action'

export interface TrapFocusOptions {
  /**
   * Called when Escape is pressed while focus has leaked outside the container.
   * Wire it to the dialog's close callback. Omit for dialogs that intentionally
   * swallow Escape (the onboarding wizard).
   */
  onEscape?: () => void
}

/**
 * No `offsetParent` visibility filtering: jsdom/happy-dom return `null` for every
 * element, which would empty the list in tests. Conditionally rendered controls
 * aren't in the DOM at all, and `[disabled]` is excluded here, so the unfiltered
 * list matches what a real user can tab to.
 */
const FOCUSABLE_SELECTOR =
  '[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])'

interface TrapEntry {
  node: HTMLElement
  options: TrapFocusOptions
  /** Where to send focus when pulling it back after a leak. */
  lastFocusedInside: HTMLElement | null
}

const stack: TrapEntry[] = []
let listenersInstalled = false

function getTabbables(node: HTMLElement): HTMLElement[] {
  return Array.from(node.querySelectorAll<HTMLElement>(FOCUSABLE_SELECTOR))
}

/** Last-focused element inside the trap, falling back to the first tabbable, then the container. */
function restoreFocus(entry: TrapEntry): void {
  const { node, lastFocusedInside } = entry
  if (lastFocusedInside?.isConnected && node.contains(lastFocusedInside)) {
    lastFocusedInside.focus()
    return
  }
  const tabbables = getTabbables(node)
  if (tabbables.length > 0) {
    tabbables[0].focus()
    return
  }
  // Overlay containers carry tabindex="-1" so this works even for an empty dialog.
  node.focus()
}

function handleKeyDown(e: KeyboardEvent): void {
  const top = stack.at(-1)
  if (!top) return
  const target = e.target instanceof HTMLElement ? e.target : null
  const inside = target !== null && top.node.contains(target)

  if (e.key === 'Tab') {
    if (!inside) {
      // Focus already leaked; don't let Tab walk it further away.
      e.preventDefault()
      restoreFocus(top)
      return
    }
    // `inside` narrows `target` to a non-null HTMLElement (aliased-condition narrowing).
    wrapTab(e, top.node, target)
    return
  }

  if (e.key === 'Escape' && !inside && top.options.onEscape) {
    // The dialog's own Escape handler can't fire (focus is outside the container, so
    // the event never propagates through it). Close from here so the keyboard always works.
    e.preventDefault()
    e.stopPropagation()
    top.options.onEscape()
  }
}

/** Wraps Tab at the boundaries of the container's tabbable cycle. */
function wrapTab(e: KeyboardEvent, node: HTMLElement, target: HTMLElement): void {
  const tabbables = getTabbables(node)
  if (tabbables.length === 0) {
    e.preventDefault()
    return
  }
  const index = tabbables.indexOf(target)
  const goingForward = !e.shiftKey
  if (goingForward && (index === -1 || index === tabbables.length - 1)) {
    // From the last tabbable, the container itself, or a click-focused
    // tabindex="-1" element: wrap to the first.
    e.preventDefault()
    tabbables[0].focus()
  } else if (!goingForward && index <= 0) {
    e.preventDefault()
    tabbables[tabbables.length - 1].focus()
  }
  // Mid-list Tab stays with the browser: the next tabbable in DOM order is inside the container.
}

function handleFocusIn(e: FocusEvent): void {
  const top = stack.at(-1)
  if (!top) return
  const target = e.target instanceof HTMLElement ? e.target : null
  if (!target) return
  if (top.node.contains(target)) {
    top.lastFocusedInside = target
    return
  }
  // Focus escaped. Pull it back after a microtask, not synchronously: when a dialog
  // closes, its onDestroy restores focus to the pane while this trap may still be
  // registered for the remainder of the teardown. The microtask runs after the
  // action's destroy has unregistered the trap, so a closing dialog never yanks
  // focus back into its own dying DOM.
  queueMicrotask(() => {
    if (stack.at(-1) !== top || !top.node.isConnected) return
    const active = document.activeElement
    if (active instanceof HTMLElement && top.node.contains(active)) return
    restoreFocus(top)
  })
}

function installListeners(): void {
  if (listenersInstalled) return
  listenersInstalled = true
  document.addEventListener('keydown', handleKeyDown, true)
  document.addEventListener('focusin', handleFocusIn, true)
}

function removeListenersIfIdle(): void {
  if (stack.length > 0 || !listenersInstalled) return
  listenersInstalled = false
  document.removeEventListener('keydown', handleKeyDown, true)
  document.removeEventListener('focusin', handleFocusIn, true)
}

/** Svelte action. Apply to the dialog's outermost element (the overlay). */
export function trapFocus(node: HTMLElement, options: TrapFocusOptions = {}): ActionReturn<TrapFocusOptions> {
  const entry: TrapEntry = { node, options, lastFocusedInside: null }
  stack.push(entry)
  installListeners()

  return {
    update(newOptions: TrapFocusOptions) {
      entry.options = newOptions
    },
    destroy() {
      const index = stack.indexOf(entry)
      if (index !== -1) stack.splice(index, 1)
      removeListenersIfIdle()
    },
  }
}

/**
 * Test-only: drop all traps and detach the document listeners. The stack is
 * module-level state shared across tests in a file; a test that fails before
 * unmounting would otherwise leak its trap into the next test.
 */
export function _resetForTests(): void {
  stack.length = 0
  removeListenersIfIdle()
}
