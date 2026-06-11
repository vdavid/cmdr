import { describe, it, expect } from 'vitest'
import { matchEntries, type SelectionMatchQuery, type MatchAccessors } from './selection-matching'

function fromNames(list: string[]): MatchAccessors {
  return { getNameFor: (i) => list[i] }
}

describe('matchEntries: glob mode', () => {
  it('matches *.png against PNG basenames', () => {
    const list = ['foo.png', 'bar.txt', 'baz.PNG', 'noext']
    const q: SelectionMatchQuery = { pattern: '*.png', kind: 'glob', caseSensitive: false }
    expect(matchEntries(fromNames(list), list.length, q)).toEqual([0, 2])
  })

  it('honors case sensitivity', () => {
    const list = ['foo.png', 'baz.PNG']
    const q: SelectionMatchQuery = { pattern: '*.png', kind: 'glob', caseSensitive: true }
    expect(matchEntries(fromNames(list), list.length, q)).toEqual([0])
  })

  it('? matches a single character', () => {
    const list = ['a.txt', 'ab.txt', 'abc.txt']
    const q: SelectionMatchQuery = { pattern: '?.txt', kind: 'glob', caseSensitive: false }
    expect(matchEntries(fromNames(list), list.length, q)).toEqual([0])
  })

  it('escapes regex metacharacters', () => {
    const list = ['foo+bar', 'foobar', 'foo+', 'foo+barbaz']
    const q: SelectionMatchQuery = { pattern: 'foo+bar', kind: 'glob', caseSensitive: false }
    expect(matchEntries(fromNames(list), list.length, q)).toEqual([0])
  })

  it('returns [] on an empty pattern', () => {
    const list = ['a.txt', 'b.txt']
    const q: SelectionMatchQuery = { pattern: '', kind: 'glob', caseSensitive: false }
    expect(matchEntries(fromNames(list), list.length, q)).toEqual([])
  })

  it('treats a whitespace-only pattern as empty', () => {
    const list = ['a.txt']
    const q: SelectionMatchQuery = { pattern: '   ', kind: 'glob', caseSensitive: false }
    expect(matchEntries(fromNames(list), list.length, q)).toEqual([])
  })
})

describe('matchEntries: regex mode', () => {
  it('matches a JS regex', () => {
    const list = ['app.log', 'app.LOG', 'README.md', 'app']
    const q: SelectionMatchQuery = { pattern: '^app\\.log$', kind: 'regex', caseSensitive: false }
    expect(matchEntries(fromNames(list), list.length, q)).toEqual([0, 1])
  })

  it('returns [] on bad regex (SyntaxError)', () => {
    const list = ['foo']
    const q: SelectionMatchQuery = { pattern: '(unclosed', kind: 'regex', caseSensitive: false }
    expect(matchEntries(fromNames(list), list.length, q)).toEqual([])
  })

  it('honors case sensitivity in regex mode', () => {
    const list = ['Foo', 'foo']
    const q: SelectionMatchQuery = { pattern: '^Foo$', kind: 'regex', caseSensitive: true }
    expect(matchEntries(fromNames(list), list.length, q)).toEqual([0])
  })
})

describe('matchEntries: size predicate', () => {
  const list = ['a.bin', 'b.bin', 'c.bin']
  const sizes: (number | null)[] = [100, 5_000_000, null] // last is a folder (no size)
  const accessors: MatchAccessors = {
    getNameFor: (i) => list[i],
    getSizeFor: (i) => sizes[i],
  }
  it('matches >= min', () => {
    const q: SelectionMatchQuery = {
      pattern: '*',
      kind: 'glob',
      caseSensitive: false,
      size: { kind: 'gte', min: 1_000_000 },
    }
    expect(matchEntries(accessors, list.length, q)).toEqual([1])
  })

  it('matches <= max', () => {
    const q: SelectionMatchQuery = {
      pattern: '*',
      kind: 'glob',
      caseSensitive: false,
      size: { kind: 'lte', max: 1000 },
    }
    expect(matchEntries(accessors, list.length, q)).toEqual([0])
  })

  it('matches between bounds (inclusive)', () => {
    const q: SelectionMatchQuery = {
      pattern: '*',
      kind: 'glob',
      caseSensitive: false,
      size: { kind: 'between', min: 50, max: 5_000_000 },
    }
    expect(matchEntries(accessors, list.length, q)).toEqual([0, 1])
  })

  it('match-all `*` + size predicate selects only files over the bound (filter-only contract)', () => {
    // The contract M2's filter-only fix relies on: an empty name bar becomes a
    // match-all glob `*`, and the size predicate alone picks the matching files.
    // A folder snapshot of a 2 MB file + small files + a dir (null size) returns
    // exactly the 2 MB file's index.
    const names = ['notes.txt', 'photo.bin', 'subdir']
    const fileSizes: (number | null)[] = [1000, 2_000_000, null] // last is a dir
    const acc: MatchAccessors = {
      getNameFor: (i) => names[i],
      getSizeFor: (i) => fileSizes[i],
    }
    const q: SelectionMatchQuery = {
      pattern: '*',
      kind: 'glob',
      caseSensitive: false,
      size: { kind: 'gte', min: 1_048_576 },
    }
    expect(matchEntries(acc, names.length, q)).toEqual([1])
  })

  it('drops entries with no size when a size predicate is set', () => {
    const q: SelectionMatchQuery = {
      pattern: '*',
      kind: 'glob',
      caseSensitive: false,
      size: { kind: 'gte', min: 0 },
    }
    expect(matchEntries(accessors, list.length, q)).toEqual([0, 1])
  })
})

