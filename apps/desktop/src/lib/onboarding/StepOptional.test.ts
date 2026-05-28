/**
 * Behaviour tests for `StepOptional.svelte`.
 *
 * Covers:
 * - Each of the four toggles round-trips: clicking the switch writes the new value
 *   to settings via the mocked `setSetting`.
 * - Switching `network.enabled` off (the most-likely "I'd like to opt out" case) is
 *   surfaced through the same path.
 * - The footer override registers a single primary "Start using Cmdr" button; clicking
 *   it bumps the wizard's `finishRequestTick` so the wizard shell fires `onComplete`.
 * - `setFooterOverride(null)` runs on destroy so a subsequent step doesn't see leaked
 *   buttons.
 *
 * Axe coverage lives in `StepOptional.a11y.test.ts`.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount, tick, unmount, flushSync } from 'svelte'
import StepOptional from './StepOptional.svelte'
import { closeWizard, resetForTesting, openWizard, setCurrentStep, getOnboardingState } from './onboarding-state.svelte'

// In-memory settings store. `setSetting` mutates the map AND records the call so we
// can assert which IDs got written.
const settingsMap: Record<string, unknown> = {
  'network.enabled': true,
  'indexing.enabled': true,
  'updates.autoCheck': true,
  'fileOperations.mtpEnabled': true,
}
const setSetting = vi.fn((id: string, value: unknown) => {
  settingsMap[id] = value
})

vi.mock('$lib/settings', async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>()
  return {
    ...actual,
    getSetting: (id: string) => settingsMap[id],
    setSetting: (id: string, value: unknown) => {
      setSetting(id, value)
    },
    onSpecificSettingChange: () => () => {},
  }
})

function mountStep(): { target: HTMLElement; instance: ReturnType<typeof mount> } {
  const target = document.createElement('div')
  document.body.appendChild(target)
  const instance = mount(StepOptional, { target, props: {} })
  return { target, instance }
}

async function waitForAsync(): Promise<void> {
  for (let i = 0; i < 10; i++) {
    await Promise.resolve()
  }
  await tick()
  flushSync()
}

function findSwitchControlByLabel(target: HTMLElement, label: string): HTMLElement | null {
  return target.querySelector<HTMLElement>(`[aria-label="${label}"]`)
}

describe('StepOptional', () => {
  let mounted: ReturnType<typeof mountStep> | undefined

  beforeEach(() => {
    settingsMap['network.enabled'] = true
    settingsMap['indexing.enabled'] = true
    settingsMap['updates.autoCheck'] = true
    settingsMap['fileOperations.mtpEnabled'] = true
    setSetting.mockClear()
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

  it('registers a single primary "Start using Cmdr" footer button', async () => {
    mounted = mountStep()
    await waitForAsync()

    const state = getOnboardingState()
    expect(state.footerOverride).not.toBeNull()
    expect(state.footerOverride).toHaveLength(1)
    expect(state.footerOverride?.[0].label).toBe('Start using Cmdr')
    expect(state.footerOverride?.[0].variant).toBe('primary')
  })

  it('"Start using Cmdr" click bumps finishRequestTick (asks wizard to complete)', async () => {
    mounted = mountStep()
    await waitForAsync()

    const state = getOnboardingState()
    const tickBefore = state.finishRequestTick
    state.footerOverride?.[0].onclick()
    await waitForAsync()

    expect(state.finishRequestTick).toBe(tickBefore + 1)
  })

  it('clears the footer override on destroy', async () => {
    mounted = mountStep()
    await waitForAsync()
    expect(getOnboardingState().footerOverride).not.toBeNull()

    await unmount(mounted.instance)
    mounted.target.remove()
    mounted = undefined
    await waitForAsync()

    expect(getOnboardingState().footerOverride).toBeNull()
  })

  it('renders all four toggles labelled by their registry definitions', async () => {
    mounted = mountStep()
    await waitForAsync()

    // Labels come from settings-registry.ts. The switch's aria-label mirrors the
    // setting's `label` field.
    expect(findSwitchControlByLabel(mounted.target, 'Enable networking')).not.toBeNull()
    expect(findSwitchControlByLabel(mounted.target, 'Drive indexing')).not.toBeNull()
    expect(findSwitchControlByLabel(mounted.target, 'Automatically check for updates')).not.toBeNull()
    expect(findSwitchControlByLabel(mounted.target, 'Android/Kindle/camera support (PTP and MTP)')).not.toBeNull()
  })

  it.each([
    ['Enable networking', 'network.enabled'],
    ['Drive indexing', 'indexing.enabled'],
    ['Automatically check for updates', 'updates.autoCheck'],
    ['Android/Kindle/camera support (PTP and MTP)', 'fileOperations.mtpEnabled'],
  ])('clicking the %s switch writes %s to settings', async (label, settingId) => {
    mounted = mountStep()
    await waitForAsync()
    setSetting.mockClear()

    const control = findSwitchControlByLabel(mounted.target, label)
    expect(control).not.toBeNull()
    control?.click()
    await waitForAsync()

    expect(setSetting).toHaveBeenCalledWith(settingId, false)
  })
})
