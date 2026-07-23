/**
 * Tier 3 a11y tests for the `Slider` primitive.
 *
 * Covers the bare track, the decorated shape (ticks, end labels, readout), and the disabled
 * state. Asserts axe-clean, a named `role="slider"` carrying the value, that the decorations
 * stay out of the accessibility tree, and that `ariaValueText` names the value when the raw
 * number wouldn't mean anything. Color contrast is tier 1's job; focus traps tier 2's.
 */

import { describe, it, expect } from 'vitest'
import { mount, tick, type ComponentProps } from 'svelte'
import Slider from './Slider.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

function mountSlider(props: ComponentProps<typeof Slider>): HTMLDivElement {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(Slider, { target, props })
  return target
}

const BUCKETS = ['Only my most-used', 'Often used', 'Sometimes used', 'Most folders', 'Everywhere']

describe('Slider a11y', () => {
  it('bare slider has no a11y violations', async () => {
    const target = mountSlider({ value: 50, onChange: () => {}, min: 0, max: 100, ariaLabel: 'Plain slider' })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('decorated slider (ticks, end labels, readout) has no a11y violations', async () => {
    const target = mountSlider({
      value: 100,
      onChange: () => {},
      min: 75,
      max: 150,
      step: 5,
      ariaLabel: 'Text size',
      ticks: [75, 100, 125, 150],
      snapTargets: [75, 100, 125, 150],
      endLabels: ['Smaller', 'Larger'],
      valueLabel: '100%',
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('disabled slider has no a11y violations', async () => {
    const target = mountSlider({
      value: 40,
      onChange: () => {},
      min: 0,
      max: 100,
      ariaLabel: 'Disabled slider',
      disabled: true,
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('exposes a named slider carrying the value', async () => {
    const target = mountSlider({ value: 30, onChange: () => {}, min: 0, max: 100, ariaLabel: 'Text size' })
    await tick()

    const slider = target.querySelector('[role="slider"]')
    expect(slider).not.toBeNull()
    expect(slider?.getAttribute('aria-label')).toBe('Text size')
    expect(slider?.getAttribute('aria-valuenow')).toBe('30')
    expect(slider?.getAttribute('aria-valuemin')).toBe('0')
    expect(slider?.getAttribute('aria-valuemax')).toBe('100')
  })

  it('announces a named value when the raw number is meaningless', async () => {
    const target = mountSlider({
      value: 3,
      onChange: () => {},
      min: 0,
      max: 4,
      ariaLabel: 'Coverage',
      ariaValueText: (v: number) => BUCKETS[v],
      valueLabel: BUCKETS[3],
      valueLabelPlacement: 'above',
    })
    await tick()

    const slider = target.querySelector('[role="slider"]')
    expect(slider?.getAttribute('aria-valuetext')).toBe('Most folders')
  })

  it('keeps the readout, ticks, and end labels out of the accessibility tree', async () => {
    const target = mountSlider({
      value: 6,
      onChange: () => {},
      min: 1,
      max: 9,
      ariaLabel: 'Compression level',
      ticks: [1, 5, 9],
      endLabels: ['Faster', 'Smaller'],
      valueLabel: '6',
    })
    await tick()

    // Duplicating the value and the track's decoration for a screen reader would just be noise:
    // the slider already announces its own value and bounds.
    for (const selector of ['.sl-value', '.sl-ticks', '.sl-ends']) {
      const el = target.querySelector(selector)
      expect(el, selector).not.toBeNull()
      expect(el?.getAttribute('aria-hidden'), selector).toBe('true')
    }
  })

  it('does not render a hidden input inside the thumb', async () => {
    // A focusable input nested in the thumb trips axe's nested-interactive rule, and nothing
    // here posts a form. Guards against a well-meaning "Ark supports HiddenInput" edit.
    const target = mountSlider({ value: 1, onChange: () => {}, min: 0, max: 2, ariaLabel: 'No hidden input' })
    await tick()
    expect(target.querySelector('input')).toBeNull()
  })
})
