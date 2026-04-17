/**
 * Tier 3 a11y tests for `SettingCheckbox.svelte`.
 */

import { describe, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import SettingCheckbox from './SettingCheckbox.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

vi.mock('$lib/settings', () => ({
  getSetting: vi.fn(() => false),
  setSetting: vi.fn(),
  getSettingDefinition: vi.fn(() => ({ label: 'Warn on size mismatch', description: '' })),
  onSpecificSettingChange: vi.fn(() => () => {}),
}))

describe('SettingCheckbox a11y', () => {
  it('default (unchecked) has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SettingCheckbox, { target, props: { id: 'listing.sizeMismatchWarning' } })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('disabled has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SettingCheckbox, { target, props: { id: 'listing.sizeMismatchWarning', disabled: true } })
    await tick()
    await expectNoA11yViolations(target)
  })
})
