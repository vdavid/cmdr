/**
 * What the document-level keydown handler should DO with a keypress. The whole
 * decision lives here as a pure function so it's unit-testable; `+page.svelte`
 * only performs the side effects (`preventDefault`, the tagged dispatch, opening
 * the debug window).
 *
 * The caller passes whether a modal is open, because that flips the regime:
 *
 * - **Nothing open**: the full Tier 1 reverse lookup runs, minus the bails that
 *   hand a combo back to the browser (native text copy, ⌘← / ⌘→ inside an input,
 *   typing keys inside an input).
 * - **A modal or explorer overlay is open**: pane-scoped commands must stay
 *   inert, so only the text-editing family resolves, and only while focus is in
 *   a text input.
 */
import { formatKeyCombo, isTypingKeyCombo } from '$lib/shortcuts/key-capture'
import { comboMatchesCommand, lookupCommand } from '$lib/shortcuts/shortcut-dispatch'
import { isTextInputFocused } from '$lib/utils/text-input-focus'
import type { CommandId } from '$lib/commands'

/**
 * The commands whose handlers act on the FOCUSED TEXT INPUT rather than the file
 * pane: `clipboard-handlers.ts` and the `selection.selectAll` arm each branch on
 * `document.activeElement` before touching the explorer. Only these may fire
 * while a modal is open, and only from a text input.
 *
 * Read through the registry (`comboMatchesCommand`), never as literal combos:
 * all four are user-rebindable.
 */
const TEXT_EDITING_COMMAND_IDS = [
  'edit.cut',
  'edit.copy',
  'edit.paste',
  'selection.selectAll',
] as const satisfies readonly CommandId[]

/** What `+page.svelte` should do with the keypress. */
export type GlobalKeyAction =
  /** Leave the event alone: the browser's default action is what the user wants. */
  | { kind: 'ignore' }
  /** `preventDefault` + `stopPropagation`, then dispatch this command tagged `'keyboard'`. */
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
  // inputs used to shield themselves with stopPropagation (NetworkLoginForm still
  // does); this guard protects every current and future text input centrally.
  // ⌘ / ⌃ / ⌥ combos and F-keys stay live.
  if (isTextInputFocused() && isTypingKeyCombo(combo)) return undefined
  return lookupCommand(combo)
}

/**
 * The text-editing command this combo means, while a modal has the keyboard and
 * focus is in a text input.
 *
 * Without this, ⌘V pastes TWICE in every dialog with a text field: nothing calls
 * `preventDefault`, so WebKit runs its native paste AND the macOS Edit > Paste
 * accelerator reaches `edit.paste` through the menu listener (which can't be
 * gated on modal state — when AppKit swallows the key outright, it's the only
 * path). Dispatching from here is the same shape that already makes ⌘V behave
 * outside a modal: the native action dies, and the menu twin is swallowed by the
 * cross-source dedup.
 */
function textEditingCommandForCombo(combo: string): CommandId | undefined {
  if (!isTextInputFocused()) return undefined
  return TEXT_EDITING_COMMAND_IDS.find((id) => comboMatchesCommand(combo, id))
}

/**
 * Decides what the keypress means. `isModalOpen` is the caller's
 * `isModalDialogOpen()` (soft dialogs plus the explorer-owned overlays).
 */
export function resolveGlobalKeyAction(event: KeyboardEvent, isModalOpen: boolean): GlobalKeyAction {
  const combo = formatKeyCombo(event)
  // A modal narrows the keyboard to text editing: everything else is pane-scoped
  // and must stay inert behind the dialog.
  const commandId = isModalOpen ? textEditingCommandForCombo(combo) : commandForCombo(combo)
  if (commandId) return { kind: 'dispatch', commandId }

  // Special cases not handled by centralized dispatch:
  // - Debug window: dev-only, not worth registering as a command
  // - Key suppression: browser behavior overrides, not commands
  if (isDebugWindowShortcut(event)) return { kind: 'openDebugWindow' }
  if (shouldSuppressKey(event)) return { kind: 'suppress' }
  return IGNORE
}
