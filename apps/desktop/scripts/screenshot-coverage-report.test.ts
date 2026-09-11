import { describe, it, expect } from 'vitest'
import {
  buildCoverageReport,
  renderCoverageReport,
  buildSurfaceReview,
  renderSurfaceReview,
} from './screenshot-coverage-report.ts'

describe('buildCoverageReport', () => {
  it('tallies direct vs representative vs uncoupled per area', () => {
    const directKeys = new Set(['common.ok', 'errors.listing.notFound.title'])
    const representativeKeys = new Set(['errors.listing.denied.title'])
    const keysByArea = new Map([
      ['common', ['common.ok', 'common.cancel']], // 1 direct, 1 uncoupled
      ['errors', ['errors.listing.notFound.title', 'errors.listing.denied.title']], // 1 direct, 1 representative
      ['about', ['about.version']], // 1 uncoupled
    ])
    const r = buildCoverageReport(directKeys, representativeKeys, keysByArea)

    expect(r.total).toBe(5)
    expect(r.direct).toBe(2)
    expect(r.representative).toBe(1)
    expect(r.uncoupled).toBe(2)
    expect(r.nativeOnly).toBe(0)

    // Areas are sorted by name.
    expect(r.areas.map((a) => a.area)).toEqual(['about', 'common', 'errors'])
    const errors = r.areas.find((a) => a.area === 'errors')
    expect(errors).toEqual({ area: 'errors', total: 2, direct: 1, representative: 1, uncoupled: 0, nativeOnly: 0 })
  })

  it('counts a native-surface key apart from an uncoupled one', () => {
    // A `menu.*` key is drawn by the OS, so no webview capture can ever show it.
    // Filing it under "uncoupled" would read as a surface the driver forgot,
    // and someone would eventually go looking for it.
    const r = buildCoverageReport(
      new Set(),
      new Set(),
      new Map([
        ['menu', ['menu.bar.file', 'menu.file.open']],
        ['common', ['common.ok']],
      ]),
    )
    expect(r.nativeOnly).toBe(2)
    expect(r.uncoupled).toBe(1)
    expect(r.areas.find((a) => a.area === 'menu')).toEqual({
      area: 'menu',
      total: 2,
      direct: 0,
      representative: 0,
      uncoupled: 0,
      nativeOnly: 2,
    })
  })

  it('lets a real capture beat the native exemption', () => {
    // The exemption is a fallback, not an override: if a native key somehow DID
    // render on a captured surface, the real screenshot is the better answer.
    const r = buildCoverageReport(new Set(['menu.bar.file']), new Set(), new Map([['menu', ['menu.bar.file']]]))
    expect(r.direct).toBe(1)
    expect(r.nativeOnly).toBe(0)
  })
})

describe('renderCoverageReport', () => {
  it('renders a markdown table distinguishing direct from representative', () => {
    const report = buildCoverageReport(
      new Set(['common.ok']),
      new Set(['common.cancel']),
      new Map([['common', ['common.ok', 'common.cancel', 'common.extra']]]),
    )
    const md = renderCoverageReport(report)
    expect(md).toContain('# Screenshot coverage')
    // 1 direct + 1 representative of 3 = 67%.
    expect(md).toContain('2 / 3 keys have a screenshot (67%)')
    expect(md).toContain('1 direct')
    expect(md).toContain('1 representative')
    expect(md).toContain('| common | 1 | 1 | 1 | 0 | 3 | 67% |')
    expect(md.toLowerCase()).toContain('partial')
    // It explains what a representative coupling is (honesty).
    expect(md.toLowerCase()).toContain('representative')
  })
})

describe('buildSurfaceReview', () => {
  it('flags a surface whose every key another surface also has', () => {
    const review = buildSurfaceReview({
      broad: { screenshot: 'broad.png', keys: ['a.one', 'a.two'] },
      narrow: { screenshot: 'narrow.png', keys: ['a.one', 'a.two', 'a.three'] },
      alone: { screenshot: 'alone.png', keys: ['b.only'] },
    })
    expect(review.surfaces).toBe(3)
    expect(review.redundant.map((r) => r.surface)).toEqual(['broad'])
    expect(review.redundant[0]).toEqual({ surface: 'broad', screenshot: 'broad.png', keys: 2 })
  })

  it('does not flag a surface that owns even one key alone', () => {
    const review = buildSurfaceReview({
      a: { screenshot: 'a.png', keys: ['x.shared', 'x.mine'] },
      b: { screenshot: 'b.png', keys: ['x.shared'] },
    })
    expect(review.redundant.map((r) => r.surface)).toEqual(['b'])
  })

  it('leaves a keyless surface out of the redundant list rather than calling it redundant', () => {
    // A surface that recorded nothing is a staging problem to investigate, not a
    // duplicate to prune, and saying "it adds no unique key" would be misleading.
    const review = buildSurfaceReview({ empty: { screenshot: 'empty.png', keys: [] } })
    expect(review.redundant).toEqual([])
  })

  it('reports the surfaces captured at a reduced UI zoom', () => {
    const review = buildSurfaceReview({
      tall: { screenshot: 'tall.png', keys: ['a.one'], uiZoom: 80 },
      normal: { screenshot: 'normal.png', keys: ['a.two'], uiZoom: 100 },
      unset: { screenshot: 'unset.png', keys: ['a.three'] },
    })
    expect(review.reducedZoom).toEqual([{ surface: 'tall', screenshot: 'tall.png', uiZoom: 80 }])
  })
})

describe('renderSurfaceReview', () => {
  it('reads as a suggestion, not a delete order', () => {
    const md = renderSurfaceReview(
      buildSurfaceReview({
        broad: { screenshot: 'broad.png', keys: ['a.one'] },
        narrow: { screenshot: 'narrow.png', keys: ['a.one', 'a.two'] },
      }),
    )
    expect(md).toContain('`broad` (1 key, none unique)')
    expect(md).toContain('NOT an automatic delete')
  })

  it('says plainly when nothing needs attention', () => {
    const md = renderSurfaceReview(buildSurfaceReview({ only: { screenshot: 'only.png', keys: ['a.one'] } }))
    expect(md).toContain('Every captured surface is the only source of at least one key.')
    expect(md).toContain('Every surface fit the display at 100% zoom')
  })

  it('warns that a reduced-zoom image shows smaller text than a user sees', () => {
    const md = renderSurfaceReview(
      buildSurfaceReview({ tall: { screenshot: 'tall.png', keys: ['a.one'], uiZoom: 75 } }),
    )
    expect(md).toContain('`tall`: captured at 75% zoom')
    expect(md).toContain('smaller')
  })
})
