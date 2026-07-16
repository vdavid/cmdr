/**
 * Focus watchdog: logs a WARN when the main window has OS focus but focus sits
 * in no keyboard home (neither file pane nor the Ask Cmdr rail) for
 * {@link WAIT_MS}+ ms, with no dialog open.
 *
 * Catches a class of bug where a modal or palette closes without restoring
 * focus to the previously focused pane. Arrow keys then silently no-op
 * because they go to `<body>`. The user just sees "keyboard nav broken"
 * with no obvious trigger; this logs the offending `activeElement` so we
 * can trace it back to the component that should have restored focus.
 *
 * Event-driven, no polling: `focusin`/`focusout` on document plus
 * `focus`/`blur` on window. Cost is a handful of listeners + at most one
 * timer (only while focus is misplaced). Warns at most once per "lost
 * focus episode"; resets when a pane regains focus.
 */

import { quickLookState } from '$lib/file-explorer/quick-look/quick-look-state.svelte'
import { getAppLogger } from '$lib/logging/logger'

const log = getAppLogger('focus-watchdog')

const WAIT_MS = 500

let timeoutId: number | null = null
let warned = false
let installed = false

/**
 * Selectors for every region that is a legitimate keyboard home: the dual-pane
 * explorer (the keyboard event sink for both file panes) and the Ask Cmdr rail
 * (composer, sessions list, and its other focusable chrome). The rail only
 * mounts while open (`{#if askCmdrState.open}`), so a closed rail matches
 * nothing here without any extra guard.
 */
const KEYBOARD_HOME_SELECTOR = '.dual-pane-explorer, .ask-cmdr-rail'

/**
 * Returns true when `document.activeElement` lives inside a keyboard home. This
 * is the "focus is where it belongs" test: the whole watchdog fires only when
 * focus is in NONE of these homes (and no suppressor is active).
 */
function focusInsideKeyboardHome(): boolean {
  const ae = document.activeElement
  if (!ae || ae === document.body) return false
  return ae.closest(KEYBOARD_HOME_SELECTOR) !== null
}

/**
 * Returns true when any dialog-role element is mounted. Covers `ModalDialog`
 * (which sets `role="dialog"` or `role="alertdialog"`) AND the command
 * palette (which manages its own overlay with `role="dialog"`).
 */
function anyDialogOpen(): boolean {
  return document.querySelector('[role="dialog"], [role="alertdialog"]') !== null
}

function shouldSuppress(): boolean {
  // Quick Look's QLPreviewPanel is key while open, so our webview's
  // `document.activeElement` falls back to `<body>` and looks like
  // misplaced focus. AppKit will restore focus to the main window when the
  // panel closes, and the user is in control of when that happens (Shift+
  // Space, Esc, ✕). Treat the panel as the "dialog" for watchdog purposes.
  return !document.hasFocus() || focusInsideKeyboardHome() || anyDialogOpen() || quickLookState.isOpen
}

function check(): void {
  if (shouldSuppress()) {
    reset()
    return
  }
  // Focus is loose: start the countdown if we don't already have one in
  // flight and we haven't already warned for this episode.
  if (timeoutId === null && !warned) {
    timeoutId = window.setTimeout(() => {
      timeoutId = null
      // Re-check in case the situation resolved during the wait.
      if (shouldSuppress()) return
      warned = true
      const ae = document.activeElement
      const aeDescription = ae
        ? `${ae.tagName.toLowerCase()}${ae.id ? `#${ae.id}` : ''}${ae.className && typeof ae.className === 'string' ? `.${ae.className.split(' ').filter(Boolean).slice(0, 3).join('.')}` : ''}`
        : 'null'
      log.warn('Focus left both panes for {ms} ms with no dialog open. activeElement={ae}', {
        ms: WAIT_MS,
        ae: aeDescription,
      })
    }, WAIT_MS)
  }
}

function reset(): void {
  if (timeoutId !== null) {
    window.clearTimeout(timeoutId)
    timeoutId = null
  }
  warned = false
}

/**
 * Install the watchdog. Idempotent (calling twice is a no-op). Call once
 * from the main window's layout `onMount`.
 */
export function initFocusWatchdog(): void {
  if (installed) return
  installed = true
  // `focusin`/`focusout` bubble and fire on every focus change in the
  // document. Capture phase ensures we see them even if a handler down the
  // tree calls `stopPropagation`.
  document.addEventListener('focusin', check, true)
  document.addEventListener('focusout', check, true)
  // Window-level focus/blur covers OS-level switches (other app, other window).
  window.addEventListener('focus', check)
  window.addEventListener('blur', reset)
  // Initial check after first paint: covers the case where focus is already
  // misplaced before the first focus change fires.
  window.setTimeout(check, 100)
}

/**
 * Test-only: tear down listeners and reset state so a fresh `initFocusWatchdog`
 * can run cleanly. jsdom's `document` is shared across tests and listeners
 * persist across module re-imports, so without this each test would accumulate
 * stale handlers that fire alongside the new ones.
 */
export function _resetForTests(): void {
  document.removeEventListener('focusin', check, true)
  document.removeEventListener('focusout', check, true)
  window.removeEventListener('focus', check)
  window.removeEventListener('blur', reset)
  reset()
  installed = false
}
