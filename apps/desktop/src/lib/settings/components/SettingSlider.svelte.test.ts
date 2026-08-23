/**
 * `SettingSlider`'s index-mapped mode, plus the pure mapping behind it.
 *
 * The plain mode (a linear track over `min`..`max`) is covered by the a11y suite and by every
 * row that uses it. What needs guarding here is the discrete mode, where the track and the
 * store speak DIFFERENT numbers: the track carries a stop index, the store carries the stop.
 * Get the direction wrong and the setting silently becomes "3 seconds" instead of "1 minute".
 */

import { describe, it, expect, vi } from 'vitest'
import { mount, tick } from 'svelte'
import SettingSlider from './SettingSlider.svelte'
import { nearestStopIndex, stopAt } from './slider-stops'

/** The Ask Cmdr wake-cadence table, in seconds. */
const STOPS = [5, 15, 30, 60, 120, 300, 900, 1800, 3600, 7200]

const setSetting = vi.fn()
let stored = 300

vi.mock('$lib/settings', () => ({
  getSetting: vi.fn(() => stored),
  setSetting: (...args: unknown[]) => {
    setSetting(...args)
  },
  getSettingDefinition: vi.fn(() => ({
    label: 'How soon Ask Cmdr looks',
    description: '',
    constraints: { min: 5, max: 7200, step: 1, sliderStops: STOPS, stopsAreDiscrete: true },
  })),
  getDefaultValue: vi.fn(() => 5),
  onSpecificSettingChange: vi.fn(() => () => {}),
}))

/** Mount the row and hand back its container. */
function render(props: Record<string, unknown> = {}): HTMLElement {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(SettingSlider, {
    target,
    props: { id: 'askCmdr.wakeDelay', formatValue: (seconds: number) => `${String(seconds)}s`, ...props },
  })
  return target
}

describe('nearestStopIndex', () => {
  it('places a value that IS a stop on that stop', () => {
    expect(nearestStopIndex(STOPS, 5)).toBe(0)
    expect(nearestStopIndex(STOPS, 300)).toBe(5)
    expect(nearestStopIndex(STOPS, 7200)).toBe(9)
  })

  // ❌ `indexOf` would answer -1 here, which reads as the first stop while the store still
  // holds the old number, so the control and the setting disagree with nobody the wiser.
  it('pulls a value that is NOT in the table onto its nearest stop', () => {
    expect(nearestStopIndex(STOPS, 40)).toBe(2) // 30 is nearer than 60
    expect(nearestStopIndex(STOPS, 1)).toBe(0) // below the table
    expect(nearestStopIndex(STOPS, 99_999)).toBe(9) // above it
  })

  it('answers 0 for an empty table rather than -1', () => {
    expect(nearestStopIndex([], 42)).toBe(0)
  })
})

describe('stopAt', () => {
  it('rounds a track position onto a stop and clamps to the table', () => {
    expect(stopAt(STOPS, 0)).toBe(5)
    expect(stopAt(STOPS, 4.4)).toBe(120)
    expect(stopAt(STOPS, -3)).toBe(5)
    expect(stopAt(STOPS, 40)).toBe(7200)
  })
})

describe('SettingSlider with discrete stops', () => {
  it('runs the track over the stop INDICES, not the values', async () => {
    stored = 300
    const target = render()
    await tick()

    const thumb = target.querySelector('[role="slider"]')
    expect(thumb?.getAttribute('aria-valuemin')).toBe('0')
    expect(thumb?.getAttribute('aria-valuemax')).toBe(String(STOPS.length - 1))
    // 300 s is the sixth stop, so the thumb sits at index 5 — ❌ never at 300, which a linear
    // track would need and which would put the first three stops inside one pixel.
    expect(thumb?.getAttribute('aria-valuenow')).toBe('5')
  })

  // ⚠️ The raw Ark value handed to `getAriaValueText` is the INDEX. Without the mapping back, a
  // screen reader announces "5" for a five-minute cadence.
  it('announces the stop, not the index', async () => {
    stored = 300
    const target = render()
    await tick()

    expect(target.querySelector('[role="slider"]')?.getAttribute('aria-valuetext')).toBe('300s')
  })

  it('shows the stop in the visible readout too', async () => {
    stored = 1800
    const target = render()
    await tick()

    expect(target.textContent).toContain('1800s')
  })

  // A hand-edited settings file is the only way this happens, and the control has to land
  // somewhere honest rather than on the first stop.
  it('seeds from the nearest stop when the stored value is off the table', async () => {
    stored = 40
    const target = render()
    await tick()

    const thumb = target.querySelector('[role="slider"]')
    expect(thumb?.getAttribute('aria-valuenow')).toBe('2')
    expect(thumb?.getAttribute('aria-valuetext')).toBe('30s')
  })
})
