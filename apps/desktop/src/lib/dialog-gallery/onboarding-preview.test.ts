/**
 * The onboarding preview's two guarantees: it opens the wizard through the
 * app's OWN command (never a private opener the gallery invents), and it moves
 * the step only after that command resolved — the opener loads settings and
 * probes for Full Disk Access first, and `openWizard()` would overwrite an
 * early jump.
 */

import { describe, expect, it, vi } from 'vitest'
import { getOnboardingState, resetForTesting, setCurrentStep } from '$lib/onboarding/onboarding-state.svelte'
import { openOnboardingPreview } from './onboarding-preview'
import { onboardingFixtures } from './fixtures/onboarding'
import { isGalleryDialogOpen, openGalleryDialog } from './gallery-state.svelte'

type Dispatch = Parameters<typeof openOnboardingPreview>[1]

/**
 * Stands in for the command bus: resolves a microtask later (the real handler
 * awaits a settings load and an FDA probe) and only then is the wizard open, at
 * the first reachable step.
 */
function fakeDispatch(): Dispatch & { calls: string[] } {
  const calls: string[] = []
  const dispatch = vi.fn(async (commandId: string) => {
    calls.push(commandId)
    await Promise.resolve()
    if (commandId === 'cmdr.openOnboarding') setCurrentStep(1)
  })
  return Object.assign(dispatch as unknown as Dispatch, { calls })
}

describe('openOnboardingPreview', () => {
  it('dispatches the app’s onboarding command, then lands on the requested step', async () => {
    resetForTesting()
    const dispatch = fakeDispatch()
    await openOnboardingPreview('step-3-beta', dispatch)

    expect(dispatch.calls).toEqual(['cmdr.openOnboarding'])
    expect(getOnboardingState().currentStep).toBe(3)
  })

  it('covers every step the gallery advertises', async () => {
    for (const [stateId, step] of Object.entries(onboardingFixtures)) {
      resetForTesting()
      await openOnboardingPreview(stateId, fakeDispatch())
      expect(getOnboardingState().currentStep, stateId).toBe(step)
    }
  })

  it('drops whatever the gallery was previewing, so nothing hides behind the wizard', async () => {
    // The wizard is a full-window overlay the gallery doesn't own; a preview left
    // open underneath reappears the moment the wizard closes.
    resetForTesting()
    openGalleryDialog('alert', 'short')
    await openOnboardingPreview('step-2-ai', fakeDispatch())

    expect(isGalleryDialogOpen()).toBe(false)
    expect(getOnboardingState().currentStep).toBe(2)
  })

  it('does nothing for a state id with no step, rather than opening a wizard it can’t place', async () => {
    resetForTesting()
    const dispatch = fakeDispatch()
    await openOnboardingPreview('no-such-state', dispatch)

    expect(dispatch.calls).toEqual([])
    expect(getOnboardingState().currentStep).toBeNull()
  })
})
