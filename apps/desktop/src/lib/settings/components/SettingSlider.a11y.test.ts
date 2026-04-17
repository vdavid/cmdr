/**
 * Tier 3 a11y tests for `SettingSlider.svelte`.
 */

import { describe, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import SettingSlider from './SettingSlider.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

vi.mock('$lib/settings', () => ({
  getSetting: vi.fn(() => 50),
  setSetting: vi.fn(),
  getSettingDefinition: vi.fn(() => ({
    label: 'Progress update interval',
    description: '',
    constraints: { min: 0, max: 100, step: 10, sliderStops: [0, 25, 50, 75, 100] },
  })),
  getDefaultValue: vi.fn(() => 50),
  onSpecificSettingChange: vi.fn(() => () => {}),
}))

describe('SettingSlider a11y', () => {
  it('default has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SettingSlider, { target, props: { id: 'fileOperations.progressUpdateInterval' } })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('with unit label has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SettingSlider, { target, props: { id: 'fileOperations.progressUpdateInterval', unit: 'ms' } })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('disabled has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SettingSlider, { target, props: { id: 'fileOperations.progressUpdateInterval', disabled: true } })
    await tick()
    await expectNoA11yViolations(target)
  })
})
