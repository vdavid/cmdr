import { describe, it, expect } from 'vitest'
import type { RowStatus } from '$lib/ipc/bindings'
import { rowStatusView } from './row-status'

const PROBLEMS: RowStatus[] = [
  { type: 'invalidName', reason: { type: 'empty' } },
  { type: 'duplicate' },
  { type: 'targetExists' },
  { type: 'missing' },
]

describe('rowStatusView', () => {
  it('keeps ready and unchanged rows quiet: no glyph, words for screen readers only', () => {
    for (const status of [{ type: 'ready' }, { type: 'unchanged' }] satisfies RowStatus[]) {
      const view = rowStatusView(status)
      expect(view.glyph).toBeNull()
      expect(view.isProblem).toBe(false)
      expect(view.label.key).toBeTruthy()
    }
    expect(rowStatusView({ type: 'ready' }).label.key).not.toBe(rowStatusView({ type: 'unchanged' }).label.key)
  })

  it('gives each kind of problem its own glyph, a short label, and the full reason', () => {
    const views = PROBLEMS.map(rowStatusView)
    expect(views.every((v) => v.isProblem && v.glyph !== null && v.reason !== null)).toBe(true)
    expect(new Set(views.map((v) => v.glyph)).size).toBe(PROBLEMS.length)
    expect(new Set(views.map((v) => v.label.key)).size).toBe(PROBLEMS.length)
  })

  it('names the reason a name is invalid, and the character a name can’t contain', () => {
    const reasons = (['empty', 'tooLong', 'reserved'] as const).map(
      (type) => rowStatusView({ type: 'invalidName', reason: { type } }).reason?.key,
    )
    expect(new Set(reasons).size).toBe(3)

    const slash = rowStatusView({ type: 'invalidName', reason: { type: 'disallowedCharacter', character: '/' } })
    expect(slash.glyph).toBe(rowStatusView({ type: 'invalidName', reason: { type: 'empty' } }).glyph)
    expect(slash.label.params).toEqual({ character: '/' })
    expect(slash.reason?.params).toEqual({ character: '/' })
  })
})
