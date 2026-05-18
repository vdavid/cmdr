/**
 * Tier 3 a11y tests for `SettingColorSwatchPicker.svelte`.
 *
 * Covers the trigger button (closed) and the open popover with the swatch
 * grid. Contrast on tinted backgrounds is checked at design time by
 * `scripts/check-a11y-contrast` (tier 1).
 */

import { describe, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import SettingColorSwatchPicker from './SettingColorSwatchPicker.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

let currentValue: string = 'none'

vi.mock('$lib/settings/settings-store', () => ({
  getSetting: vi.fn(() => currentValue),
  setSetting: vi.fn((_id: string, v: string) => {
    currentValue = v
    return Promise.resolve()
  }),
  resetSetting: vi.fn(),
  isModified: vi.fn(() => false),
  onSpecificSettingChange: vi.fn(() => () => {}),
  onSettingChange: vi.fn(() => () => {}),
}))

describe('SettingColorSwatchPicker a11y', () => {
  it('default (closed, no tint) has no a11y violations', async () => {
    currentValue = 'none'
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SettingColorSwatchPicker, {
      target,
      props: { id: 'appearance.tintLocal', label: 'Tint local-volume panes' },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('closed with a selected color has no a11y violations', async () => {
    currentValue = 'blue'
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SettingColorSwatchPicker, {
      target,
      props: { id: 'appearance.tintLocal', label: 'Tint local-volume panes' },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('open popover (with swatch grid) has no a11y violations', async () => {
    currentValue = 'none'
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SettingColorSwatchPicker, {
      target,
      props: { id: 'appearance.tintLocal', label: 'Tint local-volume panes' },
    })
    await tick()
    // Open the popover via the trigger
    target.querySelector<HTMLButtonElement>('button.trigger')?.click()
    await tick()
    await expectNoA11yViolations(target)
  })
})
