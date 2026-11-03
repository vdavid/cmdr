/**
 * Find over rendered DOM text: what counts as a hit (case-folded, across a region's text
 * nodes, never across regions), what's skipped, and how hits are painted with and without
 * the CSS Custom Highlight API.
 */

import { afterEach, describe, expect, it, vi } from 'vitest'
import {
  clearFindMatches,
  findTextRanges,
  firstRangeFrom,
  foldCasePreservingLength,
  matchOffsets,
  paintFindMatches,
} from './find-in-dom'

function container(html: string): HTMLElement {
  const el = document.createElement('div')
  // Test fixture markup, written here; no message text reaches it.
  el.innerHTML = html
  document.body.appendChild(el)
  return el
}

afterEach(() => {
  document.body.innerHTML = ''
  vi.unstubAllGlobals()
})

describe('foldCasePreservingLength', () => {
  it('lowers each character', () => {
    expect(foldCasePreservingLength('Hello WORLD')).toBe('hello world')
  })

  it('keeps a character whose lowercase is longer, so offsets stay aligned', () => {
    // `İ` lowers to `i̇` (two units); folding it would shift every later offset.
    const text = 'İstanbul Files'
    const folded = foldCasePreservingLength(text)
    expect(folded).toHaveLength(text.length)
    expect(folded.indexOf('files')).toBe(text.indexOf('Files'))
  })
})

describe('matchOffsets', () => {
  it('finds every hit without overlaps', () => {
    expect(matchOffsets('aaaa', 'aa')).toEqual([0, 2])
    expect(matchOffsets('a cat, a hat', 'at')).toEqual([3, 10])
  })

  it('finds nothing for an empty needle', () => {
    expect(matchOffsets('abc', '')).toEqual([])
  })
})

describe('findTextRanges', () => {
  it('matches case-insensitively inside find regions only', () => {
    const root = container(
      '<p data-find-region>Report.pdf is here</p><button>report button</button><p data-find-region>another REPORT</p>',
    )
    const ranges = findTextRanges(root, 'report')
    expect(ranges.map((r) => r.toString())).toEqual(['Report', 'REPORT'])
  })

  it('matches across the text nodes of one region, as rendered Markdown splits them', () => {
    const root = container('<div data-find-region><strong>bold</strong> text and <code>code</code></div>')
    const ranges = findTextRanges(root, 'bold text')
    expect(ranges).toHaveLength(1)
    expect(ranges[0].toString()).toBe('bold text')
  })

  it('never joins two regions into one hit', () => {
    const root = container('<p data-find-region>end</p><p data-find-region>start</p>')
    expect(findTextRanges(root, 'endstart')).toEqual([])
  })

  it('counts a nested region once', () => {
    const root = container('<div data-find-region>outer <span data-find-region>inner</span></div>')
    expect(findTextRanges(root, 'inner')).toHaveLength(1)
  })

  it('skips aria-hidden text', () => {
    const root = container('<p data-find-region>shown <span aria-hidden="true">shown</span></p>')
    expect(findTextRanges(root, 'shown')).toHaveLength(1)
  })

  it('finds nothing for a blank query', () => {
    const root = container('<p data-find-region>a b c</p>')
    expect(findTextRanges(root, '   ')).toEqual([])
  })

  it('treats a query as text, never markup', () => {
    const root = container('<p data-find-region>&lt;b&gt;tag&lt;/b&gt;</p>')
    expect(findTextRanges(root, '<b>').map((r) => r.toString())).toEqual(['<b>'])
  })
})

describe('firstRangeFrom', () => {
  it('is -1 with no ranges', () => {
    expect(firstRangeFrom([], 0)).toBe(-1)
  })
})

describe('paintFindMatches', () => {
  it('paints the others and the current one under separate highlight names', () => {
    const registry = new Map<string, { ranges: Range[] }>()
    vi.stubGlobal('CSS', { highlights: { set: registry.set.bind(registry), delete: registry.delete.bind(registry) } })
    vi.stubGlobal(
      'Highlight',
      class {
        ranges: Range[]
        constructor(...ranges: Range[]) {
          this.ranges = ranges
        }
      },
    )
    const root = container('<p data-find-region>one one one</p>')
    const ranges = findTextRanges(root, 'one')
    paintFindMatches(root, ranges, 1)
    expect(registry.get('cmdr-find-match')?.ranges).toEqual([ranges[0], ranges[2]])
    expect(registry.get('cmdr-find-current')?.ranges).toEqual([ranges[1]])

    clearFindMatches(root)
    expect(registry.size).toBe(0)
  })

  it('marks the current match region when the highlight API is missing', () => {
    vi.stubGlobal('CSS', {})
    const root = container('<p data-find-region id="a">one</p><p data-find-region id="b">one</p>')
    const ranges = findTextRanges(root, 'one')
    paintFindMatches(root, ranges, 1)
    expect(root.querySelector('[data-find-current]')?.id).toBe('b')
    paintFindMatches(root, ranges, 0)
    expect([...root.querySelectorAll('[data-find-current]')].map((el) => el.id)).toEqual(['a'])
    clearFindMatches(root)
    expect(root.querySelector('[data-find-current]')).toBeNull()
  })
})
