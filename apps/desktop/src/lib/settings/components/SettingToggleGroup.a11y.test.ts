/**
 * Tier 3 a11y tests for `SettingToggleGroup.svelte`.
 */

import { describe, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import SettingToggleGroup from './SettingToggleGroup.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

vi.mock('$lib/settings', () => ({
  getSetting: vi.fn(() => 'comfortable'),
  setSetting: vi.fn(),
  getSettingDefinition: vi.fn(() => ({
    label: 'UI density',
    description: '',
    constraints: {
      options: [
        { value: 'compact', label: 'Compact' },
        { value: 'comfortable', label: 'Comfortable' },
        { value: 'spacious', label: 'Spacious' },
      ],
    },
  })),
  onSpecificSettingChange: vi.fn(() => () => {}),
}))

describe('SettingToggleGroup a11y', () => {
  it('default has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SettingToggleGroup, { target, props: { id: 'appearance.uiDensity' } })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('disabled has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SettingToggleGroup, { target, props: { id: 'appearance.uiDensity', disabled: true } })
    await tick()
    await expectNoA11yViolations(target)
  })
})
