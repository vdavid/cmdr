// Tracks Alt/Option modifier key state during drag operations.
// Alt held = "Move" operation, no modifier = "Copy" (default).
// Uses Svelte 5 reactive state ($state) for live UI updates.

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
