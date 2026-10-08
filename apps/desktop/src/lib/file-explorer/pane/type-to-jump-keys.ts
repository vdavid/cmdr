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

/** The slice of a pane the typing intercept drives. `FilePaneAPI` satisfies it. */
export interface TypingKeyTarget {
  isRenaming(): boolean
  isJumpActive(): boolean
  handleJumpKeystroke(char: string): void
  clearJumpState(): void
  /** True when typing in this pane narrows the list (the `filter` mode) instead of jumping. */
  isQuickFilterMode(): boolean
  /** True while the quick filter holds a pattern. */
  isQuickFilterActive(): boolean
  appendQuickFilter(char: string): void
  /** Drops the last character of the pattern (clearing it when that was the last one). */
  backspaceQuickFilter(): void
  clearQuickFilter(): void
}

/**
 * THE typing intercept, shared by `key-dispatch.ts` (the DOM keydown path) and
 * `pane-commands.ts` `routePanelKey` (the Quick Look panel path), so the two
 * can't drift (landmine L9). Returns true when it consumed the key; on false the
 * caller hands the key to the pane's own handler.
 *
 * Jump mode: letters/digits (and, mid-jump, any printable) feed the buffer;
 * reset keys clear it and fall through.
 *
 * Filter mode (Total Commander's quick filter): letters/digits start the
 * pattern and, once it's active, any printable extends it. Backspace edits it
 * and Esc clears it, both consumed while a pattern is active, so Backspace only
 * goes to the parent folder once the filter is empty. Arrows, Enter, and the rest
 * fall through and leave the filter alone: you navigate within the filtered list.
 */
export function routeTypingKey(pane: TypingKeyTarget, e: KeyboardEvent): boolean {
  if (pane.isRenaming()) return false
  if (pane.isQuickFilterMode()) return routeFilterKey(pane, e)

  if (isTypeToJumpChar(e) || (pane.isJumpActive() && isPrintableJumpContinuation(e))) {
    pane.handleJumpKeystroke(e.key)
    return true
  }
  if (isTypeToJumpResetKey(e)) {
    pane.clearJumpState()
    // Fall through; Enter/arrows/Backspace/ESC keep their existing meaning.
  }
  return false
}

/** `routeTypingKey` in Filter mode: what extends, edits, or clears the pattern. */
function routeFilterKey(pane: TypingKeyTarget, e: KeyboardEvent): boolean {
  const active = pane.isQuickFilterActive()
  if (isTypeToJumpChar(e) || (active && isPrintableJumpContinuation(e))) {
    pane.appendQuickFilter(e.key)
    return true
  }
  if (!active || e.metaKey || e.ctrlKey || e.altKey) return false
  if (e.key === 'Backspace') {
    pane.backspaceQuickFilter()
    return true
  }
  if (e.key === 'Escape') {
    pane.clearQuickFilter()
    return true
  }
  return false
}
