/**
 * Tier 3 a11y tests for `AskCmdrConsent.svelte`, the opt-in gate.
 *
 * The screen: a labelled group (heading + intro + the "what leaves your Mac" list + the
 * read-only reassurance + the local-storage note), and the two actions (Not now / Turn on).
 * The consent + trigger modules are mocked so it mounts without a backend.
 */

import { describe, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

vi.mock('./ask-cmdr-consent.svelte', () => ({
  acceptConsent: vi.fn(() => Promise.resolve(true)),
}))
vi.mock('./ask-cmdr-trigger.svelte', () => ({
  closeRail: vi.fn(),
  openRail: vi.fn(() => Promise.resolve()),
}))

import AskCmdrConsent from './AskCmdrConsent.svelte'

describe('AskCmdrConsent a11y', () => {
  it('the opt-in gate has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(AskCmdrConsent, { target, props: {} })
    await tick()
    await expectNoA11yViolations(target)
    target.remove()
  })
})
