/**
 * The main window's view of a held quit.
 *
 * The backend owns the decision and the deadline (`src-tauri/src/quit/`). This
 * module only mirrors it: it opens the dialog on `quit-requested`, counts the
 * seconds down for display, sends whichever answer the user picks, and closes
 * on `quit-called-off` when the answer came from somewhere else (an agent over
 * MCP), where nothing would otherwise take the prompt down.
 *
 * **The countdown here is decoration.** It's derived from a wall-clock target
 * rather than decremented per tick, so a throttled or busy webview shows the
 * honest number instead of drifting behind — and if this module never runs at
 * all, Rust's timer still fires. See `CLAUDE.md`.
 */

import type { UnlistenFn } from '@tauri-apps/api/event'
import { onQuitCalledOff, onQuitRequested, quitCancel, quitConfirm, type OperationSnapshot } from '$lib/tauri-commands'
import { getAppLogger } from '$lib/logging/logger'

const log = getAppLogger('quit')

/** How often the displayed number is recomputed. Four times a second so the
 *  visible digit changes within a frame or two of the real second boundary,
 *  without a per-frame loop. */
const TICK_MS = 250

class QuitPrompt {
  /** The operations the backend is holding the quit for. Empty means closed. */
  operations = $state<OperationSnapshot[]>([])
  open = $state(false)
  /** Seconds left, floored, never below zero. Display only. */
  secondsLeft = $state(0)

  /** Wall-clock moment the backend's deadline expires. */
  #deadlineAt = 0
  #ticker: ReturnType<typeof setInterval> | null = null

  /** Opens the prompt for a fresh `quit-requested`. */
  show(operations: OperationSnapshot[], countdownMs: number) {
    this.operations = operations
    this.#deadlineAt = Date.now() + countdownMs
    this.open = true
    this.#retick()
    this.#ticker ??= setInterval(() => {
      this.#retick()
    }, TICK_MS)
  }

  /** The user is going ahead. The dialog stays up (with the buttons gone
   *  nowhere useful) because the app is about to disappear anyway; leaving it
   *  visible avoids a flash of the file panes on the way out. */
  confirm() {
    void quitConfirm().catch((e: unknown) => {
      // The gate's deadline is the backstop, so a dropped confirm costs
      // the user a wait, not the quit.
      log.warn('Confirming the quit returned an error: {error}', { error: String(e) })
    })
  }

  /** The user is staying. Closes the prompt and drops the countdown. */
  keepWorking() {
    this.#close()
    void quitCancel().catch((e: unknown) => {
      log.warn('Calling off the quit returned an error: {error}', { error: String(e) })
    })
  }

  /** The quit was called off from somewhere that isn't this dialog (an agent
   *  over MCP). The gate is already released; this only takes the prompt down,
   *  so nothing is left counting toward a quit that will never come. */
  dismiss() {
    this.#close()
  }

  #retick() {
    this.secondsLeft = Math.max(0, Math.ceil((this.#deadlineAt - Date.now()) / 1000))
  }

  #close() {
    this.open = false
    this.operations = []
    if (this.#ticker !== null) {
      clearInterval(this.#ticker)
      this.#ticker = null
    }
  }
}

export const quitPrompt = new QuitPrompt()

let unlistenRequested: Promise<UnlistenFn> | undefined
let unlistenCalledOff: Promise<UnlistenFn> | undefined

/** Starts listening for the backend holding a quit, and for one being called
 *  off. Main window only: it's the window that owns the app's dialogs. */
export function initQuitPrompt() {
  unlistenRequested ??= onQuitRequested((event) => {
    quitPrompt.show(event.operations, event.countdownMs)
  })
  unlistenCalledOff ??= onQuitCalledOff(() => {
    quitPrompt.dismiss()
  })
}

export function cleanupQuitPrompt() {
  for (const pending of [unlistenRequested, unlistenCalledOff]) {
    void pending?.then((stop) => {
      stop()
    })
  }
  unlistenRequested = undefined
  unlistenCalledOff = undefined
}
