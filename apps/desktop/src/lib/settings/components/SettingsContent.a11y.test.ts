/**
 * Tier 3 a11y tests for `SettingsContent.svelte`.
 *
 * Dispatcher that picks which section component to render based on the
 * selectedSection path and searchQuery. Tests cover a few representative
 * paths. Child sections pull heavy state — we rely on global mocks for
 * `$lib/settings/settings-store` (see each test file's vi.mock).
 */

import { describe, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import SettingsContent from './SettingsContent.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

vi.mock('$lib/settings/settings-store', () => ({
  getSetting: vi.fn(() => undefined),
  setSetting: vi.fn(() => Promise.resolve()),
  resetSetting: vi.fn(),
  isModified: vi.fn(() => false),
  onSpecificSettingChange: vi.fn(() => () => {}),
  onSettingChange: vi.fn(() => () => {}),
}))

vi.mock('$lib/tauri-commands', () => ({
  openAppearanceSettings: vi.fn(() => Promise.resolve()),
  invoke: vi.fn(() => Promise.resolve(null)),
  listen: vi.fn(() => Promise.resolve(() => {})),
}))

describe('SettingsContent a11y', () => {
  it('General summary page has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SettingsContent, {
      target,
      props: {
        searchQuery: '',
        selectedSection: ['General'],
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })
})
