/**
 * Maps a pointer's dedicated back / forward side buttons to the pane-history
 * navigation commands, so a mouse with X1/X2 buttons walks Cmdr's history the
 * same way it walks browser and Finder history (issue #31).
 *
 * The buttons reach us two ways, one per platform, both landing on the same two
 * commands:
 *
 * - **The DOM** (`navCommandForMouseButton`), on Linux/WebKitGTK. `MouseEvent.button`
 *   numbers the side buttons per the UI Events spec: 3 is the fourth button (X1,
 *   "back"), 4 is the fifth (X2, "forward"). We branch on those numeric codes,
 *   never a name string, so this never depends on OS/locale wording.
 * - **AppKit** (`navCommandForDirection`), on macOS, where the mouse's driver
 *   decides what the press becomes: a Logi Options+ mouse posts a swipe gesture
 *   and no mouse button at all, so nothing reaches the DOM path.
 *   `src-tauri/src/mouse_nav.rs` reads both shapes and emits the direction as a
 *   typed `mouse-nav` event. What each device delivers, measured:
 *   `docs/notes/mx-side-buttons-swipe-2026-09-04.md`.
 */
import type { CommandId } from '$lib/commands'
import type { MouseNavDirection } from '$lib/ipc/bindings'

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

/** The history command for a direction the native (macOS) monitor reported. */
export function navCommandForDirection(direction: MouseNavDirection): CommandId {
  switch (direction) {
    case 'back':
      return 'nav.back'
    case 'forward':
      return 'nav.forward'
  }
}
