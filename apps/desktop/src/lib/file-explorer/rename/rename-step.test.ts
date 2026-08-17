import { describe, it, expect } from 'vitest'
import { resolveStepIndex, renameStepDirection } from './rename-step'

describe('the row a chained rename step lands on', () => {
  it('steps down to the next row', () => {
    expect(resolveStepIndex('down', { cursorIndex: 3, rowCount: 10, hasParent: true })).toBe(4)
  })

  it('steps up to the previous row', () => {
    expect(resolveStepIndex('up', { cursorIndex: 3, rowCount: 10, hasParent: true })).toBe(2)
  })

  it('stops at the last row instead of wrapping around', () => {
    expect(resolveStepIndex('down', { cursorIndex: 9, rowCount: 10, hasParent: true })).toBeUndefined()
  })

  it('stops above the first row instead of wrapping around', () => {
    expect(resolveStepIndex('up', { cursorIndex: 0, rowCount: 10, hasParent: false })).toBeUndefined()
  })

  it('refuses to step onto the parent row, which is nothing to rename', () => {
    expect(resolveStepIndex('up', { cursorIndex: 1, rowCount: 10, hasParent: true })).toBeUndefined()
  })

  it('steps up freely once past the parent row', () => {
    expect(resolveStepIndex('up', { cursorIndex: 2, rowCount: 10, hasParent: true })).toBe(1)
  })

  it('has nowhere to go in a listing whose only row is the one being renamed', () => {
    expect(resolveStepIndex('down', { cursorIndex: 1, rowCount: 2, hasParent: true })).toBeUndefined()
    expect(resolveStepIndex('up', { cursorIndex: 1, rowCount: 2, hasParent: true })).toBeUndefined()
  })
})

describe('which keypresses chain a rename', () => {
  const press = (init: KeyboardEventInit) => new KeyboardEvent('keydown', init)

  it('reads a bare down arrow as a step down', () => {
    expect(renameStepDirection(press({ key: 'ArrowDown' }))).toBe('down')
  })

  it('reads a bare up arrow as a step up', () => {
    expect(renameStepDirection(press({ key: 'ArrowUp' }))).toBe('up')
  })

  it('leaves a modified arrow alone, so ⌘↓ and ⌥↑ keep their own meanings', () => {
    expect(renameStepDirection(press({ key: 'ArrowDown', metaKey: true }))).toBeUndefined()
    expect(renameStepDirection(press({ key: 'ArrowUp', altKey: true }))).toBeUndefined()
    expect(renameStepDirection(press({ key: 'ArrowDown', shiftKey: true }))).toBeUndefined()
    expect(renameStepDirection(press({ key: 'ArrowUp', ctrlKey: true }))).toBeUndefined()
  })

  it('leaves the paged and edge keys alone: only the arrows chain', () => {
    expect(renameStepDirection(press({ key: 'PageDown' }))).toBeUndefined()
    expect(renameStepDirection(press({ key: 'PageUp' }))).toBeUndefined()
    expect(renameStepDirection(press({ key: 'Home' }))).toBeUndefined()
    expect(renameStepDirection(press({ key: 'End' }))).toBeUndefined()
  })
})
