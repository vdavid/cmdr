/**
 * Keeps the window's menu-bar mnemonics off while a shortcut recorder listens, so the recorder can
 * capture `Alt+F`. On Linux every window without a menu of its own wears the main bar (Tauri hands
 * it the app-wide menu), Settings included, and GTK takes a bar title's `Alt+<letter>` before the
 * page sees the keydown (`src-tauri/src/menu/mnemonic_mask/`). A no-op elsewhere.
 *
 * The main window doesn't use this: its dialog gate (`routes/(main)/menu-dialog-gate.svelte.ts`) is
 * that window's one writer. Within a window the claims count, so two recorders can't hand the
 * mnemonics back while the other still listens.
 */
import { setMenuMnemonicsSuspended } from '#lib/tauri-commands/index.js'
import { getAppLogger } from '#lib/logging/logger.js'

const log = getAppLogger('shortcuts')

let claims = 0

function push(suspended: boolean): void {
  void setMenuMnemonicsSuspended(suspended).catch((err: unknown) => {
    // The recorder still works for every key GTK doesn't take, so it's a line, not a failure.
    log.warn("Couldn't switch the menu bar's mnemonics for the shortcut recorder: {error}", { error: err })
  })
}

/** Takes the mnemonics away until the returned release runs. Idempotent release. */
export function claimMenuMnemonics(): () => void {
  claims += 1
  if (claims === 1) push(true)
  let released = false
  return () => {
    if (released) return
    released = true
    claims -= 1
    if (claims === 0) push(false)
  }
}

/**
 * Holds a claim while `isCapturing()` is true, for the life of the calling component (an unmount
 * mid-capture releases it). Call during component init.
 */
export function suspendMenuMnemonicsWhile(isCapturing: () => boolean): void {
  $effect(() => {
    if (!isCapturing()) return
    return claimMenuMnemonics()
  })
}
