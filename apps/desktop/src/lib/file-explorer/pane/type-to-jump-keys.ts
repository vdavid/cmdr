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
 * ⌘/⌃/⌥. Shift stays allowed because it types capitals and symbols, so the one
 * Shift combo that types nothing new, ⇧Space, gets its own check below. AltGr off
 * macOS types too, though Windows reports it as Ctrl+Alt (`holdsCommandModifier`).
 */

import { isAltGraphHeld, isBoundByAnyCommand, resolveKeyCombo } from '#lib/shortcuts/index.js'

/**
 * True when a modifier makes this keypress a command: ⌘ / ⌃ / ⌥, or off macOS Ctrl /
 * Alt / Super. With AltGr held only Super counts, since Windows reports AltGr as
 * Ctrl+Alt while the key types (Polish AltGr+A is `ą`).
 */
function holdsCommandModifier(e: KeyboardEvent): boolean {
  if (isAltGraphHeld(e)) return e.metaKey
  return e.metaKey || e.ctrlKey || e.altKey
}

/**
 * True if a letter or digit in ANY script (Cyrillic, Greek, CJK, …) with no
 * command-modifier. Named keys (`Enter`, `Dead`, `F5`) are words, so they never
 * match the one-character pattern. A Latin-only test here once left every
 * non-Latin layout unable to jump or filter (cmdr-reports#56).
 */
export function isTypeToJumpChar(e: KeyboardEvent): boolean {
  if (holdsCommandModifier(e)) return false
  return /^[\p{L}\p{N}]$/u.test(e.key)
}

/**
 * True for ANY single printable character with no command-modifier (Shift is
 * allowed). Superset of `isTypeToJumpChar` that also covers punctuation, space,
 * etc. Used ONLY while a jump is already active (buffer non-empty): once you're
 * typing a name, every printable key extends the buffer instead of triggering a
 * single-char command like `-` (deselect) or Space (toggle selection). After the
 * buffer-reset timeout the buffer empties, so a lone `-` becomes a command again.
 *
 * ⇧Space is the exception: it types the same space plain Space does, so giving it to
 * its command costs no character. Without this, Quick Look's ⇧Space did nothing while
 * a filter or jump was on. Read live, so a rebind counts at once; unbound (Quick Look
 * has no key off macOS), it stays a space for anyone typing with Shift held.
 */
export function isPrintableJumpContinuation(e: KeyboardEvent): boolean {
  if (holdsCommandModifier(e)) return false
  // eslint-disable-next-line cmdr/no-raw-key-match -- not a superset: ⌘⌃⌥ bailed above, so this is exactly ⇧Space, and the combo it asks about is any command's.
  if (e.shiftKey && e.key === ' ' && isBoundByAnyCommand(resolveKeyCombo(e))) return false
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

/** A key that moves the cursor or focus, which ends the filter's typing burst: the reset keys bar the two that edit. */
function isCursorMoveKey(e: KeyboardEvent): boolean {
  return isTypeToJumpResetKey(e) && e.key !== 'Backspace' && e.key !== 'Escape'
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
  /** Whether the person is still typing into the filter (`quick-filter-typing.ts`). */
  isFilterTypingLive(): boolean
  /** The cursor moved, so a key pressed next is a command again. */
  endFilterTyping(): void
  /** A key that could have typed `char` into the filter ran its command instead. */
  noteFilterKeyRanCommand(char: string): void
}

/**
 * Whether plain typing narrows the list, per the `fileExplorer.typeToJump.mode` setting. The
 * network views have no backend listing to filter, so there typing always jumps.
 */
export function typingFilters(mode: 'filter' | 'jump', isNetworkView: boolean): boolean {
  return mode === 'filter' && !isNetworkView
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
 * pattern and always extend it. Once it's active, any other printable extends it
 * too while the person is still typing (the burst, `quick-filter-typing.ts`) or
 * when no command is bound to it. Outside the burst a bound one (Space, `-`, `*`)
 * runs its command, so Space selects what the filter left. Backspace edits the
 * pattern and Esc clears it, both consumed while a pattern is active, so Backspace
 * only goes to the parent folder once the filter is empty. Arrows, Enter, and the
 * rest fall through and leave the filter alone (you navigate within the filtered
 * list), and as cursor moves they end the burst at once.
 *
 * A ⌃S session (`quick-filter-session.svelte.ts`) takes this same Filter route on both of its
 * kinds, even with an empty pattern: `isQuickFilterMode` and `isQuickFilterActive` say yes, and
 * the session sends the pattern's edits to whichever controller is on.
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
  if (isTypeToJumpChar(e)) {
    pane.appendQuickFilter(e.key)
    return true
  }
  if (active && isPrintableJumpContinuation(e)) {
    // Read live, so a rebound key follows its new command (or none) at once.
    if (pane.isFilterTypingLive() || !isBoundByAnyCommand(resolveKeyCombo(e))) {
      pane.appendQuickFilter(e.key)
      return true
    }
    pane.noteFilterKeyRanCommand(e.key)
    return false
  }
  // A cursor move, with or without modifiers (⇧↓ extends the selection as it goes).
  if (isCursorMoveKey(e)) pane.endFilterTyping()
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
