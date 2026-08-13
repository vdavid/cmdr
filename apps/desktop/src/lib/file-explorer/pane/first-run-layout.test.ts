/**
 * The first-run layout decides, once per install, whether to open the right pane on
 * `~/Downloads`. A wrong "yes" is unrecoverable: pane paths persist like any navigation,
 * so a layout applied to someone who already had one silently BECOMES their layout.
 * That's why the decision is a pure function and why this file walks the whole matrix.
 */
import { describe, it, expect, vi } from 'vitest'
import {
  decideFirstRunLayout,
  resolveFirstRunLayout,
  applyFirstRunLayout,
  FIRST_RUN_LEFT_PATH,
  FIRST_RUN_RIGHT_PATH,
  type FirstRunLayoutContext,
  type FirstRunLayoutDecision,
  type FirstRunLayoutDeps,
} from './first-run-layout'
import type { PersistedPaneTabs, PersistedTab } from '../tabs/tab-types'

/** Every input combination, with the decision each one must produce. */
const matrix: { ctx: FirstRunLayoutContext; expected: FirstRunLayoutDecision }[] = [
  // Ordinary launches, no marker yet, no pane state: the FDA probe alone decides.
  { ctx: cx({ hasFullDiskAccess: true }), expected: 'openHomeAndDownloads' },
  { ctx: cx({ hasFullDiskAccess: false }), expected: 'leaveAlone' },

  // Pane state on disk means a prior install. Record the marker, keep hands off the panes.
  { ctx: cx({ hasPersistedPaneState: true, hasFullDiskAccess: true }), expected: 'markAlreadyLaidOut' },
  { ctx: cx({ hasPersistedPaneState: true, hasFullDiskAccess: false }), expected: 'markAlreadyLaidOut' },

  // Marker already set: done, forever, whatever else is true.
  { ctx: cx({ layoutAlreadyApplied: true, hasFullDiskAccess: true }), expected: 'leaveAlone' },
  { ctx: cx({ layoutAlreadyApplied: true, hasFullDiskAccess: false }), expected: 'leaveAlone' },
  {
    ctx: cx({ layoutAlreadyApplied: true, hasPersistedPaneState: true, hasFullDiskAccess: true }),
    expected: 'leaveAlone',
  },
  {
    ctx: cx({ layoutAlreadyApplied: true, hasPersistedPaneState: true, hasFullDiskAccess: false }),
    expected: 'leaveAlone',
  },

  // An automated run never lays out and never records anything.
  { ctx: cx({ isAutomatedRun: true, hasFullDiskAccess: true }), expected: 'leaveAlone' },
  { ctx: cx({ isAutomatedRun: true, hasFullDiskAccess: false }), expected: 'leaveAlone' },
  {
    ctx: cx({ isAutomatedRun: true, hasPersistedPaneState: true, hasFullDiskAccess: true }),
    expected: 'leaveAlone',
  },
  {
    ctx: cx({ isAutomatedRun: true, hasPersistedPaneState: true, hasFullDiskAccess: false }),
    expected: 'leaveAlone',
  },
  {
    ctx: cx({ isAutomatedRun: true, layoutAlreadyApplied: true, hasFullDiskAccess: true }),
    expected: 'leaveAlone',
  },
  {
    ctx: cx({ isAutomatedRun: true, layoutAlreadyApplied: true, hasFullDiskAccess: false }),
    expected: 'leaveAlone',
  },
  {
    ctx: cx({
      isAutomatedRun: true,
      layoutAlreadyApplied: true,
      hasPersistedPaneState: true,
      hasFullDiskAccess: true,
    }),
    expected: 'leaveAlone',
  },
  {
    ctx: cx({
      isAutomatedRun: true,
      layoutAlreadyApplied: true,
      hasPersistedPaneState: true,
      hasFullDiskAccess: false,
    }),
    expected: 'leaveAlone',
  },
]

function cx(overrides: Partial<FirstRunLayoutContext>): FirstRunLayoutContext {
  return {
    isAutomatedRun: false,
    layoutAlreadyApplied: false,
    hasPersistedPaneState: false,
    hasFullDiskAccess: false,
    ...overrides,
  }
}

function describeCtx(ctx: FirstRunLayoutContext): string {
  return [
    ctx.isAutomatedRun ? 'automated' : 'human',
    ctx.layoutAlreadyApplied ? 'marked' : 'unmarked',
    ctx.hasPersistedPaneState ? 'has pane state' : 'no pane state',
    ctx.hasFullDiskAccess ? 'FDA granted' : 'FDA not granted',
  ].join(', ')
}

