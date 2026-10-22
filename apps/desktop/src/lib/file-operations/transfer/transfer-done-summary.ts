/**
 * What the progress dialog's done state says about a finished operation: its
 * title, the size / time / average-speed line, and the skipped line. Pure, so
 * the wording rules are pinned without rendering. The figures line holds
 * numbers, not text: the dialog renders sizes and rates through `<Size>` the way
 * every other readout does (`#lib/units`).
 */

import type { WriteCompleteEvent } from '#lib/ipc/bindings.js'
import type { TransferOperationType } from '#lib/file-explorer/types.js'
import { tString } from '#lib/intl/messages.svelte.js'
import { formatInteger } from '#lib/intl/number-format.js'

/** Below this, a duration rounds to nothing worth reading and an average speed
 *  divides by noise. */
const MIN_TIMED_MS = 1_000

/** "Copied 1,234 files": what the operation DID, so the skips stay out of the
 *  count (they get their own line). */
export function doneTitle(operationType: TransferOperationType, event: WriteCompleteEvent): string {
  const count = Math.max(0, event.filesProcessed - event.filesSkipped)
  return tString('fileOperations.transferProgress.titleDone', {
    type: operationType,
    count,
    countText: formatInteger(count),
  })
}

/** "Skipped 12 files that already existed", or `null` with nothing skipped. */
export function doneSkippedLine(event: WriteCompleteEvent): string | null {
  if (event.filesSkipped <= 0) return null
  return tString('fileOperations.transferProgress.doneSkipped', {
    count: event.filesSkipped,
    countText: formatInteger(event.filesSkipped),
  })
}

/** The numbers behind the size / time / speed line, by what's known. */
type DoneFigures =
  | { kind: 'rate'; bytes: number; seconds: number; bytesPerSecond: number }
  | { kind: 'size'; bytes: number }
  | { kind: 'time'; seconds: number }

export function doneFigures(event: WriteCompleteEvent, elapsedMs: number | null): DoneFigures | null {
  const bytes = event.bytesProcessed
  const timed = elapsedMs !== null && elapsedMs >= MIN_TIMED_MS
  if (bytes > 0 && timed) {
    const seconds = elapsedMs / 1_000
    return { kind: 'rate', bytes, seconds, bytesPerSecond: bytes / seconds }
  }
  if (bytes > 0) return { kind: 'size', bytes }
  if (timed) return { kind: 'time', seconds: elapsedMs / 1_000 }
  return null
}
