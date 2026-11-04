import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { trackScrollbarWidth } from './scrollbar-width'
import { installLayoutMock } from '#lib/test-layout.js'

const PROBE = '[data-scrollbar-probe]'

function token(): string {
  return document.documentElement.style.getPropertyValue('--spacing-scrollbar-width')
}

/**
 * The token is what keeps a gutter-reserving scroller's content on the title's inset
 * (#889), so it has to be right at startup AND follow macOS flipping the scrollbar
 * style when a mouse comes or goes, without a reload.
 */
describe('trackScrollbarWidth', () => {
  let stop: (() => void) | null = null

  beforeEach(() => {
    document.body.innerHTML = ''
  })

  afterEach(() => {
    stop?.()
    stop = null
  })

  it('writes a classic scrollbar’s width to :root at startup', () => {
    installLayoutMock({ [PROBE]: { offsetWidth: 100, clientWidth: 83 } })

    stop = trackScrollbarWidth()

    expect(token()).toBe('17px')
  })

  it('writes 0px under overlay scrollbars', () => {
    installLayoutMock({ [PROBE]: { offsetWidth: 100, clientWidth: 100 } })

    stop = trackScrollbarWidth()

    expect(token()).toBe('0px')
  })

  it('follows the scrollbar style flipping at runtime (a mouse plugged in)', () => {
    const layout = installLayoutMock({ [PROBE]: { offsetWidth: 100, clientWidth: 100 } })
    stop = trackScrollbarWidth()

    layout.resize(PROBE, { offsetWidth: 100, clientWidth: 83 })

    expect(token()).toBe('17px')
  })

  it('removes the probe and the token on teardown', () => {
    installLayoutMock({ [PROBE]: { offsetWidth: 100, clientWidth: 83 } })
    stop = trackScrollbarWidth()

    stop()
    stop = null

    expect(document.querySelector(PROBE)).toBeNull()
    expect(token()).toBe('')
  })
})
