import { describe, expect, it } from 'vitest'
import { createViewerHistory, MAX_HISTORY_ENTRIES } from './viewer-history'

describe('createViewerHistory', () => {
  it('starts with the window’s own document and nowhere to go', () => {
    const history = createViewerHistory('/docs/README.md')
    expect(history.current).toEqual({ path: '/docs/README.md', scrollTop: 0 })
    expect(history.canGoBack).toBe(false)
    expect(history.canGoForward).toBe(false)
    expect(history.target('back')).toBeNull()
    expect(history.target('forward')).toBeNull()
  })

  it('goes back to the document a link left, with its scroll position', () => {
    const history = createViewerHistory('/docs/README.md')
    history.rememberScroll(420)
    history.visit('/docs/setup.md')

    expect(history.canGoBack).toBe(true)
    expect(history.target('back')).toEqual({ path: '/docs/README.md', scrollTop: 420 })
  })

  it('moves only when stepped, so an open that doesn’t happen leaves it in place', () => {
    const history = createViewerHistory('/a.md')
    history.visit('/b.md')
    history.target('back')
    expect(history.current.path).toBe('/b.md')

    history.step('back')
    expect(history.current.path).toBe('/a.md')
    expect(history.canGoForward).toBe(true)
    expect(history.target('forward')?.path).toBe('/b.md')
  })

  it('remembers where each document was scrolled when it was left', () => {
    const history = createViewerHistory('/a.md')
    history.visit('/b.md')
    history.rememberScroll(90)
    history.step('back')
    expect(history.target('forward')).toEqual({ path: '/b.md', scrollTop: 90 })
  })

  it('a new link drops the forward entries, like a browser', () => {
    const history = createViewerHistory('/a.md')
    history.visit('/b.md')
    history.visit('/c.md')
    history.step('back')
    history.step('back')
    history.visit('/d.md')

    expect(history.canGoForward).toBe(false)
    expect(history.target('back')?.path).toBe('/a.md')
  })

  it('a step with nothing that way is a no-op', () => {
    const history = createViewerHistory('/a.md')
    history.step('back')
    history.step('forward')
    expect(history.current.path).toBe('/a.md')
  })

  it('keeps at most the newest entries', () => {
    const history = createViewerHistory('/0.md')
    for (let i = 1; i <= MAX_HISTORY_ENTRIES + 5; i++) history.visit(`/${String(i)}.md`)
    let steps = 0
    while (history.canGoBack) {
      history.step('back')
      steps++
    }
    expect(steps).toBe(MAX_HISTORY_ENTRIES - 1)
    expect(history.current.path).toBe('/6.md')
  })
})
