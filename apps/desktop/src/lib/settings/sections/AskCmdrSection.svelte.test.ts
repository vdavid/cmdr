/**
 * Tier-3 tests for `AskCmdrSection.svelte`: the on/off switch and its "cloud AI is off" hint,
 * the chat memory size row, and what the section says when a memory wipe stops partway.
 *
 * Pins what the user can actually do and see: the presets are all there with Automatic
 * first, and a size larger than the window Cmdr believes the model has WARNS while keeping
 * the value. Cmdr never overrules the choice — what it knows about a model can be out of
 * date, and the user may be right about their own model.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, tick } from 'svelte'

const settings: Record<string, unknown> = {
  'ai.provider': 'cloud',
  'askCmdr.enabled': true,
  'askCmdr.interactiveModel': '',
  'askCmdr.chatMemorySize': 'auto',
}

vi.mock('$lib/settings/settings-store', () => ({
  getSetting: vi.fn((key: string) => settings[key]),
  setSetting: vi.fn((key: string, value: unknown) => {
    settings[key] = value
    return Promise.resolve()
  }),
  resetSetting: vi.fn(),
  isModified: vi.fn(() => false),
  onSpecificSettingChange: vi.fn(() => () => {}),
  onSettingChange: vi.fn(() => () => {}),
}))

const { cloudConsent } = vi.hoisted(() => ({ cloudConsent: { accepted: true } }))
vi.mock('$lib/ai/cloud-consent.svelte', () => ({
  cloudConsentState: cloudConsent,
  refreshCloudConsent: vi.fn(() => Promise.resolve()),
  cloudAiBlocked: (provider: string) => provider === 'cloud' && !cloudConsent.accepted,
  openCloudConsentSettings: vi.fn(),
}))

const { aiPolicy } = vi.hoisted(() => ({ aiPolicy: { mode: 'allowed' } }))
vi.mock('$lib/managed-policy/managed-policy.svelte', async (importOriginal) => {
  const actual = await importOriginal<typeof import('$lib/managed-policy/managed-policy.svelte')>()
  return {
    ...actual,
    getManagedPolicyView: () => ({ ...actual.UNMANAGED, ai: { mode: aiPolicy.mode, allowedCloudHosts: null } }),
  }
})

const { modelWindow } = vi.hoisted(() => ({
  modelWindow: { model: 'gpt-4o', knownWindowTokens: null as number | null },
}))
vi.mock('$lib/tauri-commands', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  askCmdrCostSummary: vi.fn(() => Promise.resolve({ days: [] })),
  askCmdrModelWindow: vi.fn(() => Promise.resolve(modelWindow)),
  askCmdrForgetMemory: vi.fn(() => Promise.resolve(3)),
  // The forget confirmation is a `ModalDialog`, which reports itself open and closed.
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
}))

import AskCmdrSection from './AskCmdrSection.svelte'
import { openCloudConsentSettings } from '$lib/ai/cloud-consent.svelte'
import { askCmdrForgetMemory } from '$lib/tauri-commands'

/** Lets a click's awaited IPC settle and the section re-render. */
async function settle(): Promise<void> {
  for (let i = 0; i < 5; i++) await Promise.resolve()
  await tick()
}

async function mountSection(): Promise<HTMLElement> {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(AskCmdrSection, { target, props: { searchQuery: '' } })
  // Two ticks: the model-window read is a promise the warning depends on.
  await tick()
  await Promise.resolve()
  await tick()
  return target
}

function warningText(target: HTMLElement): string | null {
  return target.querySelector('.memory-warning')?.textContent.trim() ?? null
}

