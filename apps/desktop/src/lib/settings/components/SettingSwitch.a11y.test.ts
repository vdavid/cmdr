/**
 * Tier 3 a11y tests for `SettingSwitch.svelte`.
 */

import { describe, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import SettingSwitch from './SettingSwitch.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

vi.mock('$lib/settings', () => ({
  getSetting: vi.fn(() => false),
  setSetting: vi.fn(),
  getSettingDefinition: vi.fn(() => ({ label: 'Striped rows', description: '' })),
  onSpecificSettingChange: vi.fn(() => () => {}),
}))

describe('SettingSwitch a11y', () => {
  it('default (unchecked) has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SettingSwitch, { target, props: { id: 'listing.stripedRows' } })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('disabled has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SettingSwitch, { target, props: { id: 'listing.stripedRows', disabled: true } })
    await tick()
    await expectNoA11yViolations(target)
  })
})
