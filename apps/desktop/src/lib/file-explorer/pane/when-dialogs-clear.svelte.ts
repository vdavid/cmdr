/**
 * Holds a dialog a BACKGROUND job raises late (its archive-password prompt, the
 * error dialog of a refused start) until nothing else is on screen, then shows
 * it.
 *
 * A background job outlives the dialog it was started from, so by the time it
 * needs to ask, the person may be typing in a new setup dialog. Stacking the
 * question over that one steals their keystrokes, and its close would hand focus
 * to the pane while the first dialog is still up. Dropping it would lose the
 * question. So it waits, in arrival order, and asks the moment the window is
 * free: one task per free moment, since each one puts a dialog on screen.
 *
 * Reactive on `isBusy` through its own `$effect.root`, created only while
 * something waits and torn down once the queue is empty, so an idle window pays
 * nothing and no component scope is needed.
 */

import { untrack } from 'svelte'

export function createWhenDialogsClear(isBusy: () => boolean) {
  const pending: Array<() => void> = []
  let stopWatching: (() => void) | null = null

  /** Runs what waits while the window stays free. A task that opens a dialog
   *  makes it busy again, and the rest wait for that one to close. */
  function drain(): void {
    while (pending.length > 0 && !isBusy()) {
      pending.shift()?.()
    }
    if (pending.length === 0 && stopWatching !== null) {
      const stop = stopWatching
      stopWatching = null
      // Not from inside the effect run that called us.
      queueMicrotask(stop)
    }
  }

  return {
    /** Runs `task` now if nothing is on screen and nothing waits ahead of it,
     *  else holds it. Returns whether it ran now. */
    run(task: () => void): boolean {
      if (pending.length === 0 && !isBusy()) {
        task()
        return true
      }
      pending.push(task)
      stopWatching ??= $effect.root(() => {
        $effect(() => {
          if (isBusy()) return
          untrack(drain)
        })
      })
      return false
    },
  }
}
