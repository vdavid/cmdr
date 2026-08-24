import { beforeEach, describe, expect, it, vi } from 'vitest'

const { trackEventSpy } = vi.hoisted(() => ({
  trackEventSpy: vi.fn<(name: string, props: Record<string, unknown>) => Promise<void>>(() => Promise.resolve()),
}))
vi.mock('$lib/tauri-commands', () => ({ trackEvent: trackEventSpy }))

import { reportTabClosed, reportTabOpened, reportTabPinToggled, reportTabSwitched } from './tab-analytics'

beforeEach(() => {
  vi.clearAllMocks()
})

/** The name and props of the single event the call under test produced. */
function sent(): [string, Record<string, unknown>] {
  expect(trackEventSpy).toHaveBeenCalledTimes(1)
  return trackEventSpy.mock.calls[0]
}

describe('tab analytics', () => {
  it('ships the open count RAW, not through the item-count bucket', () => {
    // The one documented deviation from `itemCountBucket`: a pane caps at ten
    // tabs, where that ladder has two values (`1`, `2-10`) across the entire
    // range. Bucketing here would throw the answer away for no privacy gain, so
    // if someone "fixes" this for consistency, this test says why not.
    reportTabOpened('new', 'opened', 7)
    expect(sent()).toEqual(['tab_opened', { source: 'new', outcome: 'opened', open_tabs: 7 }])
  })

  it('reports a refused open, so a low reopen count is readable', () => {
    reportTabOpened('reopened', 'atCap', 10)
    expect(sent()[1]).toMatchObject({ source: 'reopened', outcome: 'atCap' })
  })

  it('reports a close with the pin state of the tab that was targeted', () => {
    reportTabClosed('single', 'cancelled', 3, true)
    expect(sent()).toEqual(['tab_closed', { source: 'single', outcome: 'cancelled', open_tabs: 3, pinned: true }])
  })

  it('names the gesture that moved the active tab', () => {
    reportTabSwitched('cycle')
    expect(sent()).toEqual(['tab_switched', { method: 'cycle' }])
  })

  it('reports the state a pin toggle lands in, not the one it left', () => {
    reportTabPinToggled(true)
    expect(sent()).toEqual(['tab_pin_toggled', { pinned: true }])
  })

  it('carries no path anywhere, since a path is a tab whole identity', () => {
    reportTabOpened('new', 'opened', 1)
    reportTabClosed('others', 'closed', 1, false)
    reportTabSwitched('pick')
    reportTabPinToggled(false)
    const everyValue = trackEventSpy.mock.calls.flatMap(([, props]) => Object.values(props))
    for (const value of everyValue) {
      expect(typeof value === 'string' ? value.includes('/') : false).toBe(false)
    }
  })
})
