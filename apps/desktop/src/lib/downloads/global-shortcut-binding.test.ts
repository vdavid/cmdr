import { describe, it, expect } from 'vitest'
import { toAccelerator, DEFAULT_GLOBAL_REVEAL_BINDING } from './global-shortcut-binding'

describe('toAccelerator', () => {
  it('translates the default ⌃⌥⌘J to Control+Alt+Super+J', () => {
    expect(toAccelerator(DEFAULT_GLOBAL_REVEAL_BINDING)).toBe('Control+Alt+Super+J')
  })

  it('translates a Cmd+Shift+K combo', () => {
    expect(toAccelerator('⌘⇧K')).toBe('Super+Shift+K')
  })

  it('uppercases the key half', () => {
    expect(toAccelerator('⌘j')).toBe('Super+J')
  })

  it('returns null for empty input', () => {
    expect(toAccelerator('')).toBeNull()
  })

  it('returns null for a binding with no modifiers (global shortcut requires one)', () => {
    expect(toAccelerator('J')).toBeNull()
  })

  it('returns null for a binding with only modifiers, no key', () => {
    expect(toAccelerator('⌘⇧')).toBeNull()
  })

  it('deduplicates accidentally-repeated modifiers', () => {
    // Hand-typed pathological case; the recorder shouldn't emit this but we
    // shouldn't choke on it either.
    expect(toAccelerator('⌘⌘K')).toBe('Super+K')
  })
})
