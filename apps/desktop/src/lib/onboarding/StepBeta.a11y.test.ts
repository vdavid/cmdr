/**
 * Tier 3 axe a11y tests for `StepBeta.svelte` (the "Open beta" onboarding page).
 *
 * States: the opt-out switch on (default) and off, plus the required terms checkbox
 * unticked and ticked. The switch is labelled by its registry definition; the email input
 * carries its own aria-label; the terms checkbox is named by its inline label (which holds
 * a link) and marked `aria-required`. Axe runs in jsdom (no contrast checks; we cover those
 * in tier-1 scripts).
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount, tick, unmount, flushSync } from 'svelte'
import StepBeta from './StepBeta.svelte'
import { closeWizard, resetForTesting, openWizard, setCurrentStep } from './onboarding-state.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'
import { TERMS_VERSION } from '$lib/legal/terms'

const { storedSettings } = vi.hoisted(() => ({
  storedSettings: { termsAcceptedVersion: null as string | null, termsAcceptedAt: null as string | null },
}))

vi.mock('$lib/tauri-commands', async () => {
  const real = await vi.importActual<typeof import('$lib/tauri-commands')>('$lib/tauri-commands')
  return { ...real, betaSignup: vi.fn(() => Promise.resolve({ kind: 'subscribed' as const })) }
})

vi.mock('$lib/settings-store', async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>()
  return {
    ...actual,
    loadSettings: () => Promise.resolve({ ...storedSettings }),
    saveSettings: () => Promise.resolve(true),
  }
})

const settingsMap: Record<string, unknown> = {
  'analytics.enabled': true,
  'analytics.email': '',
}

vi.mock('$lib/settings', async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>()
  return {
    ...actual,
    getSetting: (id: string) => settingsMap[id],
    setSetting: (id: string, value: unknown) => {
      settingsMap[id] = value
    },
  }
})

vi.mock('$lib/settings/settings-store', () => ({
  onSpecificSettingChange: () => () => {},
}))

let mounted: { target: HTMLElement; instance: ReturnType<typeof mount> } | undefined

async function settle(): Promise<void> {
  for (let i = 0; i < 10; i++) {
    await Promise.resolve()
  }
  await tick()
  flushSync()
}

function mountStep(): HTMLElement {
  const target = document.createElement('div')
  document.body.appendChild(target)
  const instance = mount(StepBeta, { target, props: {} })
  mounted = { target, instance }
  return target
}

beforeEach(() => {
  settingsMap['analytics.enabled'] = true
  settingsMap['analytics.email'] = ''
  storedSettings.termsAcceptedVersion = null
  storedSettings.termsAcceptedAt = null
  closeWizard()
  resetForTesting()
  openWizard('force')
  setCurrentStep(3)
})

afterEach(async () => {
  if (mounted) {
    await unmount(mounted.instance)
    mounted.target.remove()
    mounted = undefined
  }
  closeWizard()
  resetForTesting()
})

describe('StepBeta a11y', () => {
  it('default state (opt-out on) has no a11y violations', async () => {
    const target = mountStep()
    await settle()
    await expectNoA11yViolations(target)
  })

  it('opted-out state (switch off) has no a11y violations', async () => {
    settingsMap['analytics.enabled'] = false
    const target = mountStep()
    await settle()
    await expectNoA11yViolations(target)
  })

  it('the required terms checkbox is named and marked required, with no a11y violations', async () => {
    const target = mountStep()
    await settle()

    const checkbox = target.querySelector<HTMLInputElement>('.terms-block input[type="checkbox"]')
    expect(checkbox?.getAttribute('aria-required')).toBe('true')
    // Named by the inline consent label, so a screen reader reads the sentence being agreed to.
    expect(checkbox?.getAttribute('aria-labelledby')).toBeTruthy()
    await expectNoA11yViolations(target)
  })

  it('accepted-terms state has no a11y violations', async () => {
    storedSettings.termsAcceptedVersion = TERMS_VERSION
    storedSettings.termsAcceptedAt = '2026-08-10T09:00:00.000Z'
    const target = mountStep()
    await settle()
    await expectNoA11yViolations(target)
  })
})
