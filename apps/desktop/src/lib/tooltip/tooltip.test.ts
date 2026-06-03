import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { tooltip } from './tooltip'

describe('tooltip', () => {
  beforeEach(() => {
    document.body.innerHTML = ''
    vi.useFakeTimers()
  })

  afterEach(() => {
    vi.useRealTimers()
  })

  function makeTrigger(): HTMLElement {
    const el = document.createElement('div')
    el.textContent = 'row'
    document.body.appendChild(el)
    return el
  }

  // Regression: a virtual-scroll row recycled while hovered used to leak its 400ms show-timer (removing
  // a node fires no `mouseleave`), which then fired against the detached node — `getBoundingClientRect`
  // returns all-zero for a detached element, so the tooltip landed in the top-left corner of the window.
  it('does not show when the trigger is destroyed during the show delay', () => {
    const el = makeTrigger()
    const action = tooltip(el, 'Folder info')
    el.dispatchEvent(new MouseEvent('mouseenter'))

    // Svelte tearing down a recycled row: it removes the node and calls the action's destroy().
    el.remove()
    action.destroy?.()

    vi.advanceTimersByTime(500)

    expect(document.querySelector('.cmdr-tooltip.visible')).toBeNull()
  })

  // Defense in depth: even if the timer fires against a detached node (no destroy ran — e.g. the trigger
  // was removed as part of a larger subtree), the tooltip must never become visible in the corner.
  it('does not show when the timer fires against a detached node', () => {
    const el = makeTrigger()
    tooltip(el, 'Folder info')
    el.dispatchEvent(new MouseEvent('mouseenter'))

    el.remove()

    vi.advanceTimersByTime(500)

    expect(document.querySelector('.cmdr-tooltip.visible')).toBeNull()
  })

  // Sanity: a still-connected trigger shows and positions the tooltip normally (not dumped at the corner).
  it('shows and positions the tooltip for a connected trigger', () => {
    const el = makeTrigger()
    // happy-dom does no layout, so feed a real rect for the positioning math.
    vi.spyOn(el, 'getBoundingClientRect').mockReturnValue({
      left: 100,
      top: 100,
      right: 150,
      bottom: 120,
      width: 50,
      height: 20,
      x: 100,
      y: 100,
      toJSON: () => ({}),
    })

    tooltip(el, 'Folder info')
    el.dispatchEvent(new MouseEvent('mouseenter'))
    vi.advanceTimersByTime(500)

    const tip = document.querySelector('.cmdr-tooltip')
    expect(tip).not.toBeNull()
    expect(tip?.classList.contains('visible')).toBe(true)
    expect(tip?.textContent).toBe('Folder info')
    expect((tip as HTMLElement).style.top).not.toBe('0px')
  })
})
