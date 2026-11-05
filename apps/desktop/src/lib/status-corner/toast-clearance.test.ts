import { afterEach, describe, expect, it, vi } from 'vitest'
import { publishCornerBottom, STATUS_CORNER_BOTTOM_VAR } from './toast-clearance'

const published = () => document.documentElement.style.getPropertyValue(STATUS_CORNER_BOTTOM_VAR)

describe('publishCornerBottom', () => {
  afterEach(() => {
    vi.unstubAllGlobals()
  })

  // Regression: at 125% zoom the "Still running in the background" toast covered the copy's progress chip.
  it('publishes the corner’s bottom edge, follows it as the corner resizes, and clears it on stop', () => {
    let resized: (() => void) | undefined
    vi.stubGlobal(
      'ResizeObserver',
      class {
        constructor(callback: () => void) {
          resized = callback
        }
        observe() {}
        disconnect() {}
      },
    )
    const corner = document.createElement('div')
    let bottom = 20
    corner.getBoundingClientRect = () => ({ bottom }) as DOMRect

    const stop = publishCornerBottom(corner)
    expect(published()).toBe('20px')
    bottom = 33.5
    resized?.()
    expect(published()).toBe('33.5px')
    stop()
    expect(published()).toBe('')
  })
})
