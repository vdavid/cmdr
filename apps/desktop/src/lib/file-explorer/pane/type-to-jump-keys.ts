/**
 * Pure key predicates for the type-to-jump intercept. Shared by
 * `DualPaneExplorer.handleKeyDown` (the DOM keydown path) and `routePanelKey`
 * (the Quick Look panel-forwarded path) so the two intercepts stay byte-identical
 * — landmine L9 (the panel mirror must match the main intercept). Factoring them
 * here avoids the copy that would let the two drift.
 *
 * These predicates deliberately stay hand-rolled rather than resolving through the
 * command registry: type-to-jump matches a CLASS of keys (any printable character),
 * not a combo. They're already exact in the way that matters — every one bails on
 * ⌘/⌃/⌥, so no modifier combo can be swallowed as typing.
 */

/** True if a printable letter or digit with no command-modifier and not a fn key. */
export function isTypeToJumpChar(e: KeyboardEvent): boolean {
  if (e.metaKey || e.ctrlKey || e.altKey) return false
  if (e.key.length !== 1) return false
  return /^[a-zA-Z0-9]$/.test(e.key)
}

/**
 * True for ANY single printable character with no command-modifier (Shift is
 * allowed). Superset of `isTypeToJumpChar` that also covers punctuation, space,
 * etc. Used ONLY while a jump is already active (buffer non-empty): once you're
 * typing a name, every printable key extends the buffer instead of triggering a
 * single-char command like `-` (deselect) or Space (toggle selection). After the
 * buffer-reset timeout the buffer empties, so a lone `-` becomes a command again.
 */
export function isPrintableJumpContinuation(e: KeyboardEvent): boolean {
  if (e.metaKey || e.ctrlKey || e.altKey) return false
  return e.key.length === 1
}

/** Keys that should clear an in-flight type-to-jump buffer (then fall through). */
export function isTypeToJumpResetKey(e: KeyboardEvent): boolean {
  switch (e.key) {
    case 'Escape':
    case 'Enter':
    case 'Tab':
    case 'Backspace':
    case 'ArrowUp':
    case 'ArrowDown':
    case 'ArrowLeft':
    case 'ArrowRight':
    case 'PageUp':
    case 'PageDown':
    case 'Home':
    case 'End':
      return true
    default:
      return false
  }
}
