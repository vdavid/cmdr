/**
 * The "What Cmdr sends" fold follows the Allow cloud AI switch: open while it's off (people
 * read it while deciding), folded once they turn it on (so the service setup below stays in
 * view, in onboarding especially), and still theirs to reopen.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, tick } from 'svelte'

const { consentState } = vi.hoisted(() => {
  const consentState: { accepted: boolean | null; acceptedAt: number | null; managed: 'cloudAiOff' | null } = {
    accepted: false,
    acceptedAt: null,
    managed: null,
  }
  return { consentState }
})
vi.mock('./cloud-consent.svelte', () => ({
  cloudConsentState: consentState,
  refreshCloudConsent: vi.fn(() => Promise.resolve()),
  acceptCloudConsent: vi.fn(() => {
    consentState.accepted = true
    return Promise.resolve('done')
  }),
  declineCloudConsent: vi.fn(() => {
    consentState.accepted = false
    return Promise.resolve('done')
  }),
  CLOUD_CONSENT_ANCHOR: 'settings-ai-cloud-consent',
}))

import AiCloudConsentToggle from './AiCloudConsentToggle.svelte'

async function settle(): Promise<void> {
  for (let i = 0; i < 5; i++) await tick()
}

async function mountToggle(): Promise<{
  target: HTMLElement
  details: HTMLDetailsElement
  toggle: () => Promise<void>
}> {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(AiCloudConsentToggle, { target, props: {} })
  await settle()
  const details = target.querySelector('details')
  const input = target.querySelector<HTMLInputElement>('[data-test="cloud-ai-consent"]')
  if (!details || !input) throw new Error('toggle did not render')
  return {
    target,
    details,
    toggle: async () => {
      input.click()
      await settle()
    },
  }
}

/** What a click on the summary does: the browser flips `open` and fires `toggle`. */
async function userToggles(details: HTMLDetailsElement): Promise<void> {
  details.open = !details.open
  details.dispatchEvent(new Event('toggle'))
  await settle()
}

beforeEach(() => {
  consentState.accepted = false
  consentState.acceptedAt = null
  consentState.managed = null
})

describe('the "What Cmdr sends" fold', () => {
  it('stays open while the switch is off', async () => {
    const { target, details } = await mountToggle()
    expect(details.open).toBe(true)
    target.remove()
  })

  it('folds when the user turns the switch on', async () => {
    const { target, details, toggle } = await mountToggle()
    await toggle()
    expect(consentState.accepted).toBe(true)
    expect(details.open).toBe(false)
    target.remove()
  })

  it('lets the user reopen it once the switch is on', async () => {
    const { target, details, toggle } = await mountToggle()
    await toggle()
    await userToggles(details)
    expect(details.open).toBe(true)
    target.remove()
  })

  it('opens again when the user turns the switch back off', async () => {
    const { target, details, toggle } = await mountToggle()
    await toggle()
    await toggle()
    expect(consentState.accepted).toBe(false)
    expect(details.open).toBe(true)
    target.remove()
  })
})

describe("the switch under the organization's policy", () => {
  it('locks off with the reason in view when the organization rules out every cloud host', async () => {
    consentState.managed = 'cloudAiOff'
    const { target } = await mountToggle()
    const input = target.querySelector<HTMLInputElement>('[data-test="cloud-ai-consent"]')
    expect(input?.disabled).toBe(true)
    const note = target.querySelector('.consent-managed')
    expect(note?.textContent).toBe('Your organization allows only on-device AI.')
    expect(input?.getAttribute('aria-describedby')).toBe(note?.id)
    target.remove()
  })
})
