/**
 * Focus watchdog: logs a WARN when the main window has OS focus but neither
 * file pane is keyboard-focused for {@link WAIT_MS}+ ms, with no dialog open.
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

import { getAppLogger } from '$lib/logging/logger'

const log = getAppLogger('focus-watchdog')

const WAIT_MS = 500

let timeoutId: number | null = null
let warned = false
let installed = false

/**
 * Returns true when `document.activeElement` lives inside the dual-pane
 * explorer (the keyboard event sink for both file panes).
 */
function focusInsideExplorer(): boolean {
  const ae = document.activeElement
  if (!ae || ae === document.body) return false
  return ae.closest('.dual-pane-explorer') !== null
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
  return !document.hasFocus() || focusInsideExplorer() || anyDialogOpen()
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
