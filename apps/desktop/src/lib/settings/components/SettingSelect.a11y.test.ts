/**
 * Tier 3 a11y tests for `SettingSelect.svelte`.
 *
 * Covers the closed default. Open-dropdown state is driven by Ark UI
 * state machines we don't exercise here; axe against the closed state
 * is what tier 3 needs to catch trigger-label regressions.
 */

import { describe, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import SettingSelect from './SettingSelect.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

vi.mock('$lib/settings', () => ({
  getSetting: vi.fn(() => 'auto'),
  setSetting: vi.fn(),
  getSettingDefinition: vi.fn(() => ({
    label: 'File size format',
    description: '',
    constraints: {
      options: [
        { value: 'auto', label: 'Auto' },
        { value: 'binary', label: 'Binary (KiB, MiB)' },
        { value: 'decimal', label: 'Decimal (KB, MB)' },
      ],
    },
  })),
  onSpecificSettingChange: vi.fn(() => () => {}),
}))

describe('SettingSelect a11y', () => {
  it('closed (default value) has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SettingSelect, { target, props: { id: 'appearance.fileSizeFormat' } })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('disabled has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SettingSelect, { target, props: { id: 'appearance.fileSizeFormat', disabled: true } })
    await tick()
    await expectNoA11yViolations(target)
  })
})
