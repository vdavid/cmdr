import { describe, it, expect } from 'vitest'

import { findOffsetByGeometry, type CharBox, type MeasureChar } from './viewer-caret-geometry'

/** Geometry of the fake monospace layout the tests measure against. */
const LEFT = 48
const TOP = 100
const CHAR_W = 8
const ROW_H = 18

/** Viewport y at the vertical middle of visual row `row`. */
function rowY(row: number): number {
  return TOP + row * ROW_H + ROW_H / 2
}

/**
 * Viewport x just inside the left edge of column `col`, so a hit on that column rounds
 * back to its start rather than sitting on the tie-breaking midpoint.
 */
function colStartX(col: number): number {
  return LEFT + col * CHAR_W + 1
}

interface FakeLayout {
  length: number
  measure: MeasureChar
  /** Every offset `measure` was asked about, in call order. */
  probes: number[]
}

/**
 * Lays `text` out on a monospace grid that wraps after `cols` cells, one cell per UTF-16
 * code unit (so an astral codepoint spans two cells, exactly as it spans two units).
 */
function monospaceLayout(text: string, cols = Number.POSITIVE_INFINITY): FakeLayout {
  const boxes: CharBox[] = []
  let row = 0
  let col = 0
  let offset = 0
  for (const ch of text) {
    const units = ch.length
    if (col > 0 && col + units > cols) {
      row += 1
      col = 0
    }
    boxes.push({
      start: offset,
      end: offset + units,
      rect: {
        left: LEFT + col * CHAR_W,
        right: LEFT + (col + units) * CHAR_W,
        top: TOP + row * ROW_H,
        bottom: TOP + (row + 1) * ROW_H,
      },
    })
    col += units
    offset += units
  }

  const probes: number[] = []
  return {
    length: text.length,
    probes,
    measure: (at) => {
      probes.push(at)
      return boxes.find((b) => at >= b.start && at < b.end) ?? null
    },
  }
}

describe('findOffsetByGeometry', () => {
  it('resolves an empty line to offset 0 without measuring anything', () => {
    const layout = monospaceLayout('')
    expect(findOffsetByGeometry({ ...layout, x: colStartX(4), y: rowY(0) })).toBe(0)
    expect(layout.probes).toEqual([])
  })

  it('rounds to the nearer edge of the character under the point', () => {
    const layout = monospaceLayout('hello')
    // Left third of "e" (offset 1) rounds back to 1, right third forward to 2.
    expect(findOffsetByGeometry({ ...layout, x: LEFT + CHAR_W + 1, y: rowY(0) })).toBe(1)
    expect(findOffsetByGeometry({ ...layout, x: LEFT + 2 * CHAR_W - 1, y: rowY(0) })).toBe(2)
  })

  it('clamps a point left of the row text to the row start', () => {
    const layout = monospaceLayout('hello')
    // The line-number gutter and the row's left padding both land here.
    expect(findOffsetByGeometry({ ...layout, x: 4, y: rowY(0) })).toBe(0)
  })

  it('clamps a point right of the row text to the end of the line', () => {
    const layout = monospaceLayout('hello')
    expect(findOffsetByGeometry({ ...layout, x: 4000, y: rowY(0) })).toBe(5)
  })

  it('clamps to the end of the visual row on a wrapped line, not the end of the logical line', () => {
    // "abcd" / "efgh" / "ij" at four columns per row.
    const layout = monospaceLayout('abcdefghij', 4)
    expect(findOffsetByGeometry({ ...layout, x: 4000, y: rowY(1) })).toBe(8)
    expect(findOffsetByGeometry({ ...layout, x: 4000, y: rowY(2) })).toBe(10)
  })

  it('clamps to the start of the visual row on a wrapped line', () => {
    const layout = monospaceLayout('abcdefghij', 4)
    expect(findOffsetByGeometry({ ...layout, x: 4, y: rowY(1) })).toBe(4)
    expect(findOffsetByGeometry({ ...layout, x: 4, y: rowY(2) })).toBe(8)
  })

  it('orders by row before x, so a small x on a later row beats a large x on an earlier one', () => {
    const layout = monospaceLayout('abcdefghij', 4)
    expect(findOffsetByGeometry({ ...layout, x: colStartX(1), y: rowY(2) })).toBe(9)
    expect(findOffsetByGeometry({ ...layout, x: colStartX(3), y: rowY(0) })).toBe(3)
  })

  it('resolves a point above the first row to 0 and below the last row to the line end', () => {
    const layout = monospaceLayout('abcdefghij', 4)
    expect(findOffsetByGeometry({ ...layout, x: colStartX(2), y: TOP - 50 })).toBe(0)
    expect(findOffsetByGeometry({ ...layout, x: colStartX(2), y: TOP + 500 })).toBe(10)
  })

  it('never splits a surrogate pair', () => {
    // "a👋b": the emoji occupies offsets 1..3.
    const layout = monospaceLayout('a👋b')
    expect(findOffsetByGeometry({ ...layout, x: LEFT + CHAR_W + 1, y: rowY(0) })).toBe(1)
    expect(findOffsetByGeometry({ ...layout, x: LEFT + 3 * CHAR_W - 1, y: rowY(0) })).toBe(3)
    expect(findOffsetByGeometry({ ...layout, x: colStartX(3), y: rowY(0) })).toBe(3)
  })

  it('probes a logarithmic number of characters on a very long line', () => {
    const layout = monospaceLayout('x'.repeat(100_000))
    expect(findOffsetByGeometry({ ...layout, x: colStartX(63_210), y: rowY(0) })).toBe(63_210)
    // log2(100000) ≈ 17; anything near the line length means we went linear.
    expect(layout.probes.length).toBeLessThan(25)
  })

  it('falls back to the last proven-before boundary when a measurement is unavailable', () => {
    const layout = monospaceLayout('hello')
    // First probe fails: nothing is proven, so the caret stays at the line start.
    const blind: MeasureChar = () => null
    expect(findOffsetByGeometry({ length: 5, measure: blind, x: colStartX(4), y: rowY(0) })).toBe(0)
    // Later probe fails: keep what the earlier probes proved (offsets 0-3 are behind us).
    const partial: MeasureChar = (at) => (at === 4 ? null : layout.measure(at))
    expect(findOffsetByGeometry({ length: 5, measure: partial, x: colStartX(4), y: rowY(0) })).toBe(4)
  })
})
