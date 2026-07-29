/**
 * How an undo's backend report becomes the one line the rail shows.
 *
 * Pure and separate from the store so the honesty rule is testable on its own: a
 * result with anything left behind must never render as a clean success.
 */

import type { RenameUndoState } from './ask-cmdr-messages'
import type { UndoReport } from '$lib/tauri-commands'

/**
 * Read a finished undo as a display state.
 *
 * The order of these branches is the honesty rule (invariant 9): anything left
 * behind outranks what came back. A file the engine skipped (it changed after the
 * rename, or its old name is taken again) and a batch that never ran an inverse are
 * both things the user has to be told about, even when most of the job succeeded.
 *
 * A refused batch is counted apart from `skipped` because it carries no per-file
 * numbers at all — folding the two together would understate what was missed.
 */
export function undoStateFromReport(report: UndoReport): RenameUndoState {
  const refusedBatches = report.operations.filter((operation) => operation.refusal !== null).length
  // Nothing moved and nothing was even attempted: say so, rather than reporting an
  // undo of zero files as a success.
  if (report.restored === 0 && report.skipped === 0) return { status: 'unavailable' }
  if (report.skipped > 0 || refusedBatches > 0) {
    return { status: 'partial', restored: report.restored, skipped: report.skipped, refusedBatches }
  }
  return { status: 'undone', restored: report.restored }
}
