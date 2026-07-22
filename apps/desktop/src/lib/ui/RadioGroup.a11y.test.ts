/**
 * Tier 3 a11y tests for the generic `RadioGroup` primitive.
 *
 * Covers the default vertical group, a group with per-item descriptions, and one with a disabled
 * item. Asserts axe-clean, the `radiogroup` / `radio` roles with accessible names drawn from the
 * labels, and that activating an option updates the value. Color contrast is tier 1's job; focus
 * traps tier 2's.
 */

import { describe, it, expect } from 'vitest'
import { mount, tick, type ComponentProps } from 'svelte'
import RadioGroup, { type RadioItem } from './RadioGroup.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

const items: RadioItem[] = [
  { value: 'iso', label: 'ISO 8601' },
  { value: 'us', label: 'US' },
  { value: 'eu', label: 'European' },
]

const itemsWithDescriptions: RadioItem[] = [
  { value: 'iso', label: 'ISO 8601', description: '2025-04-16 10:30' },
  { value: 'us', label: 'US', description: '4/16/2025 10:30 AM' },
  { value: 'custom', label: 'Custom', description: 'Define your own format' },
]

const itemsWithDisabled: RadioItem[] = [
  { value: 'auto', label: 'Automatic' },
  { value: 'manual', label: 'Manual' },
  { value: 'off', label: 'Off', disabled: true },
]

function mountGroup(props: ComponentProps<typeof RadioGroup>): HTMLDivElement {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(RadioGroup, { target, props })
  return target
}

// Ark renders each radio as a visually-hidden native `<input type="radio">` whose accessible name
// comes from the `aria-labelledby` label span. Resolve it the way assistive tech would.
function accessibleName(input: HTMLInputElement): string {
  const labelledBy = input.getAttribute('aria-labelledby')
  if (!labelledBy) return ''
  return document.getElementById(labelledBy)?.textContent.trim() ?? ''
}

describe('RadioGroup a11y', () => {
  it('vertical group has no a11y violations', async () => {
    const target = mountGroup({ items, value: 'iso', ariaLabel: 'Date format' })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('group with descriptions has no a11y violations', async () => {
    const target = mountGroup({ items: itemsWithDescriptions, value: 'iso', ariaLabel: 'Date format' })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('group with a disabled item has no a11y violations', async () => {
    const target = mountGroup({ items: itemsWithDisabled, value: 'auto', ariaLabel: 'Sync mode' })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('exposes a radiogroup with radios named after their labels', async () => {
    const target = mountGroup({ items, value: 'iso', ariaLabel: 'Date format' })
    await tick()

    const group = target.querySelector('[role="radiogroup"]')
    expect(group).not.toBeNull()
    expect(group?.getAttribute('aria-label')).toBe('Date format')

    const radios = [...target.querySelectorAll<HTMLInputElement>('input[type="radio"]')]
    expect(radios.map((r) => accessibleName(r))).toEqual(['ISO 8601', 'US', 'European'])

    const checked = radios.filter((r) => r.checked)
    expect(checked).toHaveLength(1)
    expect(accessibleName(checked[0])).toBe('ISO 8601')
  })

  it('activating an option updates the value', async () => {
    let current = 'iso'
    const target = mountGroup({
      items,
      value: 'iso',
      ariaLabel: 'Date format',
      onValueChange: (v: string) => {
        current = v
      },
    })
    await tick()

    const us = [...target.querySelectorAll<HTMLLabelElement>('.radio-item')].find(
      (label) => label.querySelector('.radio-label')?.textContent.trim() === 'US',
    )
    expect(us).toBeTruthy()
    us?.click()
    await tick()
    expect(current).toBe('us')
  })
})
