/**
 * Pure step-state derivation for the per-volume indexing checklist.
 *
 * The checklist is COMPOSED from the events that actually fire for one volume,
 * never a hardcoded "every scan has these steps" list:
 *   - a LOCAL full scan runs all four steps (find files → save the file list →
 *     compute folder sizes → catch up on recent changes),
 *   - a NETWORK (SMB/MTP) scan inserts entries inline during the walk and emits
 *     no top-level Aggregating/Reconciling phase, so its Save and Catch-up steps
 *     never appear (find files → compute folder sizes only),
 *   - an event-log roll-on (replay) collapses to a single Update-index step.
 *
 * State is derived from a "furthest reached" index across the available signals
 * (the typed `ActivityPhase` + the live aggregation sub-phase), so the steps are
 * always monotonic: every step before the active one is done, the active one is
 * the live work, and everything after is pending. Deriving from the furthest
 * signal (not only the transition-only phase event) keeps the checklist honest
 * after a mid-scan reload, when the phase event is gone but the aggregation
 * sub-phase still proves how far we are. Branch on the typed discriminants only,
 * never on message wording (`.claude/rules/no-string-matching.md`).
 *
 * Kept pure and component-free so the risky state logic is unit-tested without
 * mounting (see `indexing-steps.test.ts`).
 */
import type { ActivityPhase } from '$lib/ipc/bindings'
import type { MessageKey } from '$lib/intl/keys.gen'

/** A checklist step's stable identity. Each maps to one user-facing label. */
export type IndexStepKind = 'findFiles' | 'saveFileList' | 'computeFolderSizes' | 'catchUp' | 'updateIndex'

export type IndexStepStatus = 'pending' | 'active' | 'done'

export interface IndexStep {
  kind: IndexStepKind
  status: IndexStepStatus
}

/** The aggregation sub-phases that drive the Save and Compute steps. Typed so we
 *  branch on the discriminant, not the wording. Mirrors the Rust writer's order:
 *  `saving_entries → loading → sorting → computing → writing`. */
export type AggregationSubPhase = 'saving_entries' | 'loading' | 'sorting' | 'computing' | 'writing'

/** Which family of steps a volume's pipeline produces. */
export type IndexRunKind = 'local' | 'network' | 'replay'

export interface StepDerivationInput {
  runKind: IndexRunKind
  /** The volume's current top-level pipeline phase, or `undefined` when unknown
   *  (the event is transition-only, so it's gone after a mid-scan reload). */
  phase: ActivityPhase | undefined
  /** The live aggregation sub-phase, when this volume is aggregating. */
  aggregationSubPhase: AggregationSubPhase | undefined
}

/** The compute step's four sub-phases (everything past saving entries). */
const COMPUTE_SUB_PHASES: ReadonlySet<AggregationSubPhase> = new Set(['loading', 'sorting', 'computing', 'writing'])

/** The ordered step kinds per run kind. */
const LOCAL_STEPS: readonly IndexStepKind[] = ['findFiles', 'saveFileList', 'computeFolderSizes', 'catchUp']
const NETWORK_STEPS: readonly IndexStepKind[] = ['findFiles', 'computeFolderSizes']
const REPLAY_STEPS: readonly IndexStepKind[] = ['updateIndex']

/** True once the pipeline has finished (the volume left the active steps). */
function isTerminalPhase(phase: ActivityPhase | undefined): boolean {
  return phase === 'live' || phase === 'idle'
}

/**
 * The index of the furthest LOCAL step proven reached by the signals. Take the
 * max across signals so a present aggregation sub-phase implies find files is
 * done even when the transition-only phase event was missed.
 */
function localReachedIndex(input: StepDerivationInput): number {
  if (isTerminalPhase(input.phase)) return LOCAL_STEPS.length // all done
  let reached = 0 // find files
  if (input.phase === 'aggregating') reached = Math.max(reached, 1) // save the file list
  if (input.aggregationSubPhase === 'saving_entries') reached = Math.max(reached, 1)
  if (input.aggregationSubPhase != null && COMPUTE_SUB_PHASES.has(input.aggregationSubPhase)) {
    reached = Math.max(reached, 2) // compute folder sizes
  }
  if (input.phase === 'reconciling') reached = Math.max(reached, 3) // catch up
  return reached
}

/** The furthest NETWORK step proven reached. Compute is driven off the
 *  aggregation sub-phase (network emits no top-level Aggregating phase). */
function networkReachedIndex(input: StepDerivationInput): number {
  if (isTerminalPhase(input.phase)) return NETWORK_STEPS.length // all done
  let reached = 0 // find files
  if (input.aggregationSubPhase != null && COMPUTE_SUB_PHASES.has(input.aggregationSubPhase)) {
    reached = Math.max(reached, 1) // compute folder sizes
  }
  return reached
}

/** Assign each ordered step a status from the furthest-reached index. */
function statusesFromReached(order: readonly IndexStepKind[], reached: number): IndexStep[] {
  return order.map((kind, i) => ({
    kind,
    status: i < reached ? 'done' : i === reached ? 'active' : 'pending',
  }))
}

/**
 * Derive the ordered checklist with each step's state for one volume.
 */
export function deriveSteps(input: StepDerivationInput): IndexStep[] {
  if (input.runKind === 'replay') {
    return statusesFromReached(REPLAY_STEPS, isTerminalPhase(input.phase) ? REPLAY_STEPS.length : 0)
  }
  if (input.runKind === 'network') {
    return statusesFromReached(NETWORK_STEPS, networkReachedIndex(input))
  }
  return statusesFromReached(LOCAL_STEPS, localReachedIndex(input))
}

/** The single active step, or `undefined` when every step is done (terminal). */
export function activeStep(steps: IndexStep[]): IndexStep | undefined {
  return steps.find((s) => s.status === 'active')
}

/** The user-facing label key for each step (resolved via `tString` at render). */
export const stepKindToLabelKey: Record<IndexStepKind, MessageKey> = {
  findFiles: 'indexing.step.findFiles',
  saveFileList: 'indexing.step.saveFileList',
  computeFolderSizes: 'indexing.step.computeFolderSizes',
  catchUp: 'indexing.step.catchUp',
  updateIndex: 'indexing.step.updateIndex',
}

/** The compute step's sub-phase detail line (folder-worded), resolved at render.
 *  `saving_entries` is the Save step (no sub-line), so it isn't mapped here. */
export const computeSubPhaseToLabelKey: Record<string, MessageKey> = {
  loading: 'indexing.aggregation.loading',
  sorting: 'indexing.aggregation.sorting',
  computing: 'indexing.aggregation.computing',
  writing: 'indexing.aggregation.writing',
}
