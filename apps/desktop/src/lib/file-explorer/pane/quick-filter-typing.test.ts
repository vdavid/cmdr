import { describe, it, expect } from 'vitest'
import { createQuickFilterTyping } from './quick-filter-typing'

function setup(windowMs = 1500) {
  let t = 0
  let window = windowMs
  const typing = createQuickFilterTyping({ getWindowMs: () => window, now: () => t })
  return {
    typing,
    advance: (ms: number) => (t += ms),
    setWindow: (ms: number) => (window = ms),
  }
}

describe('the quick filter typing burst', () => {
  it('is not live before anything is typed', () => {
    expect(setup().typing.isLive()).toBe(false)
  })

  it('stays live inside the window after each edit, and ends past it', () => {
    const { typing, advance } = setup()
    typing.touch()
    advance(1499)
    expect(typing.isLive()).toBe(true)
    typing.touch()
    advance(1499)
    expect(typing.isLive()).toBe(true)
    advance(1)
    expect(typing.isLive()).toBe(false)
  })

  it('ends at once when told, without waiting for the window', () => {
    const { typing } = setup()
    typing.touch()
    typing.end()
    expect(typing.isLive()).toBe(false)
  })

  it('reads the window live, so a Settings change counts on the next key', () => {
    const { typing, advance, setWindow } = setup()
    typing.touch()
    advance(800)
    setWindow(500)
    expect(typing.isLive()).toBe(false)
  })
})