describe('decideFirstRunLayout', () => {
  it('covers every input combination', () => {
    expect(matrix).toHaveLength(2 ** 4)
    expect(new Set(matrix.map((row) => describeCtx(row.ctx))).size).toBe(2 ** 4)
  })

  for (const { ctx, expected } of matrix) {
    it(`returns "${expected}" for ${describeCtx(ctx)}`, () => {
      expect(decideFirstRunLayout(ctx)).toBe(expected)
    })
  }

  it('opens Downloads in exactly one of the sixteen combinations', () => {
    const applying = matrix.filter((row) => decideFirstRunLayout(row.ctx) === 'openHomeAndDownloads')
    expect(applying).toHaveLength(1)
    expect(applying[0]?.ctx).toEqual(cx({ hasFullDiskAccess: true }))
  })

  it('treats a not-yet-answered Full Disk Access prompt like a refusal', () => {
    // The probe is a boolean: "never asked" and "declined" both read as `false`, and
    // both must leave the panes on the home folder.
    expect(decideFirstRunLayout(cx({ hasFullDiskAccess: false }))).toBe('leaveAlone')
  })
})

function stubDeps(overrides: Partial<FirstRunLayoutDeps> = {}) {
  const pathExists = vi.fn<(path: string) => Promise<boolean>>().mockResolvedValue(true)
  const hasPersistedPaneState = vi.fn<() => Promise<boolean>>().mockResolvedValue(false)
  const hasFullDiskAccess = vi.fn<() => Promise<boolean>>().mockResolvedValue(true)
  const deps: FirstRunLayoutDeps = {
    isAutomatedRun: () => false,
    layoutAlreadyApplied: false,
    hasPersistedPaneState,
    hasFullDiskAccess,
    pathExists,
    ...overrides,
  }
  return { deps, pathExists, hasPersistedPaneState, hasFullDiskAccess }
}

/** Every probe the resolver can make. Each one costs a store open or an IPC round trip. */
function expectNoProbes(stubs: ReturnType<typeof stubDeps>): void {
  expect(stubs.hasPersistedPaneState).not.toHaveBeenCalled()
  expect(stubs.hasFullDiskAccess).not.toHaveBeenCalled()
  expect(stubs.pathExists).not.toHaveBeenCalled()
}

