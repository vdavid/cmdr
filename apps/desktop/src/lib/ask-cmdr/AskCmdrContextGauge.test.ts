/**
 * `AskCmdrContextGauge.svelte`: one case per named state, plus its a11y surface.
 *
 * The states are the contract the rest of the UI reasons about (calm / filling / setAside /
 * unmeasured), so each gets its own case here rather than one test that walks a percentage.
 */

import { flushSync, mount } from 'svelte'
import { beforeAll, describe, expect, it } from 'vitest'
import { _setLocaleForTests } from '$lib/intl/locale'
import AskCmdrContextGauge from './AskCmdrContextGauge.svelte'
import type { ContextUsage } from './ask-cmdr-context-usage'

beforeAll(() => {
  _setLocaleForTests('en-US')
})

function render(usage: ContextUsage | null): HTMLElement {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(AskCmdrContextGauge, { target, props: { usage } })
  flushSync()
  return target
}

const usage = (estimatedTokens: number, budgetTokens: number, elidedResults = 0): ContextUsage => ({
  estimatedTokens,
  budgetTokens,
  elidedResults,
})

describe('AskCmdrContextGauge', () => {
  it('renders nothing before a turn has been measured', () => {
    // Not a 0% bar: that would read as "plenty of room" for a thread nobody measured.
    const target = render(null)
    expect(target.querySelector('.gauge')).toBeNull()
    target.remove()
  })

  it('shows a calm gauge with its percentage under the threshold', () => {
    const target = render(usage(31_200, 60_000))
    const gauge = target.querySelector('.gauge')
    expect(gauge?.getAttribute('data-state')).toBe('calm')
    expect(target.textContent).toContain('52%')
    target.remove()
  })

  it('shows the real figures in the tooltip, labelled as estimates', () => {
    const target = render(usage(31_200, 60_000))
    // The hover tooltip is a `use:tooltip` action, which renders into the shared
    // tooltip element only once hovered. The same string is the meter's
    // `aria-valuetext`, so asserting there covers the wording for both surfaces.
    const text = target.querySelector('[role="meter"]')?.getAttribute('aria-valuetext')
    // Thousands separators, no "k" abbreviations, and honest about being an estimate.
    expect(text).toBe('31,200 of 60,000 tokens used (estimated)')
    target.remove()
  })

  it('marks a filling gauge once it passes the threshold', () => {
    const target = render(usage(50_000, 60_000))
    expect(target.querySelector('.gauge')?.getAttribute('data-state')).toBe('filling')
    target.remove()
  })

  it('marks a set-aside gauge when history was dropped, whatever the fill', () => {
    const target = render(usage(12_000, 60_000, 3))
    expect(target.querySelector('.gauge')?.getAttribute('data-state')).toBe('setAside')
    target.remove()
  })

  it('caps the fill at 100% when the prompt went over budget', () => {
    // Over budget renders as set aside with a full bar, never as a fourth state, and the
    // fill must not overrun its track.
    const target = render(usage(90_000, 60_000))
    const gauge = target.querySelector('.gauge')
    expect(gauge?.getAttribute('data-state')).toBe('setAside')
    expect(gauge?.querySelector<HTMLElement>('.fill')?.style.width).toBe('100%')
    expect(target.textContent).toContain('100%')
    target.remove()
  })

  it('exposes the reading to assistive tech', () => {
    const target = render(usage(31_200, 60_000))
    const meter = target.querySelector('[role="meter"]')
    expect(meter?.getAttribute('aria-valuenow')).toBe('52')
    expect(meter?.getAttribute('aria-valuetext')).toContain('31,200 of 60,000')
    target.remove()
  })
})
