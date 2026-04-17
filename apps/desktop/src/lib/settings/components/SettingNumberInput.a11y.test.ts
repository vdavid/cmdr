/**
 * Tier 3 a11y tests for `SettingNumberInput.svelte`.
 */

import { describe, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import SettingNumberInput from './SettingNumberInput.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

vi.mock('$lib/settings', () => ({
  getSetting: vi.fn(() => 200),
  setSetting: vi.fn(),
  getSettingDefinition: vi.fn(() => ({
    label: 'Max conflicts to show',
    description: '',
    constraints: { min: 10, max: 1000, step: 10 },
  })),
  onSpecificSettingChange: vi.fn(() => () => {}),
}))

describe('SettingNumberInput a11y', () => {
  it('default has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SettingNumberInput, { target, props: { id: 'fileOperations.maxConflictsToShow' } })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('with unit label has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SettingNumberInput, { target, props: { id: 'fileOperations.maxConflictsToShow', unit: 'files' } })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('disabled has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SettingNumberInput, { target, props: { id: 'fileOperations.maxConflictsToShow', disabled: true } })
    await tick()
    await expectNoA11yViolations(target)
  })
})
