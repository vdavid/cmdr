/**
 * Tier 3 axe-based a11y tests for the M1 onboarding wizard skeleton.
 *
 * The wizard renders three stub steps in M1 (each populated in M2/M3/M4). This file
 * asserts axe-clean structure on the default state plus each step variant. Focus
 * trap + Escape-swallowing behaviour live in `OnboardingWizard.test.ts`.
 */

import { describe, it, vi, afterEach } from 'vitest'
import { mount, tick, unmount, flushSync } from 'svelte'
import OnboardingWizard from './OnboardingWizard.svelte'
import { closeWizard, resetForTesting } from './onboarding-state.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

vi.mock('$lib/tauri-commands', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
}))

let lastInstance: ReturnType<typeof mount> | undefined
let lastTarget: HTMLDivElement | undefined

function mountAt(): HTMLDivElement {
  const target = document.createElement('div')
  document.body.appendChild(target)
  lastTarget = target
  lastInstance = mount(OnboardingWizard, { target, props: { onComplete: () => {} } })
  return target
}

afterEach(() => {
  if (lastInstance) {
    unmount(lastInstance)
    lastInstance = undefined
  }
  if (lastTarget) {
    lastTarget.remove()
    lastTarget = undefined
  }
  closeWizard()
  resetForTesting()
})

describe('OnboardingWizard a11y', () => {
  it('step 1 default state has no a11y violations', async () => {
    const target = mountAt()
    await tick()
    await expectNoA11yViolations(target)
  })

  it('step 2 has no a11y violations', async () => {
    const target = mountAt()
    await tick()
    const next = target.querySelector<HTMLButtonElement>('button.btn-primary')
    next?.click()
    flushSync()
    await tick()
    await expectNoA11yViolations(target)
  })

  it('step 3 has no a11y violations', async () => {
    const target = mountAt()
    await tick()
    const next = target.querySelector<HTMLButtonElement>('button.btn-primary')
    next?.click()
    flushSync()
    await tick()
    next?.click()
    flushSync()
    await tick()
    await expectNoA11yViolations(target)
  })
})
