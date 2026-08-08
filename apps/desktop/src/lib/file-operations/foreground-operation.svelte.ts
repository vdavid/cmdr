/**
 * Which operation the FOREGROUND progress modal currently owns.
 *
 * `TransferProgressDialog` shows one operation in full, so ambient surfaces (the
 * status corner's chip, the backgrounded-failure notice) must stay quiet about
 * that same operation: a duplicate readout beside an open modal is noise, and a
 * toast for a failure the user is already looking at is worse.
 *
 * ## Why a module-scoped signal
 *
 * The value would otherwise be prop-drilled from `transfer-progress-state` →
 * `TransferProgressDialog` → `DialogManager` → `DualPaneExplorer` →
 * `routes/(main)/+page.svelte`: four hops of a value nobody in between has any
 * use for. Module scope is per-webview, so this is main-window-only by
 * construction and cannot leak into the queue window.
 *
 * ## One slot, deliberately
 *
 * Exactly ONE foreground progress dialog exists at a time (a second operation
 * either replaces the dialog or auto-queues behind a busy lane), which is what
 * makes a single slot correct rather than a set. If that ever stops being true,
 * the fix is to reconsider the invariant, not to quietly widen this to a set.
 *
 * ## The lifecycle contract
 *
 * The dialog's state machine claims the slot once its `operationId` lands and
 * releases it on EVERY route out: completion, cancel, error, and unmount all go
 * through `destroy()`, while the Queue button and the auto-queue path release it
 * the moment they hand ownership to the queue window — that last one is exactly
 * when the ambient surfaces must start speaking up. Release through
 * `clearForegroundOperation(id)` rather than `setForegroundOperationId(null)`,
 * so a dialog tearing down late can't silence the operation that took the slot
 * after it.
 */

/** The operation the foreground progress dialog owns, or `null` when no modal
 *  is showing one. `$state` so consumers re-render as ownership changes. */
let foregroundOperationId = $state<string | null>(null)

/** Claims the slot for `id`, or empties it with `null`. */
export function setForegroundOperationId(id: string | null): void {
  foregroundOperationId = id
}

/** The operation the foreground dialog owns, or `null`. Reactive: reading this
 *  inside a `$derived` / `$effect` re-runs it when ownership changes. */
export function getForegroundOperationId(): string | null {
  return foregroundOperationId
}

/** Releases the slot, but only if `id` still owns it. A dialog's teardown can
 *  land after the next dialog has claimed the slot; an unconditional clear there
 *  would hide the new foreground operation from every ambient surface. */
export function clearForegroundOperation(id: string): void {
  if (foregroundOperationId === id) foregroundOperationId = null
}

/**
 * The operation whose FAILURE the foreground error dialog is showing, or `null`.
 *
 * A second slot, and it has to be, because of the order things happen in: the
 * progress dialog unmounts the instant the error arrives, releasing the slot
 * above, and the failure row only reaches the snapshot afterwards (the backend
 * emits `write-error` before it settles the record). By then the first slot is
 * empty and an ambient surface would happily announce a failure the user is
 * already reading. This one is claimed on the handover and released when the
 * dialog closes.
 */
let foregroundFailureId = $state<string | null>(null)

/** Claims the failure slot for `id`, or empties it with `null`. */
export function setForegroundFailureId(id: string | null): void {
  foregroundFailureId = id
}

/** The operation the foreground error dialog is showing, or `null`. Reactive. */
export function getForegroundFailureId(): string | null {
  return foregroundFailureId
}
