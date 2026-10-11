/**
 * The setup dialogs' F2 (copy, move, compress, and trash): confirm, and start the
 * operation with no progress dialog. Plain F2 only: `⇧F2` and `⌘F2` are other
 * combos, and on the delete dialog Shift is the permanent-delete modifier.
 *
 * Dialog-scoped, like the progress dialog's F2 and ❌ never a command-registry
 * binding: F2 is globally `file.rename`, and `ModalDialog`'s overlay stops every
 * keydown before it reaches the global handler, so the scope is structural.
 */
export function isStartInBackgroundKey(event: KeyboardEvent): boolean {
  return event.key === 'F2' && !event.metaKey && !event.ctrlKey && !event.altKey && !event.shiftKey
}

/**
 * Swallows `key`'s auto-repeat until it comes up (or the window loses focus).
 *
 * A background start mounts no modal, so the dialog that took the press is gone
 * while the key is still down, and the OS's repeat keydowns land on the pane. For
 * F2 that's `file.rename` on the cursor file: hold F2 a beat too long and you
 * rename something you never meant to. A CAPTURE listener on `window` runs before
 * anything else sees the event, and only repeats are taken, so a fresh press
 * after the key came up is the global binding again.
 */
export function swallowHeldKeyRepeat(key: string): void {
  const onKeydown = (event: KeyboardEvent): void => {
    if (event.key !== key || !event.repeat) return
    event.preventDefault()
    event.stopImmediatePropagation()
  }
  const release = (event?: Event): void => {
    if (event instanceof KeyboardEvent && event.key !== key) return
    window.removeEventListener('keydown', onKeydown, true)
    window.removeEventListener('keyup', release, true)
    window.removeEventListener('blur', release)
  }
  window.addEventListener('keydown', onKeydown, true)
  window.addEventListener('keyup', release, true)
  window.addEventListener('blur', release)
}
