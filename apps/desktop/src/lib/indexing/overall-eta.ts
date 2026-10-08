/**
 * The overall "~X left" on a drive row: the active step's live estimate plus the
 * time the steps after it took on the last completed run of this kind.
 *
 * The remembered half is the BACKEND's (`StepsAheadMs`, summed and gated in the
 * index crate's `lifecycle/steps_ahead.rs`): it's `null` wherever a step ahead
 * has no history, and that's what keeps the figure honest. This side only picks
 * the entry for the active step and adds the one estimate the row already shows.
 * ❌ Don't fill a `null` in from anything else: no figure beats a made-up one.
 */
import type { StepsAheadMs } from '$lib/ipc/bindings'
import type { IndexStepKind } from './indexing-steps'

/** What the overall line shows: nothing, a placeholder, or a figure in seconds. */
export type OverallEta = { kind: 'none' } | { kind: 'estimating' } | { kind: 'known'; seconds: number }

/** Which remembered entry follows each step. A roll-on's one step has none: its own ETA already is the overall. */
const stepToAheadKey: Record<IndexStepKind, keyof StepsAheadMs | null> = {
  findFiles: 'findFiles',
  saveFileList: 'saveFileList',
  updateFileList: 'saveFileList',
  computeFolderSizes: 'computeFolderSizes',
  catchUp: 'catchUp',
  updateIndex: null,
}

/**
 * Derive the overall line for one drive row.
 *
 * `estimating` keeps the line in place while the active step has no estimate
 * of its own yet (a sliding window still filling, or an indeterminate sub-phase)
 * and work is still ahead: the tooltip measures its height once on show, so a
 * line appearing a second later would grow past it. On the last step there's
 * nothing ahead to add, so the step's own ETA is the whole answer and the line
 * goes.
 */
export function deriveOverallEta(
  active: IndexStepKind | undefined,
  activeEtaSeconds: number | null,
  stepsAhead: StepsAheadMs | undefined,
): OverallEta {
  if (active == null || stepsAhead == null) return { kind: 'none' }
  const key = stepToAheadKey[active]
  const aheadMs = key != null ? stepsAhead[key] : null
  if (aheadMs == null) return { kind: 'none' }
  if (activeEtaSeconds != null) return { kind: 'known', seconds: activeEtaSeconds + aheadMs / 1000 }
  return aheadMs > 0 ? { kind: 'estimating' } : { kind: 'none' }
}
