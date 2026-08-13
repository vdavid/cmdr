/**
 * The one-shot first-run pane layout: on a fresh install that already has Full Disk
 * Access, the left pane opens on the home folder and the right pane on `~/Downloads`.
 * It happens once per install and never again; every later launch restores wherever the
 * user left off.
 *
 * ⚠️ An install that already carries pane state gets its marker recorded WITHOUT its
 * panes being touched (`markAlreadyLaidOut`). Why that branch is the one thing here that
 * must never be "simplified", and what an applied layout costs a user who already had
 * one: `DETAILS.md` § "First-run pane layout", which owns the full statement.
 *
 * The decision is a pure function so the whole matrix can be walked in a unit test, and
 * so the `~/Downloads` probe is structurally unreachable until Full Disk Access is
 * confirmed: `~/Downloads` sits behind a TCC gate, and even stat'ing it without the
 * permission can raise a system dialog the user has no context for.
 *
 * This module decides; it never writes. `loadPersistedState` (`initialization.ts`) owns
 * every write the outcome implies, in one ordered place.
 */
import type { PersistedPaneTabs, PersistedTab } from '../tabs/tab-types'

/** Where the left pane opens on a first run. */
export const FIRST_RUN_LEFT_PATH = '~'

/** Where the right pane opens on a first run, when the folder is there. */
export const FIRST_RUN_RIGHT_PATH = '~/Downloads'

/** What the rule reads. Every field is a fact about this install, gathered before deciding. */
export interface FirstRunLayoutContext {
  /** A Playwright-driven run (plain E2E or a screenshot capture): never lay out, never record. */
  isAutomatedRun: boolean
  /** The `firstRunLayoutApplied` marker from `app-status.json`. */
  layoutAlreadyApplied: boolean
  /** `app-status.json` carries pane state, so this install has run before. */
  hasPersistedPaneState: boolean
  /** Cmdr currently has Full Disk Access. A never-answered prompt reads as `false`. */
  hasFullDiskAccess: boolean
}

/**
 * - `leaveAlone`: touch nothing, record nothing.
 * - `markAlreadyLaidOut`: record the marker so the rule can never fire for this install,
 *   and leave the panes exactly as they were persisted.
 * - `openHomeAndDownloads`: apply the layout, then record the marker.
 */
export type FirstRunLayoutDecision = 'leaveAlone' | 'markAlreadyLaidOut' | 'openHomeAndDownloads'

/** The rule itself. Pure, so it is cheap to walk exhaustively in tests. */
export function decideFirstRunLayout(ctx: FirstRunLayoutContext): FirstRunLayoutDecision {
  if (ctx.isAutomatedRun) return 'leaveAlone'
  if (ctx.layoutAlreadyApplied) return 'leaveAlone'
  if (ctx.hasPersistedPaneState) return 'markAlreadyLaidOut'
  if (!ctx.hasFullDiskAccess) return 'leaveAlone'
  return 'openHomeAndDownloads'
}

/** The paths the two panes open on. */
export interface FirstRunLayout {
  leftPath: string
  rightPath: string
}

/**
 * What the caller should do. The `kind`s are named after `FirstRunLayoutDecision` on
 * purpose: the outcome of a fully-resolved run is the decision, which is what lets
 * `first-run-layout.test.ts` assert the two agree across every input combination.
 *
 * Nothing here is persisted by this module. The caller owns every write, so the ordering
 * that makes an interrupted first run safe lives in one readable place.
 */
export type FirstRunLayoutOutcome =
  | { kind: 'leaveAlone' }
  | { kind: 'markAlreadyLaidOut' }
  | ({ kind: 'openHomeAndDownloads' } & FirstRunLayout)

export interface FirstRunLayoutDeps {
  isAutomatedRun: () => boolean
  /** Already loaded with the rest of the app status, so a value rather than a probe. */
  layoutAlreadyApplied: boolean
  hasPersistedPaneState: () => Promise<boolean>
  hasFullDiskAccess: () => Promise<boolean>
  pathExists: (path: string) => Promise<boolean>
}

/** The two facts that cost a probe: a store open and a permission check over IPC. */
type ProbedFact = 'hasPersistedPaneState' | 'hasFullDiskAccess'

