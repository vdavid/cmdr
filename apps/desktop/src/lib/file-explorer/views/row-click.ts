/**
 * What a click on a file row means for the selection, shared by Brief and Full view so the two
 * can't read the modifiers differently.
 */
import { holdsPrimaryModifier } from '#lib/shortcuts/primary-modifier.js'
import type { SelectPayload } from '../types'

/** The selection a row click asks for: Shift extends the range, the primary modifier toggles. */
export function rowSelectPayload(event: MouseEvent, index: number): SelectPayload {
  return { index, shiftKey: event.shiftKey, toggle: holdsPrimaryModifier(event) }
}

/** True when the click is a selection gesture, which never arms click-to-rename. */
export function blocksClickToRename(event: MouseEvent): boolean {
  return event.shiftKey || holdsPrimaryModifier(event)
}
