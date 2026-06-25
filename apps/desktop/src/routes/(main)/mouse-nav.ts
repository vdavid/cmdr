/**
 * Maps a pointer's dedicated back / forward side buttons to the pane-history
 * navigation commands, so a mouse with X1/X2 buttons walks Cmdr's history the
 * same way it walks browser and Finder history (issue #31).
 *
 * `MouseEvent.button` numbers the side buttons per the UI Events spec: 3 is the
 * fourth button (X1, "back"), 4 is the fifth (X2, "forward"). We branch on those
 * numeric codes, never a name string, so this never depends on OS/locale wording.
 */
import type { CommandId } from '$lib/commands'

/** Fourth mouse button (X1), conventionally "back". */
const MOUSE_BUTTON_BACK = 3
/** Fifth mouse button (X2), conventionally "forward". */
const MOUSE_BUTTON_FORWARD = 4

/**
 * The history command a mouse button should drive, or `null` for buttons we
 * don't own (primary/middle/secondary). The caller dispatches the returned id
 * through the same command bus as the `⌘[` / `⌘]` shortcuts.
 */
export function navCommandForMouseButton(button: number): CommandId | null {
  if (button === MOUSE_BUTTON_BACK) return 'nav.back'
  if (button === MOUSE_BUTTON_FORWARD) return 'nav.forward'
  return null
}
