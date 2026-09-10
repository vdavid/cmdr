import { describe, it, expect, beforeEach } from 'vitest'
import { shouldDropCrossSourceDuplicate, _resetDedupForTests } from './dispatch-dedup'

describe('cross-source dispatch dedup', () => {
  beforeEach(() => {
    _resetDedupForTests()
  })

  it('drops a menu fire that follows a keyboard fire of the same command inside the window', () => {
    expect(shouldDropCrossSourceDuplicate('file.quickLook', 'keyboard', 1000)).toBe(false)
    expect(shouldDropCrossSourceDuplicate('file.quickLook', 'menu', 1050)).toBe(true)
  })

  it('drops a keyboard fire that follows a menu fire (order-independent)', () => {
    expect(shouldDropCrossSourceDuplicate('file.quickLook', 'menu', 1000)).toBe(false)
    expect(shouldDropCrossSourceDuplicate('file.quickLook', 'keyboard', 1050)).toBe(true)
  })

  it('never drops same-source repeats (double-press, key auto-repeat)', () => {
    expect(shouldDropCrossSourceDuplicate('pane.switch', 'keyboard', 1000)).toBe(false)
    expect(shouldDropCrossSourceDuplicate('pane.switch', 'keyboard', 1010)).toBe(false)
    expect(shouldDropCrossSourceDuplicate('pane.switch', 'keyboard', 1020)).toBe(false)
  })

  it('never drops different commands', () => {
    expect(shouldDropCrossSourceDuplicate('file.copy', 'keyboard', 1000)).toBe(false)
    expect(shouldDropCrossSourceDuplicate('file.move', 'menu', 1010)).toBe(false)
  })

  it('lets a cross-source pair through once the window has passed', () => {
    expect(shouldDropCrossSourceDuplicate('file.quickLook', 'keyboard', 1000)).toBe(false)
    expect(shouldDropCrossSourceDuplicate('file.quickLook', 'menu', 1500)).toBe(false)
  })

  it('never drops a source that has no twin to double-fire, and never lets one break a pairing', () => {
    expect(shouldDropCrossSourceDuplicate('file.quickLook', 'keyboard', 1000)).toBe(false)
    // The palette, MCP, the mouse, and the explorer's own controls fire once per gesture.
    expect(shouldDropCrossSourceDuplicate('file.quickLook', 'palette', 1010)).toBe(false)
    expect(shouldDropCrossSourceDuplicate('file.quickLook', 'mcp', 1020)).toBe(false)
    expect(shouldDropCrossSourceDuplicate('file.quickLook', 'explorer', 1030)).toBe(false)
    expect(shouldDropCrossSourceDuplicate('file.quickLook', 'menu', 1050)).toBe(true)
  })

  it('a dropped fire does not extend the window', () => {
    expect(shouldDropCrossSourceDuplicate('file.quickLook', 'keyboard', 1000)).toBe(false)
    expect(shouldDropCrossSourceDuplicate('file.quickLook', 'menu', 1100)).toBe(true)
    // 100ms after the ORIGINAL fire's window expired: a genuine new menu fire passes.
    expect(shouldDropCrossSourceDuplicate('file.quickLook', 'menu', 1400)).toBe(false)
  })
})
