// Tracks Alt/Option modifier key state during drag operations.
// Alt held = "Move" operation, no modifier = "Copy" (default).
//
// Two sources feed this state:
// 1. Document keydown/keyup events (works when webview is focused)
// 2. Native `drag-modifiers` Tauri event from the swizzled WryWebView
//    (reads [NSEvent modifierFlags] — works during OS-level drags when
//    the webview doesn't receive keyboard events)

let altKeyHeld = $state(false)
let listenerAttached = false

function handleKeyDown(e: KeyboardEvent) {
    if (e.key === 'Alt') {
        altKeyHeld = true
    }
}

function handleKeyUp(e: KeyboardEvent) {
    if (e.key === 'Alt') {
        altKeyHeld = false
    }
}

/** Starts listening for Alt key changes on the document. Idempotent. */
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
}

/** Returns whether the Alt/Option key is currently held. */
export function getIsAltHeld(): boolean {
    return altKeyHeld
}

/** Sets the Alt state from an external source (native drag-modifiers event). */
export function setAltHeld(held: boolean): void {
    altKeyHeld = held
}
