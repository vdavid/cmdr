/**
 * Pure routing for QueryDialog's in-dialog modifier shortcuts: ⌘N, ⌥A/F/R, ⌥⏎, ⌘⏎/⇧⏎,
 * ⌘H, and ⌘1-9.
 *
 * Kept side-effect free (it only reads the event and calls the handlers it's given) so the
 * modifier-superset rules and the macOS Option-glyph remap are pinned by unit tests instead
 * of by mounting the dialog. The catalog of what each shortcut does lives in
 * `query-ui/DETAILS.md` § Keyboard shortcuts.
 *
 * ⌥← / ⌥→ are deliberately absent: they're macOS's native move-by-word in the focused query
 * input, so the dialog leaves them alone (path pills are mouse-only).
 */

import type { SearchMode } from './query-filter-state.svelte'

/**
 * Matches a plain modifier-key combo (cmd OR alt, no others, no shift).
 *
 * On macOS, Option+<letter> remaps `event.key` to a typographic glyph (Option+F → "ƒ").
 * For Alt combos we therefore also match on `event.code` (which stays layout-stable as
 * `KeyF` etc.). For named keys (Enter, ArrowLeft, …) and Meta combos the plain `e.key`
 * check remains the contract.
 */
export function matchKey(e: KeyboardEvent, key: string, mod: 'meta' | 'alt'): boolean {
  if (e.shiftKey || e.ctrlKey) return false
  const modMatches = mod === 'meta' ? e.metaKey && !e.altKey : e.altKey && !e.metaKey
  if (!modMatches) return false
  if (e.key === key) return true
  if (mod === 'alt' && key.length === 1 && /[a-zA-Z]/.test(key)) {
    return e.code === `Key${key.toUpperCase()}`
  }
  return false
}

/** Returns the chip slot for ⌘1 / ⌘2 / ⌘3, or null. AI when on shifts the numbering. */
export function modeForShortcutNumber(n: number, aiEnabled: boolean): SearchMode | null {
  if (aiEnabled) {
    if (n === 1) return 'ai'
    if (n === 2) return 'filename'
    if (n === 3) return 'regex'
  } else {
    if (n === 1) return 'filename'
    if (n === 2) return 'regex'
  }
  return null
}

/** What the router calls when a combo matches. Every handler owns its own guards. */
export interface ModifierShortcutHandlers {
  /** Whether the AI chip exists; gates ⌥A and shifts the ⌘1-3 numbering. */
  aiEnabled: boolean
  /** ⌘N: the consumer's reset hook. */
  onNewQuery: () => void
  /** ⌥A / ⌥F / ⌥R and ⌘1-9: switch to the named chip. */
  onModeChange: (mode: SearchMode) => void
  /** ⌘1-9 only: the numbered shortcuts also return the caret to the field. */
  onFocusInput: () => void
  /** ⌘H: show or hide the recent-items dropdown. */
  onToggleRecent: () => void
  /** ⌥⏎: fire the primary action over the current result set (it no-ops on an empty one). */
  onPrimaryAction: () => void
}

/**
 * Mode chip shortcuts (⌥A / ⌥F / ⌥R). Wired globally inside the dialog (focus need not be
 * on the chip). The disabled Content chip has no shortcut by design.
 */
function routeModeChipShortcut(e: KeyboardEvent, handlers: ModifierShortcutHandlers): boolean {
  if (matchKey(e, 'a', 'alt') && handlers.aiEnabled) {
    e.preventDefault()
    handlers.onModeChange('ai')
    return true
  }
  if (matchKey(e, 'f', 'alt')) {
    e.preventDefault()
    handlers.onModeChange('filename')
    return true
  }
  if (matchKey(e, 'r', 'alt')) {
    e.preventDefault()
    handlers.onModeChange('regex')
    return true
  }
  return false
}

/**
 * Routes Enter combinations: ⌥⏎ fires the primary action; ⌘⏎ and ⇧⏎ are explicit no-ops
 * per R4 (bare Enter is the only key that does anything).
 */
function routeEnterCombination(e: KeyboardEvent, handlers: ModifierShortcutHandlers): boolean {
  if (e.key !== 'Enter') return false
  if (e.altKey && !e.metaKey && !e.shiftKey) {
    e.preventDefault()
    handlers.onPrimaryAction()
    return true
  }
  if (e.metaKey || e.shiftKey) {
    e.preventDefault()
    return true
  }
  return false
}

/** ⌘1-9: jump to the numbered mode chip and put the caret back in the field. */
function routeModeNumberShortcut(e: KeyboardEvent, handlers: ModifierShortcutHandlers): boolean {
  if (!e.metaKey || e.altKey || e.shiftKey || e.ctrlKey) return false
  if (e.key < '1' || e.key > '9') return false
  const target = modeForShortcutNumber(parseInt(e.key, 10), handlers.aiEnabled)
  if (!target) return false
  e.preventDefault()
  handlers.onModeChange(target)
  handlers.onFocusInput()
  return true
}

/** Returns true when the event was claimed, so the caller stops routing it. */
export function routeModifierShortcut(e: KeyboardEvent, handlers: ModifierShortcutHandlers): boolean {
  if (matchKey(e, 'n', 'meta')) {
    e.preventDefault()
    handlers.onNewQuery()
    return true
  }
  if (routeModeChipShortcut(e, handlers)) return true
  if (routeEnterCombination(e, handlers)) return true
  if (matchKey(e, 'h', 'meta')) {
    e.preventDefault()
    handlers.onToggleRecent()
    return true
  }
  if (routeModeNumberShortcut(e, handlers)) return true
  return false
}
