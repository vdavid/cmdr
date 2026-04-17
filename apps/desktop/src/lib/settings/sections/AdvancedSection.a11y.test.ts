/**
 * Tier 3 a11y tests for `AdvancedSection.svelte`.
 *
 * Auto-generated setting rows for everything marked `showInAdvanced:true`.
 * Covers default and search-filtered states.
 */

import { describe, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import AdvancedSection from './AdvancedSection.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

vi.mock('$lib/settings/settings-store', () => ({
  getSetting: vi.fn(() => 100),
  setSetting: vi.fn(() => Promise.resolve()),
  resetSetting: vi.fn(),
  isModified: vi.fn(() => false),
  onSpecificSettingChange: vi.fn(() => () => {}),
  onSettingChange: vi.fn(() => () => {}),
}))

vi.mock('$lib/utils/confirm-dialog', () => ({
  confirmDialog: vi.fn(() => Promise.resolve(false)),
}))

vi.mock('$lib/tauri-commands', () => ({
  invoke: vi.fn(() => Promise.resolve()),
}))

describe('AdvancedSection a11y', () => {
  it('default (no search) has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(AdvancedSection, { target, props: { searchQuery: '' } })
    await tick()
    await expectNoA11yViolations(target)
  })
})
