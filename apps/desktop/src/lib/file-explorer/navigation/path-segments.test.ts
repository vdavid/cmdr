import { describe, it, expect } from 'vitest'
import { splitPathSegments } from './path-segments'

describe('splitPathSegments', () => {
  it('returns empty array for empty input', () => {
    expect(splitPathSegments('')).toEqual([])
  })

  it('keeps leading slash marker for absolute paths', () => {
    const segs = splitPathSegments('/foo/bar')
    expect(segs).toEqual([
      { text: '', gitPortal: false },
      { text: 'foo', gitPortal: false },
      { text: 'bar', gitPortal: false },
    ])
  })

  it('flags .git and everything after as git-portal', () => {
    const segs = splitPathSegments('/repo/.git/branches/main')
    expect(segs).toEqual([
      { text: '', gitPortal: false },
      { text: 'repo', gitPortal: false },
      { text: '.git', gitPortal: true },
      { text: 'branches', gitPortal: true },
      { text: 'main', gitPortal: true },
    ])
  })

  it('handles a path that is only the .git portal root', () => {
    const segs = splitPathSegments('~/projects/repo/.git')
    expect(segs.at(-1)).toEqual({ text: '.git', gitPortal: true })
  })

  it('does not flag segments before .git', () => {
    const segs = splitPathSegments('/home/user/.config/.git/raw/HEAD')
    expect(segs.find((s) => s.text === '.config')).toEqual({ text: '.config', gitPortal: false })
    expect(segs.find((s) => s.text === '.git')?.gitPortal).toBe(true)
    expect(segs.find((s) => s.text === 'raw')?.gitPortal).toBe(true)
    expect(segs.find((s) => s.text === 'HEAD')?.gitPortal).toBe(true)
  })

  it('drops doubled-slash interior empties', () => {
    const segs = splitPathSegments('//foo//bar')
    // Leading-empty kept; interior-empty dropped.
    expect(segs.map((s) => s.text)).toEqual(['', 'foo', 'bar'])
  })
})
