/**
 * Pure step-state derivation for the per-volume indexing checklist.
 *
 * The checklist is COMPOSED from the events that actually fire for one volume,
 * never a hardcoded "every scan has these steps" list:
 *   - a LOCAL full scan runs all four steps (find files → save the file list →
 *     compute folder sizes → catch up on recent changes); a change check runs
 *     the same four, with the second worded as an update (it writes only what
 *     changed),
 *   - a NETWORK (SMB/MTP) scan inserts entries inline during the walk and emits
 *     no top-level Aggregating/Reconciling phase, so its Save and Catch-up steps
 *     never appear (find files → compute folder sizes only),
 *   - a PHASED first index collapses to the single find-files step: it covers the
 *     drive branch by branch, writing sizes as it goes, so there is no separate
 *     save, no separate size computation, and no catch-up pass to show,
 *   - an event-log roll-on (replay) collapses to a single Update-index step.
 *
 * State is derived from a "furthest reached" index across the available signals
 * (the typed `ActivityPhase` + the live aggregation sub-phase), so the steps are
 * always monotonic: every step before the active one is done, the active one is
 * the live work, and everything after is pending. Deriving from the furthest
 * signal (not only the transition-only phase event) keeps the checklist honest
 * after a mid-scan reload, when the phase event is gone but the aggregation
 * sub-phase still proves how far we are. Branch on the typed discriminants only,
 * never on message wording.
 *
 * Kept pure and component-free so the risky state logic is unit-tested without
 * mounting (see `indexing-steps.test.ts`).
 */
import type { ActivityPhase, CoveragePhase, ScanRunKind } from '$lib/ipc/bindings'
import type { MessageKey } from '$lib/intl/keys.gen'

/** A checklist step's stable identity. Each maps to one user-facing label. */
export type IndexStepKind =
  | 'findFiles'
  | 'saveFileList'
  | 'updateFileList'
  | 'computeFolderSizes'
  | 'catchUp'
  | 'updateIndex'

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
export type IndexRunKind = 'local' | 'network' | 'replay' | 'phased'

/** The run-kind header above the checklist: what KIND of run this is, answering
 *  "is this a full scan, a change check, or a quick roll-on?" at a glance.
 *
 *  `firstIndex` is the phased one and is deliberately NOT called a scan: it
 *  covers the drive folder by folder in the order their owner cares about, so
 *  "First full scan" would promise the one thing it never does. */
export type IndexRunLabel = 'firstIndex' | 'firstScan' | 'rescan' | 'changeCheck' | 'update'

/**
 * Derive the run-kind header for one volume's checklist, or `null` when the scan
 * kind is unknown (a mid-scan reload dropped both the scan-started event and the
 * status backfill) — no header beats a guessed one.
 *
 * The scan kind is the BACKEND's own answer (`ScanRunKind`, off the scan-started
 * event), never inferred from the calibration numbers: those answer a different
 * question and disagree on a populated index whose last scan never completed.
 */
export function deriveRunLabel(runKind: IndexRunKind, scanRunKind: ScanRunKind | undefined): IndexRunLabel | null {
  if (runKind === 'replay') return 'update'
  // A phased run has its own header whatever the backend calls the run: it IS a
  // first scan by `ScanRunKind`, and calling it a full one would describe a pass
  // this drive never makes.
  if (runKind === 'phased') return 'firstIndex'
  if (scanRunKind == null) return null
  return scanRunKindToLabel[scanRunKind]
}

const scanRunKindToLabel: Record<ScanRunKind, IndexRunLabel> = {
  first_scan: 'firstScan',
  full_rebuild: 'rescan',
  change_check: 'changeCheck',
}

/** The user-facing label key for each run-kind header (resolved via `tString` at render). */
export const runLabelToLabelKey: Record<IndexRunLabel, MessageKey> = {
  firstIndex: 'indexing.run.firstIndex',
  firstScan: 'indexing.run.firstScan',
  rescan: 'indexing.run.rescan',
  changeCheck: 'indexing.run.changeCheck',
  update: 'indexing.run.update',
}

/**
 * The header for each phase of a first index, resolved at render.
 *
 * ❌ Deliberately NOT "Indexing your folders" → "Indexing your home folder": the
 * first is a SUBSET of the second, so the pair reads as the scope widening and
 * then narrowing, which is the opposite of what is happening. Each label says
 * what is left instead, so the sequence only ever moves outward.
 */
