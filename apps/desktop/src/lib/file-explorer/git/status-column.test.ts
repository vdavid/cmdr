/**
 * Tests for the status-column helpers.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { invoke } from '@tauri-apps/api/core'
import { fetchStatusMap, glyphFor, labelFor, type EntryStatusCode } from './status-column'

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}))

describe('status-column', () => {
  it('every code maps to a single-glyph string', () => {
    const codes: EntryStatusCode[] = [
      'modified',
      'added',
      'deleted',
      'renamed',
      'copied',
      'typechange',
      'untracked',
      'ignored',
      'conflicted',
    ]
    for (const code of codes) {
      const glyph = glyphFor(code)
      expect(glyph.length).toBe(1)
      expect(labelFor(code).length).toBeGreaterThan(0)
    }
  })

  it('maps modified, added, untracked, ignored to MAUI-style glyphs', () => {
    expect(glyphFor('modified')).toBe('M')
    expect(glyphFor('added')).toBe('A')
    expect(glyphFor('untracked')).toBe('?')
    expect(glyphFor('ignored')).toBe('!')
  })

  it('long-form labels match the expected sentence-case form', () => {
    expect(labelFor('typechange')).toBe('Type changed')
    expect(labelFor('untracked')).toBe('Untracked')
    expect(labelFor('conflicted')).toBe('Conflicted')
  })

  it('long-form labels never contain "error" or "failed"', () => {
    const codes: EntryStatusCode[] = [
      'modified',
      'added',
      'deleted',
      'renamed',
      'copied',
      'typechange',
      'untracked',
      'ignored',
      'conflicted',
    ]
    for (const code of codes) {
      const label = labelFor(code).toLowerCase()
      expect(label).not.toContain('error')
      expect(label).not.toContain('failed')
    }
  })
})

describe('fetchStatusMap', () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset()
  })

  it('returns null when the backend signals a timeout', async () => {
    vi.mocked(invoke).mockResolvedValueOnce({ data: [], timedOut: true })
    const map = await fetchStatusMap('/repo', '/repo/sub')
    expect(map).toBeNull()
  })

  it('builds a relative-path keyed map from EntryStatus rows', async () => {
    vi.mocked(invoke).mockResolvedValueOnce({
      data: [
        { relativePath: 'src/main.rs', code: 'modified' },
        { relativePath: 'README.md', code: 'untracked' },
      ],
      timedOut: false,
    })
    const map = await fetchStatusMap('/repo', '/repo')
    expect(map).not.toBeNull()
    if (!map) return
    expect(map.get('src/main.rs')).toBe('modified')
    expect(map.get('README.md')).toBe('untracked')
    expect(map.size).toBe(2)
  })

  it('keys rename entries on the new path', async () => {
    vi.mocked(invoke).mockResolvedValueOnce({
      data: [{ relativePath: 'old/name.rs -> new/name.rs', code: 'renamed' }],
      timedOut: false,
    })
    const map = await fetchStatusMap('/repo', '/repo')
    expect(map?.get('new/name.rs')).toBe('renamed')
    expect(map?.get('old/name.rs')).toBeUndefined()
  })
})
