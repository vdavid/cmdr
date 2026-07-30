/**
 * Tier-3 tests for `AskCmdrSection.svelte`, the chat memory size row.
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

vi.mock('$lib/ask-cmdr/ask-cmdr-consent.svelte', () => ({
  consentState: { accepted: true, acceptedAt: 1_760_000_000 },
  refreshConsent: vi.fn(() => Promise.resolve()),
  acceptConsent: vi.fn(() => Promise.resolve(true)),
  revokeConsent: vi.fn(() => Promise.resolve()),
}))

const { modelWindow } = vi.hoisted(() => ({
  modelWindow: { model: 'gpt-4o', knownWindowTokens: null as number | null },
}))
vi.mock('$lib/tauri-commands', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  askCmdrCostSummary: vi.fn(() => Promise.resolve({ days: [] })),
  askCmdrModelWindow: vi.fn(() => Promise.resolve(modelWindow)),
}))

import AskCmdrSection from './AskCmdrSection.svelte'

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
