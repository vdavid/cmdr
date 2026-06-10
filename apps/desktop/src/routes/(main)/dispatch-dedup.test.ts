import { describe, it, expect, beforeEach } from 'vitest'
import { markDispatchSource, shouldDropCrossSourceDuplicate, _resetDedupForTests } from './dispatch-dedup'

describe('cross-source dispatch dedup', () => {
  beforeEach(() => {
    _resetDedupForTests()
  })

  it('drops a menu fire that follows a keyboard fire of the same command inside the window', () => {
    markDispatchSource('keyboard')
    expect(shouldDropCrossSourceDuplicate('file.quickLook', 1000)).toBe(false)
    markDispatchSource('menu')
    expect(shouldDropCrossSourceDuplicate('file.quickLook', 1050)).toBe(true)
  })

  it('drops a keyboard fire that follows a menu fire (order-independent)', () => {
    markDispatchSource('menu')
    expect(shouldDropCrossSourceDuplicate('file.quickLook', 1000)).toBe(false)
    markDispatchSource('keyboard')
    expect(shouldDropCrossSourceDuplicate('file.quickLook', 1050)).toBe(true)
  })

  it('never drops same-source repeats (double-press, key auto-repeat)', () => {
    markDispatchSource('keyboard')
    expect(shouldDropCrossSourceDuplicate('pane.switch', 1000)).toBe(false)
    markDispatchSource('keyboard')
    expect(shouldDropCrossSourceDuplicate('pane.switch', 1010)).toBe(false)
    markDispatchSource('keyboard')
    expect(shouldDropCrossSourceDuplicate('pane.switch', 1020)).toBe(false)
  })

  it('never drops different commands', () => {
    markDispatchSource('keyboard')
    expect(shouldDropCrossSourceDuplicate('file.copy', 1000)).toBe(false)
    markDispatchSource('menu')
    expect(shouldDropCrossSourceDuplicate('file.move', 1010)).toBe(false)
  })

  it('lets a cross-source pair through once the window has passed', () => {
    markDispatchSource('keyboard')
    expect(shouldDropCrossSourceDuplicate('file.quickLook', 1000)).toBe(false)
    markDispatchSource('menu')
    expect(shouldDropCrossSourceDuplicate('file.quickLook', 1500)).toBe(false)
  })

  it('untagged dispatches (palette, MCP) never participate and never break a pairing', () => {
    markDispatchSource('keyboard')
    expect(shouldDropCrossSourceDuplicate('file.quickLook', 1000)).toBe(false)
    // An untagged dispatch in between (palette click) passes and is invisible to the guard.
    expect(shouldDropCrossSourceDuplicate('app.about', 1010)).toBe(false)
    markDispatchSource('menu')
    expect(shouldDropCrossSourceDuplicate('file.quickLook', 1050)).toBe(true)
  })

  it('a dropped fire does not extend the window', () => {
    markDispatchSource('keyboard')
    expect(shouldDropCrossSourceDuplicate('file.quickLook', 1000)).toBe(false)
    markDispatchSource('menu')
    expect(shouldDropCrossSourceDuplicate('file.quickLook', 1100)).toBe(true)
    // 250ms after the ORIGINAL fire's window expired: a genuine new menu fire passes.
    markDispatchSource('menu')
    expect(shouldDropCrossSourceDuplicate('file.quickLook', 1400)).toBe(false)
  })

  it('the source tag is consumed by the next dispatch only', () => {
    markDispatchSource('keyboard')
    expect(shouldDropCrossSourceDuplicate('file.copy', 1000)).toBe(false)
    // The tag was consumed; this untagged dispatch is exempt even though it
    // matches the previous id cross-source-style.
    expect(shouldDropCrossSourceDuplicate('file.copy', 1010)).toBe(false)
  })
})
