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
import { lookupCommand, resolveKeyCombo } from '$lib/shortcuts/shortcut-dispatch'
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
  /** `preventDefault`, then `exitFullScreenOnEscape()`: an Escape nothing else used. */
  | { kind: 'unusedEscape' }

const IGNORE: GlobalKeyAction = { kind: 'ignore' }
const SUPPRESS: GlobalKeyAction = { kind: 'suppress' }

/**
 * Escape is never a centrally dispatched key. Every command bound to it is handled
 * where it lives (`share.back`, `volume.close`, `palette.close` are fixed-key, and
 * `about.close` is `ModalDialog`'s Escape), and the shortcut editor can't record it.
 *
 * What this decides is whether AppKit may see the key: never (`escape-key.ts` §
 * why). A handler that used it already prevented its default, so it's done. Over a
 * dialog, the palette, or in a text field, it's an Escape for them, not a request to
 * leave full screen. Anything else is an Escape nothing used.
 */
function resolveEscape(event: KeyboardEvent, onScreen: DialogsOnScreen): GlobalKeyAction {
  if (event.defaultPrevented) return IGNORE
  if (onScreen.dialogOpen || onScreen.paletteOpen || isTextInputFocused()) return SUPPRESS
  return { kind: 'unusedEscape' }
}

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
 * The command this keypress means, resolved by `resolveKeyCombo` exactly as every
 * local handler resolves it, so a binding the Settings capture recorded can't be
 * live in the pane and dead here. In a text input only the exact combo counts: a
 * key a modifier retyped (`⌥⇧=` → `±`, AltGr → `*`) typed a character there, and
 * typing wins.
 */
function commandForEvent(event: KeyboardEvent, combo: string): CommandId | undefined {
  return commandForCombo(isTextInputFocused() ? combo : resolveKeyCombo(event))
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
  if (combo === 'Escape') return resolveEscape(event, onScreen)
  const commandId = commandForEvent(event, combo)
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

/**
 * The one shape that is always a bug: a local handler called `preventDefault()` on
 * this key and did NOT claim it, and now the document road is about to run a command
 * for it anyway. When both ends land in the same place the work simply happens twice
 * with nothing looking wrong — that is how Enter opened a file twice (two browser
 * tabs on a Google Drive file), PageDown moved two pages, ⌘R re-read every host's
 * shares, and Enter mounted a share twice.
 *
 * `preventDefault` alone is not a claim: this resolver has no `defaultPrevented`
 * guard, deliberately, because preventing the browser's default and claiming a
 * command are different statements. `claimKey(e)` (`$lib/shortcuts/claim-key.ts`) is
 * the claim.
 *
 * Returns the line to log, or null when there's nothing to say. Pure, so the caller
 * decides where it goes and when it's worth saying (dev and E2E runs).
 */
export function unclaimedDispatchWarning(event: KeyboardEvent, commandId: CommandId): string | null {
  if (!event.defaultPrevented) return null
  return (
    `${formatKeyCombo(event)} reached the document dispatcher with its default already prevented, ` +
    `so ${commandId} is about to run a SECOND time. A local handler acted on this key and forgot to ` +
    `claim it — call claimKey(e) there. (If that handler only suppressed a browser default and does ` +
    `want ${commandId} to run, say so in a comment beside it.)`
  )
}
