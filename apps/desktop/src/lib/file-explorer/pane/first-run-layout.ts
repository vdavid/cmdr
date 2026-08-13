/**
 * The one-shot first-run pane layout: on a fresh install that already has Full Disk
 * Access, the left pane opens on the home folder and the right pane on `~/Downloads`.
 * It happens once per install and never again; every later launch restores wherever the
 * user left off.
 *
 * ⚠️ An install that already carries pane state gets its marker recorded WITHOUT its
 * panes being touched (`markAlreadyLaidOut`). Pane paths persist like any navigation, so
 * a layout applied to someone who already had one silently BECOMES their layout, with
 * nothing to undo it. Without that branch the rule fires for every user upgrading into
 * the build that introduces it, since they all have Full Disk Access and no marker.
 *
 * The decision is a pure function so the whole matrix can be walked in a unit test, and
 * so the `~/Downloads` probe is structurally unreachable until Full Disk Access is
 * confirmed: `~/Downloads` sits behind a TCC gate, and even stat'ing it without the
 * permission can raise a system dialog the user has no context for.
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

export interface FirstRunLayoutDeps {
  isAutomatedRun: () => boolean
  /** Already loaded with the rest of the app status, so a value rather than a probe. */
  layoutAlreadyApplied: boolean
  hasPersistedPaneState: () => Promise<boolean>
  hasFullDiskAccess: () => Promise<boolean>
  pathExists: (path: string) => Promise<boolean>
  /** Records the marker durably (no debounce): startup is followed by things that can quit. */
  markLaidOut: () => Promise<void>
}

/**
 * Gathers the facts, applies the rule, and records the marker. Returns the paths to open,
 * or `null` when the panes should keep whatever was persisted.
 */
export async function resolveFirstRunLayout(deps: FirstRunLayoutDeps): Promise<FirstRunLayout | null> {
  const decision = decideFirstRunLayout({
    isAutomatedRun: deps.isAutomatedRun(),
    layoutAlreadyApplied: deps.layoutAlreadyApplied,
    hasPersistedPaneState: await deps.hasPersistedPaneState(),
    hasFullDiskAccess: await deps.hasFullDiskAccess(),
  })

  if (decision === 'leaveAlone') return null
  if (decision === 'markAlreadyLaidOut') {
    await deps.markLaidOut()
    return null
  }

  // Reached only with Full Disk Access in hand, so this stat can't raise a TCC dialog.
  const hasDownloads = await deps.pathExists(FIRST_RUN_RIGHT_PATH)
  await deps.markLaidOut()
  return { leftPath: FIRST_RUN_LEFT_PATH, rightPath: hasDownloads ? FIRST_RUN_RIGHT_PATH : FIRST_RUN_LEFT_PATH }
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
