import { describe, it, expect } from 'vitest'
import { nextSwatchIndex } from './swatch-keyboard'

describe('nextSwatchIndex', () => {
  // 13 swatches arranged in a 4-column grid:
  //   row 0:  0  1  2  3
  //   row 1:  4  5  6  7
  //   row 2:  8  9 10 11
  //   row 3: 12
  const total = 13
  const cols = 4

  it('ArrowRight advances within the row', () => {
    expect(nextSwatchIndex('ArrowRight', 0, total, cols)).toBe(1)
    expect(nextSwatchIndex('ArrowRight', 2, total, cols)).toBe(3)
  })

  it('ArrowRight clamps at the last item', () => {
    expect(nextSwatchIndex('ArrowRight', total - 1, total, cols)).toBe(total - 1)
  })

  it('ArrowLeft retreats within the row', () => {
    expect(nextSwatchIndex('ArrowLeft', 3, total, cols)).toBe(2)
  })

  it('ArrowLeft clamps at zero', () => {
    expect(nextSwatchIndex('ArrowLeft', 0, total, cols)).toBe(0)
  })

  it('ArrowDown jumps down a row', () => {
    expect(nextSwatchIndex('ArrowDown', 0, total, cols)).toBe(4)
    expect(nextSwatchIndex('ArrowDown', 5, total, cols)).toBe(9)
  })

  it('ArrowDown clamps at the last item when no row below exists', () => {
    expect(nextSwatchIndex('ArrowDown', 11, total, cols)).toBe(total - 1)
  })

  it('ArrowUp jumps up a row', () => {
    expect(nextSwatchIndex('ArrowUp', 9, total, cols)).toBe(5)
  })

  it('ArrowUp clamps at zero from the top row', () => {
    expect(nextSwatchIndex('ArrowUp', 2, total, cols)).toBe(0)
  })

  it('Home goes to the first item', () => {
    expect(nextSwatchIndex('Home', 7, total, cols)).toBe(0)
  })

  it('End goes to the last item', () => {
    expect(nextSwatchIndex('End', 3, total, cols)).toBe(total - 1)
  })

  it('returns null for other keys', () => {
    expect(nextSwatchIndex('Enter', 0, total, cols)).toBeNull()
    expect(nextSwatchIndex('Escape', 0, total, cols)).toBeNull()
    expect(nextSwatchIndex('a', 0, total, cols)).toBeNull()
  })

  it('returns null when the grid is empty', () => {
    expect(nextSwatchIndex('ArrowRight', -1, 0, cols)).toBeNull()
  })
})
