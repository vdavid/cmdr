/**
 * Unit tests for the PURE step-state derivation (`indexing-steps.ts`), the heart
 * of the per-volume checklist. Given a volume's run kind + current pipeline
 * phase + live aggregation sub-phase, `deriveSteps` returns the ordered checklist
 * with each step's state (pending / active / done) and which one is active.
 *
 * Steps are COMPOSED from the events that fire for THIS volume, never a fixed
 * list: a network (SMB/MTP) scan never runs Save-the-file-list or Catch-up, so
 * those steps don't appear; an event-log roll-on collapses to a single step.
 *
 * Written test-first (TDD): these assertions failed against a missing module
 * before the implementation existed.
 */
import { describe, it, expect } from 'vitest'
import { deriveSteps, activeStep, type IndexStep, type IndexStepKind } from './indexing-steps'

/** The ordered step kinds, for readable assertions. */
function kinds(steps: IndexStep[]): IndexStepKind[] {
  return steps.map((s) => s.kind)
}

/** The status of one step kind in the derived list (or `undefined` if absent). */
function statusOf(steps: IndexStep[], kind: IndexStepKind): string | undefined {
  return steps.find((s) => s.kind === kind)?.status
}

describe('deriveSteps — local full scan', () => {
  it('lists all four local steps in order', () => {
    const steps = deriveSteps({ runKind: 'local', phase: 'scanning', aggregationSubPhase: undefined })
    expect(kinds(steps)).toEqual(['findFiles', 'saveFileList', 'computeFolderSizes', 'catchUp'])
  })

  it('scanning: find files active, the rest pending', () => {
    const steps = deriveSteps({ runKind: 'local', phase: 'scanning', aggregationSubPhase: undefined })
    expect(statusOf(steps, 'findFiles')).toBe('active')
    expect(statusOf(steps, 'saveFileList')).toBe('pending')
    expect(statusOf(steps, 'computeFolderSizes')).toBe('pending')
    expect(statusOf(steps, 'catchUp')).toBe('pending')
    expect(activeStep(steps)?.kind).toBe('findFiles')
  })

  it('saving entries: find files done, save active', () => {
    const steps = deriveSteps({ runKind: 'local', phase: 'aggregating', aggregationSubPhase: 'saving_entries' })
    expect(statusOf(steps, 'findFiles')).toBe('done')
    expect(statusOf(steps, 'saveFileList')).toBe('active')
    expect(statusOf(steps, 'computeFolderSizes')).toBe('pending')
    expect(statusOf(steps, 'catchUp')).toBe('pending')
  })

  it('computing folder sizes: find + save done, compute active', () => {
    const steps = deriveSteps({ runKind: 'local', phase: 'aggregating', aggregationSubPhase: 'computing' })
    expect(statusOf(steps, 'findFiles')).toBe('done')
    expect(statusOf(steps, 'saveFileList')).toBe('done')
    expect(statusOf(steps, 'computeFolderSizes')).toBe('active')
    expect(statusOf(steps, 'catchUp')).toBe('pending')
  })

  it('loading and sorting also activate the compute step (they are its sub-phases)', () => {
    for (const sub of ['loading', 'sorting', 'writing'] as const) {
      const steps = deriveSteps({ runKind: 'local', phase: 'aggregating', aggregationSubPhase: sub })
      expect(statusOf(steps, 'computeFolderSizes')).toBe('active')
      expect(statusOf(steps, 'saveFileList')).toBe('done')
    }
  })

  it('reconciling: only catch up is active, everything before it done', () => {
    const steps = deriveSteps({ runKind: 'local', phase: 'reconciling', aggregationSubPhase: undefined })
    expect(statusOf(steps, 'findFiles')).toBe('done')
    expect(statusOf(steps, 'saveFileList')).toBe('done')
    expect(statusOf(steps, 'computeFolderSizes')).toBe('done')
    expect(statusOf(steps, 'catchUp')).toBe('active')
    expect(activeStep(steps)?.kind).toBe('catchUp')
  })

  it('live (terminal): every step done, nothing active', () => {
    const steps = deriveSteps({ runKind: 'local', phase: 'live', aggregationSubPhase: undefined })
    expect(steps.every((s) => s.status === 'done')).toBe(true)
    expect(activeStep(steps)).toBeUndefined()
  })

  it('the aggregating phase with no sub-phase yet keeps find files done (save active)', () => {
    // The top-level phase advanced past scanning before the first aggregation
    // tick: scan is provably done, so find files is checked and save is active.
    const steps = deriveSteps({ runKind: 'local', phase: 'aggregating', aggregationSubPhase: undefined })
    expect(statusOf(steps, 'findFiles')).toBe('done')
    expect(statusOf(steps, 'saveFileList')).toBe('active')
  })

  it('derives from the aggregation sub-phase alone when the phase event is missing (mid-scan reload)', () => {
    // After a reload the transition-only phase event is gone, but the live
    // aggregation sub-phase still proves how far we are.
    const steps = deriveSteps({ runKind: 'local', phase: undefined, aggregationSubPhase: 'computing' })
    expect(statusOf(steps, 'findFiles')).toBe('done')
    expect(statusOf(steps, 'saveFileList')).toBe('done')
    expect(statusOf(steps, 'computeFolderSizes')).toBe('active')
  })

  it('reconcile-after-reload gap: with no signals the catch-up step stays pending (accepted)', () => {
    // Transition-only phase event missed AND aggregation already finished: we
    // cannot observe the reconcile, so catch up shows not-yet-active. Documented
    // accepted gap (in practice the surface is not rendered in this window).
    const steps = deriveSteps({ runKind: 'local', phase: undefined, aggregationSubPhase: undefined })
    expect(statusOf(steps, 'catchUp')).toBe('pending')
  })
})

