import { beforeEach, describe, expect, it, vi } from 'vitest'
import { useInlineSize } from './inline-size-action'
import { installLayoutMock } from '$lib/test-layout'

/**
 * The action is the house stand-in for a CSS container query, so what these
 * tests pin is the property that makes it one: it reports the CONTENT box, the
 * same box `@container (max-width: …)` reads, so a threshold ported from a size
 * query keeps its meaning.
 */
describe('useInlineSize', () => {
  beforeEach(() => {
    document.body.innerHTML = ''
  })

  function makeNode(): HTMLElement {
    const el = document.createElement('div')
    el.className = 'measured'
    document.body.appendChild(el)
    return el
  }

  it('reports a size before the observer has fired, so nothing renders a frame unmeasured', () => {
    installLayoutMock({ '.measured': { clientWidth: 140 } })
    const el = makeNode()
    const onResize = vi.fn()

    const action = useInlineSize(el, { onResize })

    expect(onResize).toHaveBeenCalledWith(140)
    action.destroy?.()
  })

  it('reports the content box, not the padding box', () => {
    installLayoutMock({ '.measured': { clientWidth: 140 } })
    const el = makeNode()
    el.style.paddingLeft = '8px'
    el.style.paddingRight = '8px'
    const onResize = vi.fn()

    const action = useInlineSize(el, { onResize })

    // `clientWidth` includes padding; a size query does not.
    expect(onResize).toHaveBeenCalledWith(124)
    action.destroy?.()
  })

  it('reports again when the element resizes', () => {
    const layout = installLayoutMock({ '.measured': { clientWidth: 140 } })
    const el = makeNode()
    const onResize = vi.fn()
    const action = useInlineSize(el, { onResize })
    onResize.mockClear()

    layout.resize('.measured', { clientWidth: 60 })

    expect(onResize).toHaveBeenCalledWith(60)
    action.destroy?.()
  })

  it('stops reporting once destroyed', () => {
    const layout = installLayoutMock({ '.measured': { clientWidth: 140 } })
    const el = makeNode()
    const onResize = vi.fn()
    const action = useInlineSize(el, { onResize })

    action.destroy?.()
    onResize.mockClear()
    layout.resize('.measured', { clientWidth: 60 })

    expect(onResize).not.toHaveBeenCalled()
  })

  it('hands later resizes to the callback it was updated with', () => {
    const layout = installLayoutMock({ '.measured': { clientWidth: 140 } })
    const el = makeNode()
    const first = vi.fn()
    const second = vi.fn()
    const action = useInlineSize(el, { onResize: first })

    action.update?.({ onResize: second })
    first.mockClear()
    layout.resize('.measured', { clientWidth: 60 })

    expect(second).toHaveBeenCalledWith(60)
    expect(first).not.toHaveBeenCalled()
  })
})
