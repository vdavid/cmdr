import { describe, it, expect } from 'vitest'
import { paneSplitLeak } from './pane-split-leak.js'

describe('paneSplitLeak', () => {
  it('reports nothing for the centered split', () => {
    expect(paneSplitLeak('50%')).toBeNull()
  })

  it('reports a split a test dragged off center', () => {
    // The real leak: `brief-cursor-visibility` dragged the divider ~200 px left and
    // left it there, so every later spec met a ~340 px left pane. Full view then
    // dropped its date column, and `pane-controls` couldn't find the date chevron.
    expect(paneSplitLeak('31.4%')).toBe('31.4%')
  })

  it('reports nothing when the pane wrapper is missing, so the guard never masks the real failure', () => {
    expect(paneSplitLeak(null)).toBeNull()
    expect(paneSplitLeak('')).toBeNull()
  })
})