export const coveragePhaseToLabelKey: Record<CoveragePhase, MessageKey> = {
  priorityRoot: 'indexing.phase.priorityFolders',
  // A folder someone opened mid-run answers the same question their favorites
  // do, so it reads under the same header: a fourth wording for "and this one
  // too" would be noise in a line that is already changing under them.
  visitedRoot: 'indexing.phase.priorityFolders',
  home: 'indexing.phase.home',
  wholeVolume: 'indexing.phase.wholeDrive',
}

export interface StepDerivationInput {
  runKind: IndexRunKind
  /** The volume's current top-level pipeline phase, or `undefined` when unknown
   *  (the event is transition-only, so it's gone after a mid-scan reload). */
  phase: ActivityPhase | undefined
  /** The live aggregation sub-phase, when this volume is aggregating. */
  aggregationSubPhase: AggregationSubPhase | undefined
  /** What kind of run the backend started, when known. Only picks the LABELS of
   *  a local run's steps (a change check updates the file list rather than
   *  saving a fresh one); the order and the state machine are identical. */
  scanRunKind?: ScanRunKind
}

/** The compute step's four sub-phases (everything past saving entries). */
const COMPUTE_SUB_PHASES: ReadonlySet<AggregationSubPhase> = new Set(['loading', 'sorting', 'computing', 'writing'])

/** The ordered step kinds per run kind. */
const LOCAL_STEPS: readonly IndexStepKind[] = ['findFiles', 'saveFileList', 'computeFolderSizes', 'catchUp']
/** A change check writes only what changed, so its second step is worded as an
 *  update. Same order and same state machine as `LOCAL_STEPS`. */
const CHANGE_CHECK_STEPS: readonly IndexStepKind[] = ['findFiles', 'updateFileList', 'computeFolderSizes', 'catchUp']
const NETWORK_STEPS: readonly IndexStepKind[] = ['findFiles', 'computeFolderSizes']
/** A phased first index has one observable step. The other three would sit
 *  pending for the whole run and then flip to done together, which reads as three
 *  things going wrong rather than one thing working. */
const PHASED_STEPS: readonly IndexStepKind[] = ['findFiles']
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
  if (input.runKind === 'phased') {
    return statusesFromReached(PHASED_STEPS, isTerminalPhase(input.phase) ? PHASED_STEPS.length : 0)
  }
  const order = input.scanRunKind === 'change_check' ? CHANGE_CHECK_STEPS : LOCAL_STEPS
  return statusesFromReached(order, localReachedIndex(input))
}

/** The single active step, or `undefined` when every step is done (terminal). */
export function activeStep(steps: IndexStep[]): IndexStep | undefined {
  return steps.find((s) => s.status === 'active')
}

/** The user-facing label key for each step (resolved via `tString` at render). */
export const stepKindToLabelKey: Record<IndexStepKind, MessageKey> = {
  findFiles: 'indexing.step.findFiles',
  saveFileList: 'indexing.step.saveFileList',
  updateFileList: 'indexing.step.updateFileList',
  computeFolderSizes: 'indexing.step.computeFolderSizes',
  catchUp: 'indexing.step.catchUp',
  updateIndex: 'indexing.step.updateIndex',
}

/**
 * The reassuring sub-line under the ACTIVE step, or `null` when it needs none.
 *
 * Only the find-files step has one, and the three cases answer different
 * worries: a change check is slow but leaves folder sizes on screen (say so, or
 * a 20-minute bar looks like something is wrong), a phased first index is slow
 * but hands the drive back in pieces as it goes (which is the whole point, and
 * invisible unless it's said), and a plain first scan has no trustworthy
 * estimate at all (say that instead of showing a stuck-looking count).
 *
 * ❌ The phased case is checked BEFORE the rough-first-scan one: a phased run is
 * a first scan by the backend's own `ScanRunKind`, so the plain hint would win
 * and promise a wait with nothing arriving during it.
 */
export function activeStepHintKey(
  step: IndexStepKind,
  scanRunKind: ScanRunKind | undefined,
  roughFirstScan: boolean,
  coveredInPhases = false,
): MessageKey | null {
  if (step !== 'findFiles') return null
  if (scanRunKind === 'change_check') return 'indexing.step.findFilesChangeCheck'
  if (coveredInPhases) return 'indexing.step.findFilesPhased'
  return roughFirstScan ? 'indexing.step.findFilesFirstScan' : null
}

/** The compute step's sub-phase detail line (folder-worded), resolved at render.
 *  `saving_entries` is the Save step (no sub-line), so it isn't mapped here. */
export const computeSubPhaseToLabelKey: Record<string, MessageKey> = {
  loading: 'indexing.aggregation.loading',
  sorting: 'indexing.aggregation.sorting',
  computing: 'indexing.aggregation.computing',
  writing: 'indexing.aggregation.writing',
}
