/**
 * Pure routing for QueryDialog's in-dialog modifier shortcuts: new query, the mode chips, the
 * Enter combinations, recent items, and the numbered modes, each on its platform's own key
 * (⌘N on macOS, Ctrl+N on Windows and Linux).
 *
 * Kept side-effect free (it only reads the event and calls the handlers it's given) so the
 * modifier-superset rules and the macOS Option-glyph remap are pinned by unit tests instead
 * of by mounting the dialog. The catalog of what each shortcut does lives in
 * `query-ui/DETAILS.md` § Keyboard shortcuts.
 *
 * Where the keys come from: new query, recent items, and the numbered modes are fixed registry
 * commands (`sources/search-dialog.ts`), matched with `eventMatchesCommand`; the rest are
 * `query-keys.ts`'s per-platform table, which the chips render from too.
 */

import { eventMatchesCommand, getEffectiveShortcuts, resolveKeyCombo } from '#lib/shortcuts/index.js'
import { holdsPrimaryModifier } from '#lib/shortcuts/primary-modifier.js'
import { matchesQueryKey } from './query-keys'
import type { SearchMode } from './query-filter-state.svelte'

/** Returns the chip slot for the first / second / third numbered key, or null. AI when on shifts the numbering. */
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
  /** Whether the AI chip exists; gates the AI chip's key and shifts the numbered modes. */
  aiEnabled: boolean
  /** `search.newQuery`: the consumer's reset hook. */
  onNewQuery: () => void
  /** The mode chips' keys and `search.modeByNumber`: switch to the named chip. */
  onModeChange: (mode: SearchMode) => void
  /** `search.modeByNumber` only: the numbered shortcuts also return the caret to the field. */
  onFocusInput: () => void
  /** `search.toggleRecent`: show or hide the recent-items dropdown. */
  onToggleRecent: () => void
  /** ⌥⏎ / Alt+Enter: fire the primary action over the current result set (it no-ops on an empty one). */
  onPrimaryAction: () => void
}

/**
 * Mode chip shortcuts (⌥A / ⌥F / ⌥R, Alt+ off macOS). Wired globally inside the dialog (focus
 * need not be on the chip). The disabled Content chip has no shortcut by design.
 */
function routeModeChipShortcut(e: KeyboardEvent, handlers: ModifierShortcutHandlers): boolean {
  const mode: SearchMode | null =
    matchesQueryKey(e, 'modeAi') && handlers.aiEnabled
      ? 'ai'
      : matchesQueryKey(e, 'modeFilename')
        ? 'filename'
        : matchesQueryKey(e, 'modeRegex')
          ? 'regex'
          : null
  if (mode === null) return false
  e.preventDefault()
  handlers.onModeChange(mode)
  return true
}

/**
 * Routes Enter combinations: ⌥⏎ fires the primary action; any other modified Enter (the
 * platform's primary modifier, Shift, or Alt held) is an explicit no-op per R4, so bare Enter
 * is the only key that does anything, and Ctrl+Enter off macOS doesn't fall through to it.
 */
function routeEnterCombination(e: KeyboardEvent, handlers: ModifierShortcutHandlers): boolean {
  if (matchesQueryKey(e, 'primaryAction')) {
    e.preventDefault()
    handlers.onPrimaryAction()
    return true
  }
  if (matchesQueryKey(e, 'enter') || e.key !== 'Enter') return false
  if (holdsPrimaryModifier(e) || e.shiftKey || e.altKey) {
    e.preventDefault()
    return true
  }
  return false
}

/** `search.modeByNumber`: jump to the numbered mode chip and put the caret back in the field. */
function routeModeNumberShortcut(e: KeyboardEvent, handlers: ModifierShortcutHandlers): boolean {
  if (!eventMatchesCommand(e, 'search.modeByNumber')) return false
  // The command's keys are the digits in order, so the Nth key picks the Nth chip.
  const n = getEffectiveShortcuts('search.modeByNumber').indexOf(resolveKeyCombo(e)) + 1
  const target = modeForShortcutNumber(n, handlers.aiEnabled)
  if (!target) return false
  e.preventDefault()
  handlers.onModeChange(target)
  handlers.onFocusInput()
  return true
}

/** Returns true when the event was claimed, so the caller stops routing it. */
export function routeModifierShortcut(e: KeyboardEvent, handlers: ModifierShortcutHandlers): boolean {
  if (eventMatchesCommand(e, 'search.newQuery')) {
    e.preventDefault()
    handlers.onNewQuery()
    return true
  }
  if (routeModeChipShortcut(e, handlers)) return true
  if (routeEnterCombination(e, handlers)) return true
  if (eventMatchesCommand(e, 'search.toggleRecent')) {
    e.preventDefault()
    handlers.onToggleRecent()
    return true
  }
  if (routeModeNumberShortcut(e, handlers)) return true
  return false
}