describe('matchEntries: date predicate', () => {
  const list = ['old.txt', 'new.txt']
  const mtimes = [1_000_000, 2_000_000]
  const accessors: MatchAccessors = {
    getNameFor: (i) => list[i],
    getMtimeFor: (i) => mtimes[i],
  }
  it('matches after threshold (inclusive)', () => {
    const q: SelectionMatchQuery = {
      pattern: '*',
      kind: 'glob',
      caseSensitive: false,
      date: { kind: 'after', after: 1_500_000 },
    }
    expect(matchEntries(accessors, list.length, q)).toEqual([1])
  })

  it('matches before threshold (inclusive)', () => {
    const q: SelectionMatchQuery = {
      pattern: '*',
      kind: 'glob',
      caseSensitive: false,
      date: { kind: 'before', before: 1_500_000 },
    }
    expect(matchEntries(accessors, list.length, q)).toEqual([0])
  })

  it('composes pattern + size + date with AND semantics', () => {
    const accessorsAll: MatchAccessors = {
      getNameFor: (i) => ['a.png', 'b.png', 'c.txt'][i],
      getSizeFor: (i) => [100, 5000, 100][i],
      getMtimeFor: (i) => [1000, 2000, 3000][i],
    }
    const q: SelectionMatchQuery = {
      pattern: '*.png',
      kind: 'glob',
      caseSensitive: false,
      size: { kind: 'gte', min: 1000 },
      date: { kind: 'after', after: 1500 },
    }
    expect(matchEntries(accessorsAll, 3, q)).toEqual([1])
  })
})

describe('matchEntries: snapshot-pane accessor', () => {
  it('matches against the friendly path (what `entry.name` returns on snapshot panes)', () => {
    // On snapshot panes, the accessor returns the displayed friendly full path
    // (with `~` for home), not the basename.
    const friendlyPaths = ['~/Documents/foo.png', '~/Library/Caches/bar.png', 'README.md']
    const q: SelectionMatchQuery = { pattern: '*Documents*', kind: 'glob', caseSensitive: false }
    expect(matchEntries(fromNames(friendlyPaths), friendlyPaths.length, q)).toEqual([0])
  })
})

describe('matchEntries: stress invariants', () => {
  // Deterministic LCG so the same seed yields the same sequence every run.
  function rng(seed: number): () => number {
    let s = seed
    return () => {
      s = (s * 1103515245 + 12345) & 0x7fffffff
      return s
    }
  }
  const charset = 'abcdef.0123456789'
  function randStr(r: () => number, maxLen: number): string {
    const len = (r() % maxLen) + 1
    let out = ''
    for (let i = 0; i < len; i++) out += charset[r() % charset.length]
    return out
  }

  it('returns indices in [0, total), no dups, sorted, ≤ total — many random inputs', () => {
    for (let seed = 1; seed <= 100; seed++) {
      const r = rng(seed)
      const total = (r() % 100) + 1
      const names = Array.from({ length: total }, () => randStr(r, 10))
      const patternFragment = r() % 4 === 0 ? '' : randStr(r, 3)
      const q: SelectionMatchQuery = {
        pattern: `*${patternFragment}*`,
        kind: 'glob',
        caseSensitive: false,
      }
      const out = matchEntries(fromNames(names), names.length, q)
      expect(out.length).toBeLessThanOrEqual(names.length)
      expect(new Set(out).size).toBe(out.length)
      // Sorted ascending: matchEntries iterates 0..total in order.
      for (let i = 1; i < out.length; i++) {
        expect(out[i]).toBeGreaterThan(out[i - 1])
      }
      for (const idx of out) {
        expect(idx).toBeGreaterThanOrEqual(0)
        expect(idx).toBeLessThan(names.length)
      }
    }
  })
})