describe('deriveSteps — network scan (SMB/MTP)', () => {
  it('lists only find files and compute folder sizes (no save, no catch up)', () => {
    const steps = deriveSteps({ runKind: 'network', phase: 'scanning', aggregationSubPhase: undefined })
    expect(kinds(steps)).toEqual(['findFiles', 'computeFolderSizes'])
  })

  it('scanning: find files active', () => {
    const steps = deriveSteps({ runKind: 'network', phase: 'scanning', aggregationSubPhase: undefined })
    expect(statusOf(steps, 'findFiles')).toBe('active')
    expect(statusOf(steps, 'computeFolderSizes')).toBe('pending')
  })

  it('computing folder sizes (driven off aggregation, not a top-level phase network never emits)', () => {
    const steps = deriveSteps({ runKind: 'network', phase: 'scanning', aggregationSubPhase: 'computing' })
    expect(statusOf(steps, 'findFiles')).toBe('done')
    expect(statusOf(steps, 'computeFolderSizes')).toBe('active')
  })

  it('live: both steps done', () => {
    const steps = deriveSteps({ runKind: 'network', phase: 'live', aggregationSubPhase: undefined })
    expect(steps.every((s) => s.status === 'done')).toBe(true)
  })
})

describe('deriveSteps — event-log roll-on (replay)', () => {
  it('collapses to a single update-index step, active while replaying', () => {
    const steps = deriveSteps({ runKind: 'replay', phase: 'replaying', aggregationSubPhase: undefined })
    expect(kinds(steps)).toEqual(['updateIndex'])
    expect(statusOf(steps, 'updateIndex')).toBe('active')
  })

  it('the single step is done once the volume goes live', () => {
    const steps = deriveSteps({ runKind: 'replay', phase: 'live', aggregationSubPhase: undefined })
    expect(statusOf(steps, 'updateIndex')).toBe('done')
  })
})

describe('activeStep', () => {
  it('returns the single active step, or undefined when all are done', () => {
    const scanning = deriveSteps({ runKind: 'local', phase: 'scanning', aggregationSubPhase: undefined })
    expect(activeStep(scanning)?.kind).toBe('findFiles')
    const done = deriveSteps({ runKind: 'local', phase: 'live', aggregationSubPhase: undefined })
    expect(activeStep(done)).toBeUndefined()
  })
})
