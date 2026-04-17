/**
 * Tier 3 a11y tests for `McpServerSection.svelte`.
 */

import { describe, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import McpServerSection from './McpServerSection.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

vi.mock('$lib/settings/settings-store', () => ({
  getSetting: vi.fn((key: string) => {
    if (key === 'developer.mcpEnabled') return false
    if (key === 'developer.mcpPort') return 9224
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
  checkPortAvailable: vi.fn(() => Promise.resolve(true)),
  findAvailablePort: vi.fn(() => Promise.resolve(9224)),
  setMcpEnabled: vi.fn(() => Promise.resolve()),
  setMcpPort: vi.fn(() => Promise.resolve()),
  getMcpRunning: vi.fn(() => Promise.resolve(false)),
  getMcpPort: vi.fn(() => Promise.resolve(9224)),
}))

describe('McpServerSection a11y', () => {
  it('default (server off) has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(McpServerSection, { target, props: { searchQuery: '' } })
    await tick()
    await expectNoA11yViolations(target)
  })
})
