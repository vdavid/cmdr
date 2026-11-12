/**
 * What the document-level keydown handler should DO with a keypress. The whole
 * decision lives here as a pure function so it's unit-testable; `+page.svelte`
 * only performs the side effects (`preventDefault`, the dispatch, opening the
 * debug window).
 *
 * Two questions, in order:
 *
 * - **Which command does the combo mean?** The Tier 1 reverse lookup, minus the
 *   bails that hand a combo back to the browser (the platform's native text keys,
 *   `native-text-keys.ts`, and typing keys inside an input).
 * - **Would the dispatch core run it right now?** The dialog gate
 *   (`dialog-command-gate.ts`) answers from the command's own `whileDialogOpen`
 *   rule. A key the core would refuse stays unclaimed, so nothing
 *   `preventDefault`s the browser's own action: Tab still moves focus inside a
 *   dialog instead of reaching `pane.switch` behind it.
 */
import { formatKeyCombo, isTypingKeyCombo, latinKeyCombo } from '#lib/shortcuts/key-capture.js'
import { lookupCommand, resolveKeyCombo } from '#lib/shortcuts/shortcut-dispatch.js'
import { isTextInputFocused, isTextInputTarget } from '#lib/utils/text-input-focus.js'
import { paneChromeOwnsKey } from '#lib/file-explorer/pane/pane-chrome.js'
import { toastStackOwnsKey } from '#lib/ui/toast/toast-focus.js'
import type { CommandId } from '#lib/commands/index.js'
import type { DialogsOnScreen } from './command-dispatch-context'
import { isRefusedOverDialog } from './dialog-command-gate'
import { nativeTextKeys } from './native-text-keys'
import { isRefusedInTextField } from './text-field-gate'
import { getPlatform, isMacOS } from '#lib/platform/index.js'

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

/** One exact chord: the platform's primary modifier plus exactly these others, and a letter. */
interface PrimaryChord {
  alt: boolean
  shift: boolean
  letter: string
}

/**
 * True when the event is `chord` exactly, with the platform's primary modifier (⌘ on macOS,
 * Ctrl elsewhere) and nothing else: `⌃⌘⇧D` and `Super+Ctrl+A` are other combos.
 */
function isPrimaryChord(event: KeyboardEvent, chord: PrimaryChord): boolean {
  const mac = isMacOS()
  return (
    event.metaKey === mac &&
    event.ctrlKey === !mac &&
    event.altKey === chord.alt &&
    event.shiftKey === chord.shift &&
    event.key.toLowerCase() === chord.letter
  )
}

/** The debug window's chord (dev only): ⌘⇧D, `Ctrl+Shift+D` off macOS. */
const DEBUG_WINDOW: PrimaryChord = { alt: false, shift: true, letter: 'd' }
/** Select all: ⌘A, `Ctrl+A` off macOS. Unclaimed, the browser would select the whole page. */
const SELECT_ALL: PrimaryChord = { alt: false, shift: false, letter: 'a' }

/** The web inspector's chord, which production suppresses: ⌥⌘I in WebKit, `Ctrl+Shift+I` in WebKitGTK and WebView2. */
function devToolsChord(): PrimaryChord {
  return isMacOS() ? { alt: true, shift: false, letter: 'i' } : { alt: false, shift: true, letter: 'i' }
}

function isDebugWindowShortcut(event: KeyboardEvent): boolean {
  return import.meta.env.DEV && isPrimaryChord(event, DEBUG_WINDOW)
}

/** Browser defaults we suppress outright (select all, and the inspector in prod), each an exact combo. */
function shouldSuppressKey(event: KeyboardEvent): boolean {
  if (isPrimaryChord(event, SELECT_ALL)) return true
  return !import.meta.env.DEV && isPrimaryChord(event, devToolsChord())
}

/** True if the user has selected text in the document (non-collapsed range). */
function hasTextSelection(): boolean {
  const selection = window.getSelection()
  return !!selection && !selection.isCollapsed && selection.toString().length > 0
}

