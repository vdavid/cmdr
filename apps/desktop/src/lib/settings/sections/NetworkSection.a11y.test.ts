/**
 * Tier 3 a11y tests for `NetworkSection.svelte`.
 */

import { describe, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import NetworkSection from './NetworkSection.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

vi.mock('$lib/settings/settings-store', () => ({
  getSetting: vi.fn((key: string) => {
    if (key === 'network.directSmbConnection') return true
    if (key === 'network.shareCacheDuration') return 300
    if (key === 'network.timeoutMode') return 'balanced'
    return undefined
  }),
  setSetting: vi.fn(() => Promise.resolve()),
  resetSetting: vi.fn(),
  isModified: vi.fn(() => false),
  onSpecificSettingChange: vi.fn(() => () => {}),
  onSettingChange: vi.fn(() => () => {}),
}))

vi.mock('$lib/tauri-commands', () => ({
  invoke: vi.fn(() => Promise.resolve()),
}))

describe('NetworkSection a11y', () => {
  it('default (no search) has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(NetworkSection, { target, props: { searchQuery: '' } })
    await tick()
    await expectNoA11yViolations(target)
  })
})
