import { afterEach, describe, expect, it, vi } from 'vitest'
import { focusQuietly, QUIET_FOCUS_ATTRIBUTE } from './quiet-focus'

function region(): HTMLElement {
  const el = document.createElement('div')
  el.tabIndex = 0
  document.body.appendChild(el)
  return el
}

afterEach(() => {
  document.body.innerHTML = ''
})

describe('focusQuietly', () => {
  it('focuses the element, marked quiet and asking the browser for no ring', () => {
    const el = region()
    const focus = vi.spyOn(el, 'focus')
    focusQuietly(el, { preventScroll: true })
    expect(document.activeElement).toBe(el)
    expect(el.hasAttribute(QUIET_FOCUS_ATTRIBUTE)).toBe(true)
    expect(focus).toHaveBeenCalledWith({ preventScroll: true, focusVisible: false })
  })

  it('drops the mark once focus leaves, so a later Tab focus rings', () => {
    const el = region()
    focusQuietly(el)
    el.blur()
    expect(el.hasAttribute(QUIET_FOCUS_ATTRIBUTE)).toBe(false)
    el.focus()
    expect(el.hasAttribute(QUIET_FOCUS_ATTRIBUTE)).toBe(false)
  })

  it('leaves no mark behind when the element could not take focus', () => {
    const el = document.createElement('div')
    el.tabIndex = 0
    // Detached: focus() is a no-op, so no blur would ever clear a mark.
    focusQuietly(el)
    expect(el.hasAttribute(QUIET_FOCUS_ATTRIBUTE)).toBe(false)
  })
})
