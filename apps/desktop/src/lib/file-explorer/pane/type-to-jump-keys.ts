/**
 * Pure key predicates for the type-to-jump intercept. Shared by
 * `DualPaneExplorer.handleKeyDown` (the DOM keydown path) and `routePanelKey`
 * (the Quick Look panel-forwarded path) so the two intercepts stay byte-identical
 * — landmine L9 (the panel mirror must match the main intercept). Factoring them
 * here avoids the copy that would let the two drift.
 */

/** True if a printable letter or digit with no command-modifier and not a fn key. */
export function isTypeToJumpChar(e: KeyboardEvent): boolean {
  if (e.metaKey || e.ctrlKey || e.altKey) return false
  if (e.key.length !== 1) return false
  return /^[a-zA-Z0-9]$/.test(e.key)
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
