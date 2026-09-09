/**
 * Tier 3 axe a11y tests for `ProviderSetupSteps.svelte`, across the three shapes the plan
 * produces: a cloud provider (links + key), one with an editable endpoint, and a local one
 * (no key step at all).
 */

import { describe, it, vi, beforeEach, afterEach } from 'vitest'
import { mount, tick, unmount, flushSync } from 'svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

const stubs = vi.hoisted(() => ({
  settingsMap: Object.create(null) as Record<string, unknown>,
}))

vi.mock('$lib/settings', async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>()
  return {
    ...actual,
    getSetting: (id: string) => stubs.settingsMap[id] ?? '',
    setSetting: (id: string, value: unknown) => {
      stubs.settingsMap[id] = value
    },
    onSpecificSettingChange: () => () => {},
  }
})

vi.mock('$lib/tauri-commands', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  checkAiConnection: vi.fn(() => Promise.resolve({ connected: false, authError: false, models: [], error: null })),
  saveAiApiKey: vi.fn(() => Promise.resolve(null)),
  getAiApiKeyStatus: vi.fn(() => Promise.resolve({ isSet: false, fingerprint: '' })),
  openExternalUrl: vi.fn(() => Promise.resolve()),
}))

import ProviderSetupSteps from './ProviderSetupSteps.svelte'
import { ProviderSetupController } from './provider-setup.svelte'

let mounted: { target: HTMLElement; instance: ReturnType<typeof mount> } | undefined
let controller: ProviderSetupController | undefined

async function settle(): Promise<void> {
  for (let i = 0; i < 30; i++) {
    await Promise.resolve()
  }
  await tick()
  flushSync()
}

async function mountFor(providerId: string): Promise<HTMLElement> {
  const target = document.createElement('div')
  document.body.appendChild(target)
  controller = new ProviderSetupController({ logScope: 'a11y-test' })
  controller.setProvider(providerId)
  const instance = mount(ProviderSetupSteps, { target, props: { controller, idPrefix: 'a11y' } })
  mounted = { target, instance }
  await settle()
  return target
}

afterEach(async () => {
  controller?.destroy()
  controller = undefined
  if (mounted) {
    await unmount(mounted.instance)
    mounted.target.remove()
    mounted = undefined
  }
})

describe('ProviderSetupSteps a11y', () => {
  beforeEach(() => {
    stubs.settingsMap['ai.cloudProviderConfigs'] = '{}'
  })

  it('a cloud provider has no a11y violations', async () => {
    await expectNoA11yViolations(await mountFor('openai'))
  })

  it('a provider with an editable endpoint has no a11y violations', async () => {
    await expectNoA11yViolations(await mountFor('azure-openai'))
  })

  it('a local provider (no API-key step) has no a11y violations', async () => {
    await expectNoA11yViolations(await mountFor('ollama'))
  })
})