/** The centralized command lookup, minus the combos that belong to the browser. */
function commandForCombo(combo: string): CommandId | undefined {
  const nativeKeys = nativeTextKeys(getPlatform())
  // Let the browser copy selected text natively (for example, from the error pane)
  // instead of triggering our file-copy command.
  if (nativeKeys.copyWithSelection.has(combo) && hasTextSelection()) return undefined
  // A focused text field owns its editing keys (line and word motion, word delete, …), so
  // "Copy path between panes" can't fire from inside a rename editor, the palette search,
  // or the search dialog, and Delete can't run on a word delete.
  if (isTextInputFocused() && (nativeKeys.fieldOwned.has(combo) || nativeKeys.bailInField.has(combo))) return undefined
  // Typing wins in text inputs: a bare-key (or shift-only) Tier 1 binding — Tab →
  // switch pane being the built-in case — must not fire mid-typing. Individual
  // inputs used to shield themselves with stopPropagation; this guard protects
  // every current and future text input centrally.
  // ⌘ / ⌃ / ⌥ combos and F-keys stay live.
  if (isTextInputFocused() && isTypingKeyCombo(combo)) return undefined
  // A focused pane-chrome control (a path segment, a tab, a sort header) answers Enter,
  // Space, the arrows, and Home/End itself, so they mustn't open the cursor row or move it.
  if (paneChromeOwnsKey(combo)) return undefined
  // Same for a button in a toast: Enter on Undo presses Undo, not the cursor row.
  if (toastStackOwnsKey(combo)) return undefined
  const commandId = lookupCommand(combo)
  // A key that would move or remove files is the field's while one has focus, whatever command
  // it's bound to (`text-field-gate.ts`): F8 in the quick filter trashes nothing.
  if (commandId !== undefined && isRefusedInTextField(commandId, 'keyboard', isTextInputFocused())) return undefined
  return commandId
}

/**
 * The command this keypress means, resolved by `resolveKeyCombo` exactly as every
 * local handler resolves it, so a binding the Settings capture recorded can't be
 * live in the pane and dead here. In a text input only the exact combo counts: a
 * key a modifier retyped (`⌥⇧=` → `±`, AltGr → `*`) typed a character there, and
 * typing wins.
 *
 * The one fallback a text input also tries is a non-Latin letter's Latin key
 * position (`latinKeyCombo`), through the same bails, so `⇧В` still types while a
 * Russian `⌘V` is claimed like any `⌘V`. Unclaimed, the menu's Paste (which macOS
 * matches by key position) and WebKit's native paste would BOTH insert.
 */
function commandForEvent(event: KeyboardEvent, combo: string): CommandId | undefined {
  if (!isTextInputFocused()) return commandForCombo(resolveKeyCombo(event))
  const latin = latinKeyCombo(event)
  return commandForCombo(combo) ?? (latin === null ? undefined : commandForCombo(latin))
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
  if (shouldSuppressKey(event) || isUnclaimedListScrollKey(event)) return { kind: 'suppress' }
  return IGNORE
}

/** The keys WebKit scrolls a focused scroller with, when nobody cancels them. */
const NATIVE_SCROLL_KEYS = new Set([
  'ArrowUp',
  'ArrowDown',
  'ArrowLeft',
  'ArrowRight',
  'PageUp',
  'PageDown',
  'Home',
  'End',
  ' ',
])

/**
 * A scroll key that reached the document unclaimed from a pane's file list (any modifiers:
 * ⌥↓ and ⌘↑ scroll too). Focus sits on the list (`active-pane-focus.ts`), so WebKit would
 * scroll it natively and yank the virtual view off the cursor: ↓ on Full view's last row
 * moved `scrollTop` 7,481 → 7,485 (real keys, macOS 27.0, 2026-10-15). The virtual scroller
 * alone moves the view. ❌ Not an early `preventDefault` in the pane's key dispatch: that
 * trips `unclaimedDispatchWarning` on every arrow. A text input inside the list (the rename
 * editor) keeps its caret keys.
 */
function isUnclaimedListScrollKey(event: KeyboardEvent): boolean {
  if (event.defaultPrevented || !NATIVE_SCROLL_KEYS.has(event.key)) return false
  const target = event.target
  return target instanceof Element && !isTextInputTarget(target) && target.closest('[data-file-list-surface]') !== null
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
 * command are different statements. `claimKey(e)` (`#lib/shortcuts/claim-key.ts`) is
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
