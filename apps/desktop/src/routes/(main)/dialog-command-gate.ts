/**
 * The dialog gate: whether a command may run while a dialog, an explorer overlay, or the
 * command palette is up in the main window.
 *
 * The rule belongs to the command (`Command.whileDialogOpen`, required on every registry
 * entry); this module applies it. The dispatch core asks on EVERY dispatch, so the answer is
 * the same whichever road the command came in by. When only the keyboard resolver knew about
 * dialogs, a native-menu accelerator (⌘W, ⌘K) reached its handler behind an open one.
 *
 * The keyboard resolver asks too, one step earlier and for its own reason: a key the core would
 * refuse must stay unclaimed, so the browser's default still happens (Tab moves focus inside the
 * dialog).
 */
import { whileDialogOpenFor, type CommandId } from '$lib/commands'
import type { DialogsOnScreen, DispatchSource } from './command-dispatch-context'

export interface DialogGateQuestion {
  commandId: CommandId
  source: DispatchSource
  onScreen: DialogsOnScreen
  /** `isTextInputFocused()`, read at dispatch time. */
  textInputFocused: boolean
}

/** Whether the dispatch core refuses `commandId` from `source` with `onScreen` up. */
export function isRefusedOverDialog({ commandId, source, onScreen, textInputFocused }: DialogGateQuestion): boolean {
  // MCP answers for itself: its file-operation tools refuse with a typed reason while a blocking
  // dialog is up (`mcp/executor`), and `dialog.confirm` exists to act on the open dialog. A
  // refusal here would be silent, so the agent would read it as success.
  if (source === 'mcp') return false
  // The palette closes on the way to its row's handler, so it never stands in its own way.
  const somethingInTheWay = onScreen.dialogOpen || (onScreen.paletteOpen && source !== 'palette')
  if (!somethingInTheWay) return false

  const rule = whileDialogOpenFor(commandId)
  switch (rule.runs) {
    case 'always':
      return false
    case 'inTextInput':
      return !textInputFocused
    case 'never':
      return true
  }
}