describe('AskCmdrSection chat memory size', () => {
  beforeEach(() => {
    settings['askCmdr.chatMemorySize'] = 'auto'
    modelWindow.model = 'gpt-4o'
    modelWindow.knownWindowTokens = null
  })

  it('renders the row, showing the current choice by name', async () => {
    // The row exists at all (a registry entry alone renders nothing), and the closed picker
    // reads as the choice rather than as a raw stored value. The preset list itself is pinned
    // in `settings-registry.test.ts`: Ark UI renders its items only once opened.
    const target = await mountSection()
    const labelFors = Array.from(target.querySelectorAll('label.setting-label')).map((el) => el.getAttribute('for'))
    expect(labelFors).toContain('askCmdr.chatMemorySize')
    expect(target.textContent).toContain('Automatic (recommended)')
    target.remove()
  })

  it('shows a chosen preset as a grouped number, so 200000 never reads as 20,000', async () => {
    settings['askCmdr.chatMemorySize'] = '200000'
    const target = await mountSection()
    expect(target.textContent).toContain('200,000')
    target.remove()
  })

  it('says nothing while the size fits the window Cmdr knows about', async () => {
    settings['askCmdr.chatMemorySize'] = '60000'
    modelWindow.knownWindowTokens = 128_000
    const target = await mountSection()
    expect(warningText(target)).toBeNull()
    target.remove()
  })

  it('warns, without overruling, when the size is larger than that window', async () => {
    settings['askCmdr.chatMemorySize'] = '200000'
    modelWindow.knownWindowTokens = 128_000
    const target = await mountSection()
    expect(warningText(target)).toBe('Your model may refuse a message this long. Cmdr keeps the value you set.')
    target.remove()
  })

  it('stays quiet when nothing knows the window: an unknown model is not a warning', async () => {
    settings['askCmdr.chatMemorySize'] = '200000'
    modelWindow.model = 'some-future-model-9000'
    modelWindow.knownWindowTokens = null
    const target = await mountSection()
    expect(warningText(target)).toBeNull()
    target.remove()
  })

  it('never warns on Automatic: it follows the window by construction', async () => {
    settings['askCmdr.chatMemorySize'] = 'auto'
    modelWindow.knownWindowTokens = 16_384
    const target = await mountSection()
    expect(warningText(target)).toBeNull()
    target.remove()
  })
})

describe('AskCmdrSection on/off', () => {
  beforeEach(() => {
    settings['ai.provider'] = 'cloud'
    settings['askCmdr.enabled'] = true
    cloudConsent.accepted = true
  })

  it('is a plain switch bound to askCmdr.enabled', async () => {
    const target = await mountSection()
    const labelFors = Array.from(target.querySelectorAll('label.setting-label')).map((el) => el.getAttribute('for'))
    expect(labelFors).toContain('askCmdr.enabled')
    expect(target.querySelector('label.setting-label[for="askCmdr.enabled"]')?.textContent.trim()).toBe('Ask Cmdr')
    target.remove()
  })

  it('says cloud AI is off when Ask Cmdr is on over Cloud without consent, and links to the switch', async () => {
    cloudConsent.accepted = false
    const target = await mountSection()

    expect(target.querySelector('.cloud-off-hint')?.textContent).toContain(
      'Ask Cmdr uses your cloud AI service. Allow cloud AI in AI settings to start chatting.',
    )
    target.querySelector<HTMLButtonElement>('.cloud-off-hint button')?.click()
    expect(openCloudConsentSettings).toHaveBeenCalledWith('ask-cmdr-settings-hint')
    target.remove()
  })

  it("says the organization turned AI off, rather than sending the person to a setting they can't change", async () => {
    aiPolicy.mode = 'off'
    settings['ai.provider'] = 'off'
    try {
      const target = await mountSection()
      const hints = Array.from(target.querySelectorAll('.provider-hint')).map((el) => el.textContent.trim())
      expect(hints).toContain('Your organization turned off AI in Cmdr.')
      expect(hints).not.toContain('Turn on an AI provider in Settings › AI to start chatting.')
      target.remove()
    } finally {
      aiPolicy.mode = 'allowed'
    }
  })

  it('stays quiet when cloud AI is allowed, on Local, or with Ask Cmdr off', async () => {
    for (const [provider, enabled, accepted] of [
      ['cloud', true, true],
      ['local', true, false],
      ['cloud', false, false],
    ] as const) {
      settings['ai.provider'] = provider
      settings['askCmdr.enabled'] = enabled
      cloudConsent.accepted = accepted
      const target = await mountSection()
      expect(target.querySelector('.cloud-off-hint'), `${provider} ${String(enabled)}`).toBeNull()
      target.remove()
    }
  })
})

describe('AskCmdrSection when the store says no', () => {
  it('points at the memory folder when forgetting stopped partway, rather than closing without a word', async () => {
    vi.mocked(askCmdrForgetMemory).mockRejectedValueOnce(new Error('unwritable'))
    const target = await mountSection()

    const forgetButton = target.querySelectorAll<HTMLButtonElement>('.memory-actions button')[1]
    forgetButton.click()
    await settle()
    const dialogButtons = document.querySelectorAll<HTMLButtonElement>('[data-dialog-id="forget-memory"] button')
    dialogButtons[dialogButtons.length - 1].click()
    await settle()

    expect(target.querySelector('.memory-not-forgotten')?.textContent.trim()).toBe(
      'Cmdr couldn’t delete every note. Open the memory folder to remove the rest.',
    )
    expect(target.querySelector('.memory-forgotten')).toBeNull()
    target.remove()
  })
})
