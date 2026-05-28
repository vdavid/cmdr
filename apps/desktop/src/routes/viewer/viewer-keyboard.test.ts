/**
 * Pure-helper tests for the viewer's keyboard plumbing. Each helper returns
 * `true` when it consumed the event, `false` when the caller should fall
 * through to another handler.
 */

import { describe, expect, it, vi } from 'vitest'

import { handleSearchToggleKey, handleTailToggleKey, handleToggleKey } from './viewer-keyboard'

function makeKey(props: Partial<KeyboardEventInit & { key: string }>): KeyboardEvent {
  return new KeyboardEvent('keydown', { key: 'a', ...props })
}

describe('handleTailToggleKey', () => {
  it('toggles on unmodified `F`', () => {
    const toggle = vi.fn()
    const handled = handleTailToggleKey(makeKey({ key: 'F' }), toggle)
    expect(handled).toBe(true)
    expect(toggle).toHaveBeenCalledOnce()
  })

  it('toggles on unmodified lower-case `f`', () => {
    const toggle = vi.fn()
    const handled = handleTailToggleKey(makeKey({ key: 'f' }), toggle)
    expect(handled).toBe(true)
    expect(toggle).toHaveBeenCalledOnce()
  })

  it('does NOT trigger when meta/ctrl/alt/shift is held', () => {
    const toggle = vi.fn()
    for (const mod of ['metaKey', 'ctrlKey', 'altKey', 'shiftKey'] as const) {
      const handled = handleTailToggleKey(makeKey({ key: 'f', [mod]: true }), toggle)
      expect(handled).toBe(false)
    }
    expect(toggle).not.toHaveBeenCalled()
  })

  it('ignores other keys', () => {
    const toggle = vi.fn()
    expect(handleTailToggleKey(makeKey({ key: 't' }), toggle)).toBe(false)
    expect(toggle).not.toHaveBeenCalled()
  })
})

describe('handleToggleKey (word wrap on `W`)', () => {
  it('toggles on unmodified `w`', () => {
    const toggle = vi.fn()
    expect(handleToggleKey(makeKey({ key: 'w' }), toggle)).toBe(true)
    expect(toggle).toHaveBeenCalledOnce()
  })

  it('does NOT trigger when meta is held', () => {
    const toggle = vi.fn()
    expect(handleToggleKey(makeKey({ key: 'w', metaKey: true }), toggle)).toBe(false)
    expect(toggle).not.toHaveBeenCalled()
  })
})

describe('handleSearchToggleKey', () => {
  it('toggles regex on ⌘⌥R', () => {
    const toggleUseRegex = vi.fn()
    const toggleCaseSensitive = vi.fn()
    const handled = handleSearchToggleKey(makeKey({ key: 'r', metaKey: true, altKey: true }), {
      toggleUseRegex,
      toggleCaseSensitive,
    })
    expect(handled).toBe(true)
    expect(toggleUseRegex).toHaveBeenCalledOnce()
    expect(toggleCaseSensitive).not.toHaveBeenCalled()
  })

  it('toggles case-sensitive on ⌘⌥C', () => {
    const toggleUseRegex = vi.fn()
    const toggleCaseSensitive = vi.fn()
    const handled = handleSearchToggleKey(makeKey({ key: 'c', metaKey: true, altKey: true }), {
      toggleUseRegex,
      toggleCaseSensitive,
    })
    expect(handled).toBe(true)
    expect(toggleCaseSensitive).toHaveBeenCalledOnce()
  })

  it('does NOT fire without alt', () => {
    const toggleUseRegex = vi.fn()
    const toggleCaseSensitive = vi.fn()
    const handled = handleSearchToggleKey(makeKey({ key: 'r', metaKey: true }), {
      toggleUseRegex,
      toggleCaseSensitive,
    })
    expect(handled).toBe(false)
  })
})
