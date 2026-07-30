/**
 * Tier 3 a11y tests for `AskCmdrSection.svelte`.
 *
 * The section: the enable/consent toggle, the "what Ask Cmdr sends" disclosure, the
 * provider hint, the interactive-model and chat-memory-size rows (including the over-window
 * warning), and the spend rollup. The settings store and
 * the consent + cost commands are mocked so it mounts without a backend; the consent state
 * is driven directly (off, then on) to cover both toggle labels.
 */

import { describe, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

vi.mock('$lib/settings/settings-store', () => ({
  getSetting: vi.fn((key: string) => {
    if (key === 'ai.provider') return 'cloud'
    if (key === 'askCmdr.interactiveModel') return ''
    if (key === 'askCmdr.chatMemorySize') return '200000'
    return undefined
  }),
  setSetting: vi.fn(() => Promise.resolve()),
  resetSetting: vi.fn(),
  isModified: vi.fn(() => false),
  onSpecificSettingChange: vi.fn(() => () => {}),
  onSettingChange: vi.fn(() => () => {}),
}))

const { consentState } = vi.hoisted(() => ({
  consentState: { accepted: false, acceptedAt: null as number | null },
}))
vi.mock('$lib/ask-cmdr/ask-cmdr-consent.svelte', () => ({
  consentState,
  refreshConsent: vi.fn(() => Promise.resolve()),
  acceptConsent: vi.fn(() => Promise.resolve(true)),
  revokeConsent: vi.fn(() => Promise.resolve()),
}))
vi.mock('$lib/tauri-commands', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  askCmdrCostSummary: vi.fn(() => Promise.resolve({ days: [] })),
  // A 200,000-token pick against a 128,000-token window, so the over-window warning is part
  // of what axe sees.
  askCmdrModelWindow: vi.fn(() => Promise.resolve({ model: 'gpt-4o', knownWindowTokens: 128_000 })),
}))

import AskCmdrSection from './AskCmdrSection.svelte'

function mountSection(): HTMLElement {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(AskCmdrSection, { target, props: { searchQuery: '' } })
  return target
}

describe('AskCmdrSection a11y', () => {
  it('has no a11y violations when Ask Cmdr is off', async () => {
    consentState.accepted = false
    consentState.acceptedAt = null
    const target = mountSection()
    await tick()
    await expectNoA11yViolations(target)
    target.remove()
  })

  it('has no a11y violations when Ask Cmdr is on', async () => {
    consentState.accepted = true
    consentState.acceptedAt = 1_760_000_000
    const target = mountSection()
    await tick()
    await expectNoA11yViolations(target)
    target.remove()
  })
})
