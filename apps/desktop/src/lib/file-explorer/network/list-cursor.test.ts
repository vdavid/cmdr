import { describe, it, expect } from 'vitest'
import { cursorAfterArrow, nameStartsWith } from './list-cursor'

describe('cursorAfterArrow', () => {
  it('steps one row, clamped to the list', () => {
    expect(cursorAfterArrow('ArrowDown', 2, 5)).toBe(3)
    expect(cursorAfterArrow('ArrowDown', 4, 5)).toBe(4)
    expect(cursorAfterArrow('ArrowUp', 0, 5)).toBe(0)
  })

  it('jumps to the first and the last row on left and right', () => {
    expect(cursorAfterArrow('ArrowLeft', 3, 5)).toBe(0)
    expect(cursorAfterArrow('ArrowRight', 1, 5)).toBe(4)
  })

  it('leaves every other key alone', () => {
    expect(cursorAfterArrow('Enter', 1, 5)).toBeNull()
  })
})

describe('nameStartsWith', () => {
  it('ignores case', () => {
    expect(nameStartsWith('Testnas', 'test')).toBe(true)
    expect(nameStartsWith('testnas', 'TEST')).toBe(true)
  })

  it('wants the start of the name, not any part of it', () => {
    expect(nameStartsWith('Testnas', 'nas')).toBe(false)
  })

  it('reads a decomposed name the way a keyboard types it', () => {
    expect(nameStartsWith('Re\u0301sume\u0301', 'R\u00e9s')).toBe(true)
  })
})
