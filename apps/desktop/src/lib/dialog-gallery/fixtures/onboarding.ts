/**
 * Which wizard step each `onboarding` gallery state lands on.
 *
 * There's no fixture DATA here on purpose: the wizard renders this machine's
 * real onboarding context (the FDA probe, the persisted choice, the AI and beta
 * settings), and the gallery opens it through the app's own re-entry command
 * rather than faking any of that. The step is the only thing a preview picks,
 * which is what makes every page reviewable: on macOS step 1 refuses to advance
 * until the user commits to Allow or Deny for real.
 */

import type { OnboardingStep } from '$lib/onboarding/onboarding-state.svelte'

export const onboardingFixtures: Record<string, OnboardingStep | undefined> = {
  'step-1-fda': 1,
  'step-2-ai': 2,
  'step-3-beta': 3,
  'step-4-optional': 4,
}
