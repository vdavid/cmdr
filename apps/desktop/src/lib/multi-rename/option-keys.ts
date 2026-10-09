/**
 * The Multi-rename sheet's option keys: ⌘⌥ plus a letter flips one checkbox (or opens the
 * Letter case menu). Each is a fixed-key registry command in the `Main window/Multi-rename`
 * scope, so the sheet asks the registry rather than testing key flags, and Settings and the
 * Help window list them. The registry's matcher reads ⌥-composed keys by their physical key,
 * so they work while a text field has focus.
 */

import type { CommandId } from '$lib/commands'
import type { MultiRenameSpec } from '$lib/ipc/bindings'
import { eventMatchesCommand } from '$lib/shortcuts'

/** The spec's on/off fields that have a key. */
export type ToggleField = Extract<
  keyof MultiRenameSpec,
  'removeDiacritics' | 'caseSensitive' | 'firstOnly' | 'includeExtension' | 'regex' | 'substitute'
>

/** What a keypress in the sheet means for its options, if anything. */
export type OptionKey = { kind: 'letterCase' } | { kind: 'toggle'; field: ToggleField }

/** Each option's command, in the order the sheet lists them. */
export const TOGGLE_COMMANDS: readonly { field: ToggleField; commandId: CommandId }[] = [
  { field: 'removeDiacritics', commandId: 'multiRename.removeDiacritics' },
  { field: 'caseSensitive', commandId: 'multiRename.matchCase' },
  { field: 'firstOnly', commandId: 'multiRename.firstMatchOnly' },
  { field: 'includeExtension', commandId: 'multiRename.includeExtension' },
  { field: 'regex', commandId: 'multiRename.regex' },
  { field: 'substitute', commandId: 'multiRename.replaceWholeName' },
]

export function optionKeyOf(event: KeyboardEvent): OptionKey | null {
  // A key that finishes an IME composition belongs to the field, never to the sheet.
  if (event.isComposing) return null
  if (eventMatchesCommand(event, 'multiRename.letterCase')) return { kind: 'letterCase' }
  const toggle = TOGGLE_COMMANDS.find(({ commandId }) => eventMatchesCommand(event, commandId))
  return toggle ? { kind: 'toggle', field: toggle.field } : null
}
