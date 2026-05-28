/**
 * Tier 3 axe a11y tests for `StepOptional.svelte`.
 *
 * Two states: default (all four toggles on) and one-off (networking off). Each switch
 * is labelled by its registry definition; the section heading gives the question
 * context. Axe runs in jsdom (no contrast checks; we cover those in tier-1 scripts).
 */

import { describe, it, vi, beforeEach, afterEach } from 'vitest'
import { mount, tick, unmount, flushSync } from 'svelte'
import StepOptional from './StepOptional.svelte'
import { closeWizard, resetForTesting, openWizard, setCurrentStep } from './onboarding-state.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

const settingsMap: Record<string, unknown> = {
  'network.enabled': true,
  'indexing.enabled': true,
  'updates.autoCheck': true,
  'fileOperations.mtpEnabled': true,
}

vi.mock('$lib/settings', async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>()
  return {
    ...actual,
    getSetting: (id: string) => settingsMap[id],
    setSetting: (id: string, value: unknown) => {
      settingsMap[id] = value
    },
    onSpecificSettingChange: () => () => {},
  }
})

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
  const instance = mount(StepOptional, { target, props: {} })
  mounted = { target, instance }
  return target
}

beforeEach(() => {
  settingsMap['network.enabled'] = true
  settingsMap['indexing.enabled'] = true
  settingsMap['updates.autoCheck'] = true
  settingsMap['fileOperations.mtpEnabled'] = true
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

describe('StepOptional a11y', () => {
  it('default state (all toggles on) has no a11y violations', async () => {
    const target = mountStep()
    await settle()
    await expectNoA11yViolations(target)
  })

  it('one-off state (networking off) has no a11y violations', async () => {
    settingsMap['network.enabled'] = false
    const target = mountStep()
    await settle()
    await expectNoA11yViolations(target)
  })
})
