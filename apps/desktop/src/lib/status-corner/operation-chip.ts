/**
 * Which operation the corner chip shows, and how full its bar is.
 *
 * The chip is a PREVIEW of the operation queue, not a second queue window: one
 * operation, one bar, no detail. Every rule about what it may say lives here as
 * a pure function over the store's rows, so each gate is provable without a DOM
 * and the component stays markup.
 */

import { isInstantOperation, type OperationRow } from '$lib/file-operations/queue/operations-store.svelte'

/**
 * How long an operation has to last before the chip appears for the first time.
 *
 * Work that's over in a blink shouldn't flash the corner (the house rule for
 * loading states: under about a second, no indicator). The same beat closes a
 * race the frontend can't close otherwise: an operation reaches the main
 * window's store the moment the backend registers it, which is a hair before
 * the start command's response lets the foreground modal claim it.
 */
export const CHIP_SETTLE_MS = 500

/** The operation the chip renders, measured. */
export interface ChipOperation {
  row: OperationRow
  /** How full the bar is, 0–1. */
  fraction: number
  /** The same value as a whole number, so the chip's spoken percentage and the
   *  bar's `aria-valuenow` can't round differently. */
  percent: number
  /** The user paused this one: the bar is frozen, not moving. A paused
   *  operation still reports `is_running: true` from the backend, so this comes
   *  from the snapshot status and nowhere else. */
  paused: boolean
}

/**
 * How full the bar is, 0–1.
 *
 * Bytes are the honest metric for a transfer, EXCEPT when no bytes move: a
 * same-volume move renames server-side, so `bytesTotal` is 0 and a bytes bar
 * would sit at zero for the whole operation. The file count is what's actually
 * progressing there. With neither, the bar sits empty rather than dividing by
 * zero.
 */
function barFraction(progress: OperationRow['progress']): number {
  if (!progress) return 0
  const fraction =
    progress.bytesTotal > 0
      ? progress.bytesDone / progress.bytesTotal
      : progress.filesTotal > 0
        ? progress.filesDone / progress.filesTotal
        : 0
  // A total revised downward mid-operation can overshoot; the bar clamps rather
  // than overflowing its track.
  return Math.min(1, Math.max(0, fraction))
}

/**
 * The destination folder's name for the tooltip, or `''` when the operation has
 * nowhere to put things (a delete or a trash). Tolerates a trailing slash, which
 * the descriptor's directory paths can carry.
 */
export function destinationName(destination: string | null): string {
  if (destination === null) return ''
  const trimmed = destination.replace(/\/+$/, '')
  return trimmed.slice(trimmed.lastIndexOf('/') + 1)
}

/**
 * The operation the chip should show, or `null` to stay quiet.
 *
 * The gates, in order:
 *  - nothing to show for an empty queue;
 *  - instant ops (rename, create folder/file) never reach the corner: they have
 *    no progress to draw and are gone before the eye lands on them;
 *  - the operation the foreground progress modal owns stays with the modal — a
 *    duplicate readout in the corner beside it is noise;
 *  - the FIRST running operation wins (lanes parallelize disjoint volumes, and
 *    snapshot order is the manager's FIFO order, so "first" is stable);
 *  - with nothing running, the first PAUSED one keeps the chip up. Hiding it on
 *    pause would re-hide the work, which is the whole bug this chip exists for.
 *
 * A `queued` row on its own shows nothing: it's waiting on a lane that some
 * other row already speaks for.
 */
export function pickChipOperation(rows: OperationRow[], foregroundOperationId: string | null): ChipOperation | null {
  const eligible = rows.filter(
    (row) => !isInstantOperation(row.snapshot.operationType) && row.snapshot.operationId !== foregroundOperationId,
  )
  const row =
    eligible.find((r) => r.snapshot.status === 'running') ?? eligible.find((r) => r.snapshot.status === 'paused')
  if (!row) return null

  const fraction = barFraction(row.progress)
  return { row, fraction, percent: Math.round(fraction * 100), paused: row.snapshot.status === 'paused' }
}
