/**
 * The text-field gate: a command that moves or removes files (`FILE_MOVING_COMMAND_IDS`) never
 * runs from a key or the menu bar while a text field has focus, on any OS.
 *
 * Two roads carry a key into a command, and both ask here. The keyboard resolver
 * (`global-keydown.ts`) leaves the key unclaimed, so the field gets it (`⌘⌫` deletes to the line
 * start, `Delete` forward-deletes). The dispatch core refuses the menu road too: a bar accelerator
 * (macOS's `F8`, `⇧F8`, `F6`, `⌥⌘V`, or any key a user bound) fires before the field sees the
 * key, and the menu event can't say whether a key or a click sent it, so a click on File > Delete
 * with a field focused does nothing either. The palette, the F-key bar, and MCP run: each is a
 * deliberate pick of the command itself.
 */
import { movesFiles, type CommandId } from '#lib/commands/index.js'
import type { DispatchSource } from './command-dispatch-context'

/** Whether the dispatch core refuses `commandId` from `source` because a text field has focus. */
export function isRefusedInTextField(commandId: CommandId, source: DispatchSource, textInputFocused: boolean): boolean {
  if (!textInputFocused) return false
  if (source !== 'keyboard' && source !== 'menu') return false
  return movesFiles(commandId)
}
