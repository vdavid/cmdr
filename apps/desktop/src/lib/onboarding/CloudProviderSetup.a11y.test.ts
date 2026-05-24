/**
 * Tier 3 axe a11y tests for `CloudProviderSetup.svelte` (M3).
 */

import { describe, it, vi, beforeEach, afterEach } from 'vitest'
import { mount, tick, unmount, flushSync } from 'svelte'
import CloudProviderSetup from './CloudProviderSetup.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

vi.mock('$lib/tauri-commands', () => ({
  checkAiConnection: vi.fn(() => Promise.resolve({ connected: false, authError: false, models: [], error: null })),
  saveAiApiKey: vi.fn(() => Promise.resolve(null)),
  getAiApiKey: vi.fn(() => Promise.resolve('')),
  openExternalUrl: vi.fn(() => Promise.resolve()),
}))

const settingsMap: Record<string, unknown> = { 'ai.cloudProviderConfigs': '{}' }
vi.mock('$lib/settings', async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>()
  return {
    ...actual,
    getSetting: (id: string) => settingsMap[id] ?? '',
    setSetting: (id: string, value: unknown) => {
      settingsMap[id] = value
    },
    onSpecificSettingChange: () => () => {},
  }
})

let mounted: { target: HTMLElement; instance: ReturnType<typeof mount> } | undefined

async function settle(): Promise<void> {
  for (let i = 0; i < 20; i++) {
    await Promise.resolve()
  }
  await tick()
  flushSync()
}

beforeEach(() => {
  settingsMap['ai.cloudProviderConfigs'] = '{}'
})

afterEach(() => {
  if (mounted) {
    unmount(mounted.instance)
    mounted.target.remove()
    mounted = undefined
  }
})

describe('CloudProviderSetup a11y', () => {
  it('OpenAI provider state has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    const instance = mount(CloudProviderSetup, { target, props: { providerId: 'openai' } })
    mounted = { target, instance }
    await settle()
    await expectNoA11yViolations(target)
  })

  it('Custom provider (editable endpoint) has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    const instance = mount(CloudProviderSetup, { target, props: { providerId: 'custom' } })
    mounted = { target, instance }
    await settle()
    await expectNoA11yViolations(target)
  })
})
