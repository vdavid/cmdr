import { describe, expect, it } from 'vitest'
import { buildSnapshotLabel } from './snapshot-label'

describe('buildSnapshotLabel', () => {
  it('returns the pattern as-is for filename mode', () => {
    expect(buildSnapshotLabel({ mode: 'filename', query: '*.pdf' })).toBe('*.pdf')
  })

  it('wraps the pattern in slashes for regex mode', () => {
    expect(buildSnapshotLabel({ mode: 'regex', query: '^foo.*\\.log$' })).toBe('/^foo.*\\.log$/')
  })

  it("prefers the AI prompt over the AI's translated query for AI mode", () => {
    expect(
      buildSnapshotLabel({
        mode: 'ai',
        query: '*.pdf',
        aiPrompt: 'find my pdf invoices',
      }),
    ).toBe('find my pdf invoices')
  })

  it('falls back to query when no AI prompt is supplied in AI mode', () => {
    expect(buildSnapshotLabel({ mode: 'ai', query: 'large screenshots' })).toBe('large screenshots')
  })

  it('truncates a long AI prompt with a single-char ellipsis', () => {
    const long = 'a'.repeat(60)
    const out = buildSnapshotLabel({ mode: 'ai', query: '', aiPrompt: long })
    // 40-char cap: keep 39 'a' + 1 ellipsis = 40 visible chars
    expect(out.length).toBe(40)
    expect(out.endsWith('…')).toBe(true)
  })

  it('returns a sensible fallback for an empty AI prompt', () => {
    expect(buildSnapshotLabel({ mode: 'ai', query: '', aiPrompt: '' })).toBe('Search')
  })

  it('returns a sensible fallback for an empty filename pattern', () => {
    expect(buildSnapshotLabel({ mode: 'filename', query: '   ' })).toBe('Search')
  })

  it('trims trailing whitespace before the ellipsis when truncating', () => {
    // 41 chars: 40 'a' + 1 space → cap 40 keeps 39 chars + '…'. We expect no trailing
    // space before the ellipsis even if the cut landed on one.
    const text = 'a'.repeat(38) + '   xyz'
    const out = buildSnapshotLabel({ mode: 'ai', query: '', aiPrompt: text })
    expect(out.endsWith('…')).toBe(true)
    expect(out).not.toMatch(/ …$/)
  })
})
