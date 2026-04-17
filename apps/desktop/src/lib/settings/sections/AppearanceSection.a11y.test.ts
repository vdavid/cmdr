/**
 * Tier 3 a11y tests for `AppearanceSection.svelte`.
 *
 * Representative settings section audited end-to-end with its child
 * SettingRow/SettingSwitch/SettingSelect/SettingRadioGroup tree. The
 * settings-store is stubbed so the section can mount without real IPC.
 */

import { describe, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import AppearanceSection from './AppearanceSection.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

vi.mock('$lib/settings/settings-store', () => ({
  getSetting: vi.fn((key: string) => {
    if (key === 'appearance.appColor') return 'system'
    if (key === 'appearance.uiDensity') return 'comfortable'
    if (key === 'appearance.useAppIconsForDocuments') return false
    if (key === 'appearance.fileSizeFormat') return 'auto'
    if (key === 'appearance.dateTimeFormat') return 'iso'
    if (key === 'appearance.customDateTimeFormat') return 'YYYY-MM-DD HH:mm'
    return undefined
  }),
  setSetting: vi.fn(() => Promise.resolve()),
  resetSetting: vi.fn(),
  isModified: vi.fn(() => false),
  onSpecificSettingChange: vi.fn(() => () => {}),
  onSettingChange: vi.fn(() => () => {}),
}))

vi.mock('$lib/tauri-commands', () => ({
  openAppearanceSettings: vi.fn(() => Promise.resolve()),
  invoke: vi.fn(() => Promise.resolve()),
}))

describe('AppearanceSection a11y', () => {
  it('default (no search) has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(AppearanceSection, { target, props: { searchQuery: '' } })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('with search query (partial match) has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(AppearanceSection, { target, props: { searchQuery: 'density' } })
    await tick()
    await expectNoA11yViolations(target)
  })
})
