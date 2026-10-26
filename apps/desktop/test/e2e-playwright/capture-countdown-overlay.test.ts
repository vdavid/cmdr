/**
 * Tests for the in-page half of the capture countdown: the script the specs inject
 * into the Cmdr webview, run here against happy-dom's `window`, plus the env reader.
 * The Node-side loop needs a running app, so the two capture runs prove it.
 */

import { describe, it, expect, afterEach, vi } from 'vitest'
import {
  OVERLAY_ID,
  countdownFromEnv,
  installOverlayScript,
  removeOverlayScript,
  setCountScript,
} from './capture-countdown-overlay.js'

/** Runs an injected script the way `page.evaluate` would: as one expression in the page. */
function inject(script: string): unknown {
  // eslint-disable-next-line @typescript-eslint/no-implied-eval -- mirrors `page.evaluate(string)`
  const evaluate = new Function(`return (${script})`) as () => unknown
  return evaluate()
}

function overlay(): HTMLElement | null {
  return document.getElementById(OVERLAY_ID)
}

afterEach(() => {
  inject(removeOverlayScript())
  vi.useRealTimers()
})

describe('installOverlayScript', () => {
  it('covers the window with the count and the estimate line', () => {
    inject(installOverlayScript({ seconds: 3, line: 'This will take about 25 seconds.' }))
    const el = overlay()
    expect(el).not.toBeNull()
    expect(el?.style.position).toBe('fixed')
    expect(el?.textContent).toContain('3')
    expect(el?.textContent).toContain('This will take about 25 seconds.')
  })

  it('swallows key presses and clicks before the app sees them', () => {
    const seen: string[] = []
    const onKey = (e: Event) => seen.push(e.type)
    document.addEventListener('keydown', onKey)
    document.addEventListener('click', onKey)
    inject(installOverlayScript({ seconds: 3, line: '' }))

    const key = new KeyboardEvent('keydown', { key: 'a', bubbles: true, cancelable: true })
    document.body.dispatchEvent(key)
    document.body.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true }))

    expect(seen).toEqual([])
    expect(key.defaultPrevented).toBe(true)
    document.removeEventListener('keydown', onKey)
    document.removeEventListener('click', onKey)
  })

  it('is idempotent: a second install leaves one overlay', () => {
    inject(installOverlayScript({ seconds: 3, line: '' }))
    inject(installOverlayScript({ seconds: 3, line: '' }))
    expect(document.querySelectorAll(`#${OVERLAY_ID}`)).toHaveLength(1)
  })

  it('removes itself after its hard timeout even if nobody calls remove', () => {
    vi.useFakeTimers()
    inject(installOverlayScript({ seconds: 3, line: '' }))
    vi.advanceTimersByTime(3000 + 5000)
    expect(overlay()).toBeNull()
  })
})

describe('setCountScript', () => {
  it('updates the big number', () => {
    inject(installOverlayScript({ seconds: 3, line: 'x' }))
    inject(setCountScript(2))
    expect(overlay()?.querySelector('[data-countdown-count]')?.textContent).toBe('2')
  })
})

describe('removeOverlayScript', () => {
  it('takes the overlay away and gives the input back', () => {
    const seen: string[] = []
    const onKey = (e: Event) => seen.push(e.type)
    document.addEventListener('keydown', onKey)
    inject(installOverlayScript({ seconds: 3, line: '' }))
    inject(removeOverlayScript())

    document.body.dispatchEvent(new KeyboardEvent('keydown', { key: 'a', bubbles: true }))
    expect(overlay()).toBeNull()
    expect(seen).toEqual(['keydown'])
    document.removeEventListener('keydown', onKey)
  })

  it('is safe when nothing is installed', () => {
    expect(() => inject(removeOverlayScript())).not.toThrow()
  })
})

describe('countdownFromEnv', () => {
  it('reads the seconds and line the orchestrator passes', () => {
    expect(
      countdownFromEnv({
        CMDR_CAPTURE_COUNTDOWN_S: '3',
        CMDR_CAPTURE_COUNTDOWN_LINE: 'This will take about 25 seconds.',
      }),
    ).toEqual({ seconds: 3, line: 'This will take about 25 seconds.' })
  })

  it('answers null when unset, so a lane run of the same spec never counts down', () => {
    expect(countdownFromEnv({})).toBeNull()
    expect(countdownFromEnv({ CMDR_CAPTURE_COUNTDOWN_S: 'soon', CMDR_CAPTURE_COUNTDOWN_LINE: 'x' })).toBeNull()
    expect(countdownFromEnv({ CMDR_CAPTURE_COUNTDOWN_S: '0', CMDR_CAPTURE_COUNTDOWN_LINE: 'x' })).toBeNull()
  })
})
