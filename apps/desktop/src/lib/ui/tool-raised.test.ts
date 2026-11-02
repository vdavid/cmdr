import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { TOOL_NAVIGATION_WINDOW_MS, mayBeToolRaised, noteToolNavigation } from './tool-raised'

describe('mayBeToolRaised', () => {
  // Each test starts far past any note an earlier one left.
  let startAt = Date.UTC(2030, 0, 1)

  beforeEach(() => {
    vi.useFakeTimers()
    startAt += 10 * TOOL_NAVIGATION_WINDOW_MS
    vi.setSystemTime(startAt)
  })

  afterEach(() => {
    vi.useRealTimers()
    document.body.innerHTML = ''
  })

  it('says no for a person working in the panes', () => {
    expect(mayBeToolRaised()).toBe(false)
  })

  it('says yes while a tool has just navigated, and stops when the window ends', () => {
    noteToolNavigation()
    expect(mayBeToolRaised()).toBe(true)
    vi.advanceTimersByTime(TOOL_NAVIGATION_WINDOW_MS - 1)
    expect(mayBeToolRaised()).toBe(true)
    vi.advanceTimersByTime(1)
    expect(mayBeToolRaised()).toBe(false)
  })

  it('says yes whenever the rail holds focus, however long a dial took', () => {
    document.body.innerHTML = '<aside class="ask-cmdr-rail"><textarea></textarea></aside><input class="pane" />'
    document.querySelector<HTMLElement>('textarea')?.focus()
    expect(mayBeToolRaised()).toBe(true)
    document.querySelector<HTMLElement>('.pane')?.focus()
    expect(mayBeToolRaised()).toBe(false)
  })
})
