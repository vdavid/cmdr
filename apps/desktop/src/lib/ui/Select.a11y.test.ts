/**
 * Tier 3 a11y tests for the generic `Select` primitive.
 *
 * Covers the closed default, a grouped list, and the disabled state. Open-dropdown state is driven
 * by Ark UI state machines we don't exercise here; axe against the closed trigger is what tier 3
 * needs to catch trigger-label / aria regressions. Color contrast is tier 1's job; focus traps are
 * tier 2's.
 */

import { describe, it } from 'vitest'
import { mount, tick } from 'svelte'
import Select, { type SelectItem } from './Select.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

const flatItems: SelectItem[] = [
  { value: 'auto', label: 'Auto', description: 'Pick the unit that reads best' },
  { value: 'binary', label: 'Binary (KiB, MiB)' },
  { value: 'decimal', label: 'Decimal (KB, MB)' },
]

const groupedItems: SelectItem[] = [
  { value: 'utf-8', label: 'UTF-8', group: 'Unicode' },
  { value: 'utf-16le', label: 'UTF-16 LE', group: 'Unicode' },
  { value: 'windows-1252', label: 'Windows-1252', group: 'Western' },
]

describe('Select a11y', () => {
  it('closed (flat list with description) has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(Select, {
      target,
      props: { items: flatItems, value: 'auto', onChange: () => {}, ariaLabel: 'File size format' },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('closed (grouped list) has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(Select, {
      target,
      props: { items: groupedItems, value: 'utf-8', onChange: () => {}, ariaLabel: 'Text encoding' },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('disabled has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(Select, {
      target,
      props: { items: flatItems, value: 'auto', onChange: () => {}, ariaLabel: 'File size format', disabled: true },
    })
    await tick()
    await expectNoA11yViolations(target)
  })
})
