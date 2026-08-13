/**
 * Leaving no operation behind: the drain every surface that starts real work ends
 * with, for the i18n screenshot-capture driver (`i18n-capture.spec.ts`).
 *
 * Its own module rather than a corner of `i18n-capture-helpers.ts`, which is
 * capture machinery (the sink RPC, paint settling, the shutter, the two surface
 * engines). This is the opposite direction: BACKEND state a surface staged and
 * has to hand back. The queue surfaces' own source-tree helpers live next to
 * their surfaces in `i18n-capture-special.ts` and belong here if a third caller
 * ever needs them.
 */

import { pollUntil } from './helpers.js'
import type { TauriPage } from '@srsholmes/tauri-playwright'

/**
 * Clears the capture throttle, cancels every live operation, drops every retained
 * failure, and WAITS for the operation registry to actually empty. Returns whether
 * it drained inside the budget.
 *
 * ❗ Every surface that starts real work ends with this, and REACTS to a `false`.
 * An operation left in flight isn't litter, it poisons the rest of the run: it
 * holds its device lane, so the next copy is admitted `queued` and never moves,
 * and it holds a queue row, so the queue window is never empty. That reads as four
 * unrelated surfaces failing (`toast-transfer-complete` plus all three queue
 * shots) many surfaces after the one that actually left it behind.
 *
 * Retained failures are deliberately sticky (only an explicit dismissal clears
 * one), so they're dismissed here too, and INSIDE the loop: an operation that dies
 * while the loop is already spinning retains a fresh failure that a one-shot
 * dismiss ahead of it would miss.
 */
export async function resetOperationState(main: TauriPage, timeoutMs = 10000): Promise<boolean> {
  await main
    .evaluate(`(async function(){
      try { await window.__TAURI_INTERNALS__.invoke('set_test_throttle', { ms: null }); } catch (e) {}
      try {
        var ops = await window.__TAURI_INTERNALS__.invoke('list_operations');
        var ids = ops.filter(function(o){ return o.status !== 'failed'; }).map(function(o){ return o.operationId; });
        if (ids.length) await window.__TAURI_INTERNALS__.invoke('cancel_operations', { operationIds: ids });
      } catch (e) {}
    })()`)
    .catch(() => {})

  // `cancel_operations` returns once cancellation is REQUESTED, not once the
  // operations have wound down, and a still-cancelling one still holds its lane.
  // So poll the registry rather than assume: the same drain (and the same reason
  // for it) as `operation-queue.spec.ts`'s afterEach hook.
  return await pollUntil(
    main,
    async () =>
      await main
        .evaluate<boolean>(`(async function(){
          try { await window.__TAURI_INTERNALS__.invoke('dismiss_all_failed_operations'); } catch (e) {}
          var ops = await window.__TAURI_INTERNALS__.invoke('list_operations');
          return !ops || ops.length === 0;
        })()`)
        .catch(() => false),
    timeoutMs,
  )
}

/**
 * `resetOperationState`, recording a drain that didn't happen instead of
 * swallowing it. What a cleanup should call: a `finally` that threw would mask
 * whatever failure got the run there, but a cleanup that stays silent about an
 * operation it couldn't stop hands the next twenty surfaces a poisoned app.
 */
export async function resetOperationStateOrReport(main: TauriPage, failed: string[], where: string): Promise<void> {
  if (await resetOperationState(main)) return
  const label = `${where}-cleanup`
  if (!failed.includes(label)) failed.push(label)
  console.warn(`[i18n-capture] ${where} left an operation in flight; later surfaces needing real work will fail`)
}
