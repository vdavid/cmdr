/**
 * Tier 3 a11y tests for `CompressLevelControl.svelte`.
 *
 * A thin frame around the shared `SettingSlider`. The settings barrel is mocked
 * so the slider renders without a store.
 */

import { describe, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

vi.mock('$lib/settings', () => ({
  getSetting: vi.fn(() => 6),
  setSetting: vi.fn(),
  resetSetting: vi.fn(),
  isModified: vi.fn(() => false),
  onSpecificSettingChange: vi.fn(() => () => {}),
  onSettingChange: vi.fn(() => () => {}),
  getDefaultValue: vi.fn(() => 6),
  getSettingDefinition: vi.fn(() => ({
    label: 'Compression level',
    description: '',
    constraints: { min: 1, max: 9, step: 1, sliderStops: [1, 2, 3, 4, 5, 6, 7, 8, 9] },
  })),
}))

import CompressLevelControl from './CompressLevelControl.svelte'

describe('CompressLevelControl a11y', () => {
  it('has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(CompressLevelControl, { target })
    await tick()
    await expectNoA11yViolations(target)
  })
})
