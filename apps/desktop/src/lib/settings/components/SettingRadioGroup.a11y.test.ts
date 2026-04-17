/**
 * Tier 3 a11y tests for `SettingRadioGroup.svelte`.
 */

import { describe, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import SettingRadioGroup from './SettingRadioGroup.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

vi.mock('$lib/settings', () => ({
  getSetting: vi.fn(() => 'iso'),
  setSetting: vi.fn(),
  getSettingDefinition: vi.fn(() => ({
    label: 'Date/time format',
    description: '',
    constraints: {
      options: [
        { value: 'iso', label: 'ISO 8601', description: '2025-04-16 10:30' },
        { value: 'us', label: 'US', description: '4/16/2025 10:30 AM' },
        { value: 'custom', label: 'Custom', description: 'Define your own format' },
      ],
    },
  })),
  onSpecificSettingChange: vi.fn(() => () => {}),
}))

describe('SettingRadioGroup a11y', () => {
  it('default has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SettingRadioGroup, { target, props: { id: 'appearance.dateTimeFormat' } })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('disabled has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SettingRadioGroup, { target, props: { id: 'appearance.dateTimeFormat', disabled: true } })
    await tick()
    await expectNoA11yViolations(target)
  })
})
