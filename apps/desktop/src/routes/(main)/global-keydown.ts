/**
 * What the document-level keydown handler should DO with a keypress. The whole
 * decision lives here as a pure function so it's unit-testable; `+page.svelte`
 * only performs the side effects (`preventDefault`, the dispatch, opening the
 * debug window).
 *
 * Two questions, in order:
 *
 * - **Which command does the combo mean?** The Tier 1 reverse lookup, minus the
 *   bails that hand a combo back to the browser (native text copy, ⌘← / ⌘→ inside
 *   an input, typing keys inside an input).
 * - **Would the dispatch core run it right now?** The dialog gate
 *   (`dialog-command-gate.ts`) answers from the command's own `whileDialogOpen`
 *   rule. A key the core would refuse stays unclaimed, so nothing
 *   `preventDefault`s the browser's own action: Tab still moves focus inside a
 *   dialog instead of reaching `pane.switch` behind it.
 */
import { formatKeyCombo, isTypingKeyCombo } from '$lib/shortcuts/key-capture'
import { lookupCommand } from '$lib/shortcuts/shortcut-dispatch'
import { isTextInputFocused } from '$lib/utils/text-input-focus'
import type { CommandId } from '$lib/commands'
import type { DialogsOnScreen } from './command-dispatch-context'
import { isRefusedOverDialog } from './dialog-command-gate'

/** What `+page.svelte` should do with the keypress. */
export type GlobalKeyAction =
  /** Leave the event alone: the browser's default action is what the user wants. */
  | { kind: 'ignore' }
  /** `preventDefault` + `stopPropagation`, then dispatch this command down the keyboard road. */
  | { kind: 'dispatch'; commandId: CommandId }
  /** `preventDefault`, then open the debug window (dev only). */
  | { kind: 'openDebugWindow' }
  /** `preventDefault` and nothing else: a browser default we don't want. */
  | { kind: 'suppress' }

const IGNORE: GlobalKeyAction = { kind: 'ignore' }

/** ⌘⇧D opens the debug window (dev only). Exact combo: ⌃⌘⇧D / ⌥⌘⇧D are other combos. */
function isDebugWindowShortcut(event: KeyboardEvent): boolean {
  return (
    import.meta.env.DEV &&
    event.metaKey &&
    event.shiftKey &&
    !event.altKey &&
    !event.ctrlKey &&
    event.key.toLowerCase() === 'd'
  )
}

/** Browser defaults we suppress outright (⌘A, ⌥⌘I in prod), each an exact combo. */
function shouldSuppressKey(event: KeyboardEvent): boolean {
  if (event.metaKey && !event.altKey && !event.ctrlKey && !event.shiftKey && event.key === 'a') return true
  return !import.meta.env.DEV && event.metaKey && event.altKey && !event.ctrlKey && !event.shiftKey && event.key === 'i'
}

/** True if the user has selected text in the document (non-collapsed range). */
function hasTextSelection(): boolean {
  const selection = window.getSelection()
  return !!selection && !selection.isCollapsed && selection.toString().length > 0
}

/** The centralized command lookup, minus the combos that belong to the browser. */
function commandForCombo(combo: string): CommandId | undefined {
  // Let the browser copy selected text natively (for example, from the error pane)
  // instead of triggering our file-copy command.
  if (combo === '⌘C' && hasTextSelection()) return undefined
  // Let macOS's native line-start / line-end (⌘← / ⌘→) reach text inputs instead of
  // triggering "Copy path between panes" from inside a rename editor, the palette
  // search, the search dialog, settings inputs, etc.
  if ((combo === '⌘←' || combo === '⌘→') && isTextInputFocused()) return undefined
  // Typing wins in text inputs: a bare-key (or shift-only) Tier 1 binding — Tab →
  // switch pane being the built-in case — must not fire mid-typing. Individual
  // inputs used to shield themselves with stopPropagation; this guard protects
  // every current and future text input centrally.
  // ⌘ / ⌃ / ⌥ combos and F-keys stay live.
  if (isTextInputFocused() && isTypingKeyCombo(combo)) return undefined
  return lookupCommand(combo)
}

/**
 * Decides what the keypress means, given what's on screen (`+page.svelte`'s
 * `dialogsOnScreen()`).
 *
 * Claiming a key matters inside a dialog too. With focus in a text input, the gate
 * lets the text-editing family through, and dispatching ⌘V from here is what stops
 * WebKit's native paste from ALSO inserting, while the menu's twin dispatch is
 * swallowed by the cross-source dedup. Left unclaimed, the text would land twice.
 */
export function resolveGlobalKeyAction(event: KeyboardEvent, onScreen: DialogsOnScreen): GlobalKeyAction {
  const combo = formatKeyCombo(event)
  const commandId = commandForCombo(combo)
  if (
    commandId &&
    !isRefusedOverDialog({ commandId, source: 'keyboard', onScreen, textInputFocused: isTextInputFocused() })
  ) {
    return { kind: 'dispatch', commandId }
  }

  // Special cases not handled by centralized dispatch:
  // - Debug window: dev-only, not worth registering as a command
  // - Key suppression: browser behavior overrides, not commands
  if (isDebugWindowShortcut(event)) return { kind: 'openDebugWindow' }
  if (shouldSuppressKey(event)) return { kind: 'suppress' }
  return IGNORE
}