/**
 * Resolves one fact, but only when it can still change the answer: if the rule decides the
 * same thing whichever way the fact goes, the probe is skipped and the placeholder stays.
 *
 * That keeps `decideFirstRunLayout` the single statement of the rule while the resolver
 * stays lazy. A hand-written short-circuit would have to repeat the guard order here, and
 * would drift the day the rule changes.
 *
 * ⚠️ The skip is only sound relative to the context AS IT STANDS, which still holds
 * placeholders for facts settled later. Two properties of the rule make that safe, and a
 * change to `decideFirstRunLayout` has to preserve both:
 *
 * 1. Each probed fact is read by exactly ONE guard, so no later guard can revive a fact an
 *    earlier one made irrelevant.
 * 2. Any skip is caused by an earlier guard that returns unconditionally, so the guards
 *    after it are unreachable whatever the placeholders say.
 *
 * Break either and a probe gets skipped while its placeholder still steers the answer: a
 * rule reading `hasPersistedPaneState && hasFullDiskAccess` in one guard would skip the
 * pane-state probe, leave it `false`, and lay out over a returning user's real layout. The
 * "matches the fully-probed decision" test in `first-run-layout.test.ts` is what catches
 * that, so ❌ don't delete it when editing the rule.
 */
async function settle(
  ctx: FirstRunLayoutContext,
  fact: ProbedFact,
  probe: () => Promise<boolean>,
): Promise<FirstRunLayoutContext> {
  const withFact = (value: boolean): FirstRunLayoutContext => ({ ...ctx, [fact]: value })
  if (decideFirstRunLayout(withFact(false)) === decideFirstRunLayout(withFact(true))) return ctx
  return withFact(await probe())
}

/**
 * Gathers the facts and applies the rule. Writes nothing; the caller owns persistence.
 *
 * This sits between the app launching and the panes appearing, and the overwhelmingly
 * common case is a returning user whose marker is already set. So the two probes start as
 * placeholders and `settle` resolves each only if it matters: that launch does no I/O at
 * all, and an upgrading user's costs one store read and no permission probe.
 */
export async function resolveFirstRunLayout(deps: FirstRunLayoutDeps): Promise<FirstRunLayoutOutcome> {
  let ctx: FirstRunLayoutContext = {
    isAutomatedRun: deps.isAutomatedRun(),
    layoutAlreadyApplied: deps.layoutAlreadyApplied,
    // Placeholders. `settle` either replaces one or proves the answer doesn't turn on it,
    // so the decision below is the same one a fully-probed context would give.
    hasPersistedPaneState: false,
    hasFullDiskAccess: false,
  }
  ctx = await settle(ctx, 'hasPersistedPaneState', deps.hasPersistedPaneState)
  ctx = await settle(ctx, 'hasFullDiskAccess', deps.hasFullDiskAccess)

  const decision = decideFirstRunLayout(ctx)
  if (decision !== 'openHomeAndDownloads') return { kind: decision }

  // Reached only with Full Disk Access in hand, so this stat can't raise a TCC dialog.
  const hasDownloads = await deps.pathExists(FIRST_RUN_RIGHT_PATH)
  return {
    kind: decision,
    leftPath: FIRST_RUN_LEFT_PATH,
    rightPath: hasDownloads ? FIRST_RUN_RIGHT_PATH : FIRST_RUN_LEFT_PATH,
  }
}

/** Points each pane's active tab at its layout path, in place. */
export function applyFirstRunLayout(
  leftPaneTabs: PersistedPaneTabs,
  rightPaneTabs: PersistedPaneTabs,
  layout: FirstRunLayout,
): void {
  pointActiveTabAt(leftPaneTabs, layout.leftPath)
  pointActiveTabAt(rightPaneTabs, layout.rightPath)
}

function pointActiveTabAt(paneTabs: PersistedPaneTabs, path: string): void {
  // `.at(0)` rather than `[0]`, so an empty tab list types as `undefined` and the guard
  // below stays honest instead of reading as dead code.
  const target: PersistedTab | undefined =
    paneTabs.tabs.find((t) => t.id === paneTabs.activeTabId) ?? paneTabs.tabs.at(0)
  if (target) target.path = path
}