describe('resolveFirstRunLayout', () => {
  it('opens home on the left and Downloads on the right for a fresh install with Full Disk Access', async () => {
    const { deps } = stubDeps()
    await expect(resolveFirstRunLayout(deps)).resolves.toEqual({
      kind: 'openHomeAndDownloads',
      leftPath: FIRST_RUN_LEFT_PATH,
      rightPath: FIRST_RUN_RIGHT_PATH,
    })
  })

  it('falls back to home when the Downloads folder is missing', async () => {
    const { deps, pathExists } = stubDeps()
    pathExists.mockResolvedValue(false)
    await expect(resolveFirstRunLayout(deps)).resolves.toEqual({
      kind: 'openHomeAndDownloads',
      leftPath: FIRST_RUN_LEFT_PATH,
      rightPath: FIRST_RUN_LEFT_PATH,
    })
  })

  it('asks the caller to record the marker for an install that already has pane state', async () => {
    // The destructive case: every existing user boots the new build with Full Disk Access
    // and no marker. Backfilling here is what keeps their real layout intact.
    const stubs = stubDeps()
    stubs.hasPersistedPaneState.mockResolvedValue(true)
    await expect(resolveFirstRunLayout(stubs.deps)).resolves.toEqual({ kind: 'markAlreadyLaidOut' })
  })

  it('matches the fully-probed decision for every combination of facts', async () => {
    // `settle` skips a probe whose value can't change the answer, which is only sound
    // while the rule keeps the two properties documented on it. This walks the same 16
    // rows the pure matrix does, but through the resolver, so a rule change that breaks
    // those properties fails HERE even though the pure function stays correct.
    for (const { ctx } of matrix) {
      const stubs = stubDeps({
        isAutomatedRun: () => ctx.isAutomatedRun,
        layoutAlreadyApplied: ctx.layoutAlreadyApplied,
      })
      stubs.hasPersistedPaneState.mockResolvedValue(ctx.hasPersistedPaneState)
      stubs.hasFullDiskAccess.mockResolvedValue(ctx.hasFullDiskAccess)

      const outcome = await resolveFirstRunLayout(stubs.deps)

      expect(outcome.kind, `lazy resolution diverged for ${describeCtx(ctx)}`).toBe(decideFirstRunLayout(ctx))
    }
  })

  it('never touches the filesystem for an install that already has pane state', async () => {
    const stubs = stubDeps()
    stubs.hasPersistedPaneState.mockResolvedValue(true)
    await resolveFirstRunLayout(stubs.deps)
    expect(stubs.pathExists).not.toHaveBeenCalled()
  })

  it('never asks about Full Disk Access once pane state settles the answer', async () => {
    // Pane state present means `markAlreadyLaidOut` whichever way the permission goes, so
    // the probe would be a round trip that can't change anything.
    const stubs = stubDeps()
    stubs.hasPersistedPaneState.mockResolvedValue(true)
    await resolveFirstRunLayout(stubs.deps)
    expect(stubs.hasFullDiskAccess).not.toHaveBeenCalled()
  })

  it('never probes a folder without Full Disk Access, so no permission dialog can appear', async () => {
    const stubs = stubDeps()
    stubs.hasFullDiskAccess.mockResolvedValue(false)
    await expect(resolveFirstRunLayout(stubs.deps)).resolves.toEqual({ kind: 'leaveAlone' })
    expect(stubs.pathExists).not.toHaveBeenCalled()
  })

  it('leaves an automated run alone entirely, probing nothing', async () => {
    const stubs = stubDeps({ isAutomatedRun: () => true })
    await expect(resolveFirstRunLayout(stubs.deps)).resolves.toEqual({ kind: 'leaveAlone' })
    expectNoProbes(stubs)
  })

  it('does no work at all on an install whose marker is already set', async () => {
    // The overwhelmingly common launch. It sits between the app starting and the panes
    // appearing, so it has to cost nothing: no store open, no permission probe, no stat.
    const stubs = stubDeps({ layoutAlreadyApplied: true })
    await expect(resolveFirstRunLayout(stubs.deps)).resolves.toEqual({ kind: 'leaveAlone' })
    expectNoProbes(stubs)
  })

  it('probes the Downloads folder and nothing else', async () => {
    const { deps, pathExists } = stubDeps()
    await resolveFirstRunLayout(deps)
    expect(pathExists.mock.calls).toEqual([[FIRST_RUN_RIGHT_PATH]])
  })

  it('asks each probe at most once on a true first run', async () => {
    const stubs = stubDeps()
    await resolveFirstRunLayout(stubs.deps)
    expect(stubs.hasPersistedPaneState).toHaveBeenCalledTimes(1)
    expect(stubs.hasFullDiskAccess).toHaveBeenCalledTimes(1)
    expect(stubs.pathExists).toHaveBeenCalledTimes(1)
  })
})

function tab(id: string, path: string): PersistedTab {
  return { id, path, volumeId: 'root', sortBy: 'name', sortOrder: 'ascending', viewMode: 'brief', pinned: false }
}

describe('applyFirstRunLayout', () => {
  it('points the active tab of each pane at its layout path', () => {
    const left: PersistedPaneTabs = { tabs: [tab('l1', '~'), tab('l2', '~')], activeTabId: 'l2' }
    const right: PersistedPaneTabs = { tabs: [tab('r1', '~')], activeTabId: 'r1' }

    applyFirstRunLayout(left, right, { leftPath: '~', rightPath: '~/Downloads' })

    expect(left.tabs.map((t) => t.path)).toEqual(['~', '~'])
    expect(right.tabs.map((t) => t.path)).toEqual(['~/Downloads'])
  })

  it('falls back to the first tab when the active tab ID does not match', () => {
    const left: PersistedPaneTabs = { tabs: [tab('l1', '~')], activeTabId: 'gone' }
    const right: PersistedPaneTabs = { tabs: [tab('r1', '~')], activeTabId: 'gone' }

    applyFirstRunLayout(left, right, { leftPath: '~', rightPath: '~/Downloads' })

    expect(right.tabs[0]?.path).toBe('~/Downloads')
    expect(left.tabs[0]?.path).toBe('~')
  })

  it('tolerates a pane with no tabs at all', () => {
    const left: PersistedPaneTabs = { tabs: [], activeTabId: 'gone' }
    const right: PersistedPaneTabs = { tabs: [], activeTabId: 'gone' }

    expect(() => {
      applyFirstRunLayout(left, right, { leftPath: '~', rightPath: '~/Downloads' })
    }).not.toThrow()
  })
})
