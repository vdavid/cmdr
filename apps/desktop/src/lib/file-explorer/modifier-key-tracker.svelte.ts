// Tracks Alt/Option, Cmd, and Shift modifier key state during drag operations.
//
// The drop-operation policy (`drag/drop-operation.ts`) decides what each modifier means:
// Alt forces Copy; Cmd or Shift forces Move; otherwise the default is volume-aware
// (same volume → Move, cross volume → Copy), matching Finder.
//
// Two sources feed this state:
// 1. Document keydown/keyup events (works when webview is focused)
// 2. Native `drag-modifiers` Tauri event from the swizzled WryWebView
//    (reads [NSEvent modifierFlags] — works during OS-level drags when
//    the webview doesn't receive keyboard events)

let altKeyHeld = $state(false)
let cmdKeyHeld = $state(false)
let shiftKeyHeld = $state(false)
let listenerAttached = false

function handleKeyDown(e: KeyboardEvent) {
  if (e.key === 'Alt') altKeyHeld = true
  else if (e.key === 'Meta') cmdKeyHeld = true
  else if (e.key === 'Shift') shiftKeyHeld = true
}

function handleKeyUp(e: KeyboardEvent) {
  if (e.key === 'Alt') altKeyHeld = false
  else if (e.key === 'Meta') cmdKeyHeld = false
  else if (e.key === 'Shift') shiftKeyHeld = false
}

/** Starts listening for modifier key changes on the document. Idempotent. */
export function startModifierTracking(): void {
  if (listenerAttached) return
  document.addEventListener('keydown', handleKeyDown)
  document.addEventListener('keyup', handleKeyUp)
  listenerAttached = true
}

/** Stops listening and resets state. Idempotent. */
export function stopModifierTracking(): void {
  if (!listenerAttached) return
  document.removeEventListener('keydown', handleKeyDown)
  document.removeEventListener('keyup', handleKeyUp)
  listenerAttached = false
  altKeyHeld = false
  cmdKeyHeld = false
  shiftKeyHeld = false
}

/** Returns the current modifier state. */
export function getModifierState(): { altHeld: boolean; cmdHeld: boolean; shiftHeld: boolean } {
  return { altHeld: altKeyHeld, cmdHeld: cmdKeyHeld, shiftHeld: shiftKeyHeld }
}

/** Sets modifier state from an external source (native drag-modifiers event). */
export function setModifiers(state: { altHeld: boolean; cmdHeld: boolean; shiftHeld: boolean }): void {
  altKeyHeld = state.altHeld
  cmdKeyHeld = state.cmdHeld
  shiftKeyHeld = state.shiftHeld
}
