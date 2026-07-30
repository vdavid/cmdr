import { describe, expect, it } from 'vitest'
import { contextUsageState, contextUsagePercent, type ContextUsage } from './ask-cmdr-context-usage'

const usage = (estimatedTokens: number, budgetTokens: number, elidedResults = 0): ContextUsage => ({
  estimatedTokens,
  budgetTokens,
  elidedResults,
})

describe('contextUsageState', () => {
  it('is calm below the 80% threshold', () => {
    expect(contextUsageState(usage(31_200, 60_000))).toBe('calm')
    // Just under the boundary stays calm, so the state changes exactly where documented.
    expect(contextUsageState(usage(47_000, 60_000))).toBe('calm')
  })

  it('changes state on the percentage it displays, not on the exact ratio', () => {
    // 47,999 of 60,000 is 79.998%, which the gauge SHOWS as 80%. The state has to follow
    // the number the user can see, or the bar reads "80%" while behaving as calm.
    expect(contextUsagePercent(usage(47_999, 60_000))).toBe(80)
    expect(contextUsageState(usage(47_999, 60_000))).toBe('filling')
  })

  it('is filling from 80% of the budget, before anything is dropped', () => {
    expect(contextUsageState(usage(48_000, 60_000))).toBe('filling')
    expect(contextUsageState(usage(59_000, 60_000))).toBe('filling')
  })

  it('is set aside once history was dropped, at any fill level', () => {
    // The count is what makes this state true, not the percentage: a turn can drop history
    // and still assemble small, and the user needs to know something left the model's view.
    expect(contextUsageState(usage(12_000, 60_000, 3))).toBe('setAside')
    expect(contextUsageState(usage(59_000, 60_000, 1))).toBe('setAside')
  })

  it('treats going over the budget as set aside rather than a fourth state', () => {
    // Over budget is not a user-visible failure: the turn worked, history made room.
    expect(contextUsageState(usage(61_000, 60_000))).toBe('setAside')
  })

  it('reports nothing when no turn has been measured', () => {
    expect(contextUsageState(null)).toBe('unmeasured')
    // A zero budget can't be turned into a percentage, so it is not a measurement.
    expect(contextUsageState(usage(1_000, 0))).toBe('unmeasured')
  })
})

describe('contextUsagePercent', () => {
  it('rounds to a whole percent', () => {
    expect(contextUsagePercent(usage(31_200, 60_000))).toBe(52)
    // A measured turn never rounds down to 0%: an empty bar would read as "nothing used".
    expect(contextUsagePercent(usage(1, 60_000))).toBe(1)
  })

  it('never exceeds 100, so the bar cannot overflow its track', () => {
    expect(contextUsagePercent(usage(90_000, 60_000))).toBe(100)
  })

  it('is 0 when there is nothing to report', () => {
    expect(contextUsagePercent(null)).toBe(0)
  })
})
