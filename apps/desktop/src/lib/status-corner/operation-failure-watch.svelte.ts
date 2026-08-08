/**
 * Tells the main window when a backgrounded operation stops before it's done.
 *
 * The backend retains every real failure and carries it on the snapshot the
 * main window already subscribes to, so this is a pure reaction to rows entering
 * `failed`: no new event, no listener, no polling.
 *
 * ## Why a toast, not a dialog
 *
 * The user pressed Queue precisely to stop being blocked, and a settled failure
 * asks for no decision (unlike a conflict prompt, which genuinely blocks). A
 * modal here would steal focus and eat the keystroke they were mid-way through.
 * So: a toast that NEVER auto-dismisses, because the whole point is the person
 * who was away from the keyboard when it happened.
 *
 * ## Why the cap
 *
 * Past three, the individual toasts collapse into one summary. That's
 * mechanical, not aesthetic: a toast stack full of persistent toasts silently
 * DROPS new ones (`lib/ui/CLAUDE.md`), so an unbounded burst would lose the very
 * failures it's meant to report. Every reason stays reachable in the queue
 * window, which is the surface that promises completeness.
 */

import { getMainWindowOperationRows } from '$lib/file-operations/queue/main-window-operations.svelte'
import type { OperationRow } from '$lib/file-operations/queue/operations-store.svelte'
import { getForegroundFailureId, getForegroundOperationId } from '$lib/file-operations/foreground-operation.svelte'
import { addToast, dismissToast, getToasts } from '$lib/ui/toast'
import OperationFailedToastContent from './OperationFailedToastContent.svelte'
import OperationFailuresToastContent from './OperationFailuresToastContent.svelte'

/** Keeps a burst of failures from pushing unrelated toasts off the screen: the
 *  group's own cap bites before the global one. */
export const FAILURE_TOAST_GROUP = 'operation-failure'

/** How many failures get a toast of their own before they collapse. */
export const MAX_FAILURE_TOASTS = 3

/** Dedup id of the collapsed summary. */
export const FAILURE_SUMMARY_TOAST_ID = 'operation-failure-summary'

/** Headroom over what {@link announceFailures} ever creates (three individual
 *  toasts, or one summary), so the group cap can only ever be a backstop. A
 *  full group of persistent toasts drops new arrivals silently, and this is the
 *  one place that could go wrong unnoticed. */
const MAX_IN_GROUP = MAX_FAILURE_TOASTS + 1

const failureToastId = (operationId: string): string => `operation-failure:${operationId}`

/**
 * Operations already spoken for, so a re-emitted snapshot can't toast twice. It
 * holds SUPPRESSED failures too (the ones a foreground dialog is showing): they
 * were reported, just not by us, and they must not get a late toast when that
 * dialog closes.
 *
 * A plain `Set`, not `SvelteSet`: nothing renders from it, and making it
 * reactive would have the announcing effect re-run itself.
 */
// eslint-disable-next-line svelte/prefer-svelte-reactivity -- bookkeeping only; nothing renders from it, and reactivity here would re-trigger the effect that writes it
let announced = new Set<string>()

/** Drops the memory of every announced failure. For test isolation and for the
 *  watch's own teardown; production never needs it (ids are unique). */
export function resetAnnouncedFailures(): void {
  // eslint-disable-next-line svelte/prefer-svelte-reactivity -- see `announced`
  announced = new Set<string>()
}

/** Live failure toasts, read off the toast store rather than tracked here: the
 *  user can dismiss one at any time, and the store is the only thing that knows. */
function currentFailureToasts(): { id: string }[] {
  return getToasts().filter((toast) => toast.toastGroup === FAILURE_TOAST_GROUP)
}

function raiseToast(row: OperationRow): void {
  const live = currentFailureToasts()
  // The summary counts failures itself, so once it's up there's nothing to add.
  if (live.some((toast) => toast.id === FAILURE_SUMMARY_TOAST_ID)) return

  if (live.length < MAX_FAILURE_TOASTS) {
    addToast(OperationFailedToastContent, {
      id: failureToastId(row.snapshot.operationId),
      level: 'error',
      dismissal: 'persistent',
      toastGroup: FAILURE_TOAST_GROUP,
      maxInGroup: MAX_IN_GROUP,
      props: { snapshot: row.snapshot },
    })
    return
  }

  // Collapse: clear the individual ones FIRST, so the summary lands in a group
  // that has room for it.
  for (const toast of live) dismissToast(toast.id)
  addToast(OperationFailuresToastContent, {
    id: FAILURE_SUMMARY_TOAST_ID,
    level: 'error',
    dismissal: 'persistent',
    toastGroup: FAILURE_TOAST_GROUP,
    maxInGroup: MAX_IN_GROUP,
  })
}

/**
 * One pass over the current rows: toast whatever failed that hasn't been spoken
 * for yet. Idempotent, so the effect can run it on every snapshot tick.
 */
export function announceFailures(rows: OperationRow[]): void {
  const failures = rows.filter((row) => row.snapshot.status === 'failed')

  // Forget failures that left the snapshot, so the set can't grow for the life
  // of the process. An operation id is never reused, so a forgotten failure can
  // never come back and toast a second time.
  // eslint-disable-next-line svelte/prefer-svelte-reactivity -- transient local, not reactive state
  const present = new Set(failures.map((row) => row.snapshot.operationId))
  for (const id of announced) {
    if (!present.has(id)) announced.delete(id)
  }

  for (const row of failures) {
    const id = row.snapshot.operationId
    if (announced.has(id)) continue
    announced.add(id)
    // The backend retains unconditionally (it can't know a modal is up), so
    // this is where a foreground failure stops being reported twice. The row
    // still appears in the queue: that window is the operation-status surface,
    // and honesty beats tidiness.
    if (id === getForegroundOperationId() || id === getForegroundFailureId()) continue
    raiseToast(row)
  }
}

let stop: (() => void) | null = null

/**
 * Starts watching the main window's operations. Idempotent; call
 * {@link stopOperationFailureWatch} on teardown.
 *
 * The effect runs outside a component (`$effect.root`) because the watch is the
 * page's, not the corner's: a toast isn't part of the status row, and tying it
 * to a component would make its lifetime that component's business.
 */
export function startOperationFailureWatch(): void {
  if (stop) return
  stop = $effect.root(() => {
    $effect(() => {
      announceFailures(getMainWindowOperationRows())
    })
  })
}

export function stopOperationFailureWatch(): void {
  stop?.()
  stop = null
  resetAnnouncedFailures()
}
