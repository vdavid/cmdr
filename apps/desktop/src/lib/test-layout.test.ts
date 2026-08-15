/**
 * The viewport harness itself. If these slip, every spec built on it starts
 * measuring something other than what it says it measures.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import { installLayoutMock } from './test-layout'

// Selectors are matched against the whole document, so each test starts clean or
// it would also be measuring the previous one's elements.
beforeEach(() => {
  document.body.replaceChildren()
})

function div(className: string): HTMLElement {
  const element = document.createElement('div')
  element.className = className
  document.body.appendChild(element)
  return element
}

describe('installLayoutMock', () => {
  it('gives the named elements a size and leaves every other one alone', () => {
    const surface = div('surface')
    const other = div('other')
    installLayoutMock({ '.surface': { clientHeight: 400, clientWidth: 785, offsetWidth: 800 } })

    expect(surface.clientHeight).toBe(400)
    expect(surface.clientWidth).toBe(785)
    expect(surface.offsetWidth).toBe(800)
    // Unnamed elements keep reading whatever the layout-less DOM says, so a stub
    // can't quietly hand a size to a component the spec never mentioned.
    expect(other.clientHeight).toBe(0)
    expect(other.offsetWidth).toBe(0)
  })

  it('falls through to the environment for metrics a rule omits', () => {
    const surface = div('surface')
    installLayoutMock({ '.surface': { clientHeight: 400 } })

    expect(surface.clientHeight).toBe(400)
    expect(surface.offsetHeight).toBe(0)
  })

  it('applies the first matching rule', () => {
    const surface = div('surface tall')
    installLayoutMock({ '.tall': { clientHeight: 900 }, '.surface': { clientHeight: 400 } })

    expect(surface.clientHeight).toBe(900)
  })

  it('restores the environment getters', () => {
    const describeClientHeight = () => Object.getOwnPropertyDescriptor(HTMLElement.prototype, 'clientHeight')
    const original = describeClientHeight()
    const surface = div('surface')

    const layout = installLayoutMock({ '.surface': { clientHeight: 400 } })
    expect(surface.clientHeight).toBe(400)
    expect(describeClientHeight()).not.toEqual(original)

    layout.restore()

    // The environment's own getter is back, by identity — not a look-alike that
    // happens to answer 0 while still routing through the harness.
    expect(surface.clientHeight).toBe(0)
    expect(describeClientHeight()).toEqual(original)
  })

  it('resize reports the new size and notifies observers watching a matching element', () => {
    const surface = div('surface')
    const other = div('other')
    const layout = installLayoutMock({ '.surface': { clientHeight: 400, clientWidth: 800 } })

    const seen = vi.fn()
    const observer = new ResizeObserver(seen)
    observer.observe(surface)
    observer.observe(other)

    layout.resize('.surface', { clientHeight: 100 })

    expect(surface.clientHeight).toBe(100)
    // Merged into the existing box, not replacing it.
    expect(surface.clientWidth).toBe(800)
    expect(seen).toHaveBeenCalledTimes(1)
    const entries = seen.mock.calls[0][0] as ResizeObserverEntry[]
    expect(entries.map((entry) => entry.target)).toEqual([surface])
    expect(entries[0].contentRect.height).toBe(100)
  })

  it('does not notify an observer that stopped watching', () => {
    const surface = div('surface')
    const layout = installLayoutMock({ '.surface': { clientHeight: 400 } })

    const seen = vi.fn()
    const observer = new ResizeObserver(seen)
    observer.observe(surface)
    observer.unobserve(surface)

    layout.resize('.surface', { clientHeight: 100 })

    expect(seen).not.toHaveBeenCalled()
  })

  it('scroll moves the element and fires the event the component listens for', () => {
    const surface = div('surface')
    const layout = installLayoutMock({ '.surface': { clientHeight: 400 } })

    // The handler reads `scrollTop` off the event target, so the value has to be
    // in place before the event goes out.
    const seenAt: number[] = []
    surface.addEventListener('scroll', (event) => {
      seenAt.push((event.target as HTMLElement).scrollTop)
    })

    layout.scroll('.surface', 240)

    expect(seenAt).toEqual([240])
  })
})
