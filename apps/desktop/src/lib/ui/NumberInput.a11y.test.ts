/**
 * Tier 3 a11y tests for the `NumberInput` primitive.
 *
 * Covers the plain field, one with a unit, and the disabled state. Asserts axe-clean, a named
 * spinbutton, that both steppers carry an accessible name naming the field, and that an emptied
 * field doesn't commit `NaN`. Color contrast is tier 1's job; focus traps tier 2's.
 */

import { describe, it, expect } from 'vitest'
import { mount, tick, type ComponentProps } from 'svelte'
import NumberInput from './NumberInput.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

function mountInput(props: ComponentProps<typeof NumberInput>): HTMLDivElement {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(NumberInput, { target, props })
  return target
}

describe('NumberInput a11y', () => {
  it('plain field has no a11y violations', async () => {
    const target = mountInput({ value: 5, onChange: () => {}, min: 0, max: 10, ariaLabel: 'Parallel workers' })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('field with a unit has no a11y violations', async () => {
    const target = mountInput({
      value: 400,
      onChange: () => {},
      min: 250,
      max: 1000,
      step: 25,
      unit: 'px',
      ariaLabel: 'Column width limit',
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('disabled field has no a11y violations', async () => {
    const target = mountInput({
      value: 12,
      onChange: () => {},
      min: 0,
      max: 99,
      ariaLabel: 'Disabled number input',
      disabled: true,
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('names the field and both steppers', async () => {
    const target = mountInput({ value: 5, onChange: () => {}, min: 0, max: 10, ariaLabel: 'Parallel workers' })
    await tick()

    const input = target.querySelector('input')
    expect(input?.getAttribute('aria-label')).toBe('Parallel workers')

    const stepperNames = [...target.querySelectorAll('button')].map((b) => b.getAttribute('aria-label'))
    expect(stepperNames).toEqual(['Decrease Parallel workers', 'Increase Parallel workers'])
  })

  it('clamps to the bounds and never commits an emptied field', async () => {
    const seen: number[] = []
    const target = mountInput({
      value: 5,
      onChange: (v: number) => {
        seen.push(v)
      },
      min: 1,
      max: 9,
      ariaLabel: 'Compression level',
    })
    await tick()

    const input = target.querySelector('input')
    if (!input) throw new Error('number input not found')

    // Ark reads the field through its focused-input handler, so focus first: an `input` event on
    // an unfocused field is ignored and this test would silently assert nothing.
    input.focus()
    input.value = '50'
    input.dispatchEvent(new Event('input', { bubbles: true }))
    await tick()

    // Emptying the field parses as NaN. Committing it would write a broken number to the store,
    // so it's swallowed until Ark's clamp-on-blur restores a real value.
    input.value = ''
    input.dispatchEvent(new Event('input', { bubbles: true }))
    await tick()

    expect(seen).toEqual([9])
  })
})
