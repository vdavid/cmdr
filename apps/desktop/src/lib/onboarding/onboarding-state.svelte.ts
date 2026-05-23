/**
 * Onboarding wizard state machine.
 *
 * M1 scope: minimal step machine (1/2/3) + a "source" tag for telemetry/debugging. M2+
 * adds the persistence-aware resume rule (`resumeStepFor(...)`) plus per-step decision
 * persistence. Today the wizard is only reachable via `CMDR_FORCE_ONBOARDING=1`, so the
 * full flag-driven resume logic isn't needed yet.
 */

/** Where the wizard was opened from. M1 only ever sees `'force'`. */
export type OnboardingSource = 'force' | 'first-launch' | 'menu' | 'palette'

/** Step index (1-based to match the visible dot indicator). */
export type OnboardingStep = 1 | 2 | 3

/** Number of steps. Kept as a constant so the dot indicator and bounds checks agree. */
export const ONBOARDING_STEP_COUNT = 3 as const

interface OnboardingStateData {
  /** `null` when the wizard is closed; an integer step when open. */
  currentStep: OnboardingStep | null
  /** What opened the wizard. `null` when closed. */
  source: OnboardingSource | null
}

const state = $state<OnboardingStateData>({
  currentStep: null,
  source: null,
})

export function getOnboardingState(): Readonly<OnboardingStateData> {
  return state
}

/**
 * Open the wizard at step 1 (M1). M2+ will plug in `resumeStepFor(settings, hasFda)`
 * to pick the right resume step.
 */
export function openWizard(source: OnboardingSource): void {
  state.source = source
  state.currentStep = 1
}

export function closeWizard(): void {
  state.currentStep = null
  state.source = null
}

/** Advance to the next step. No-op if already on the last step. */
export function nextStep(): void {
  if (state.currentStep === null) return
  if (state.currentStep < ONBOARDING_STEP_COUNT) {
    state.currentStep = (state.currentStep + 1) as OnboardingStep
  }
}

/** Go back one step. No-op if already on the first step. */
export function previousStep(): void {
  if (state.currentStep === null) return
  if (state.currentStep > 1) {
    state.currentStep = (state.currentStep - 1) as OnboardingStep
  }
}

/** True when on the first reachable step (Back should be disabled). */
export function isAtFirstStep(): boolean {
  return state.currentStep === 1
}

/** True when on the last step (Next should read "Finish" / submit instead). */
export function isAtLastStep(): boolean {
  return state.currentStep === ONBOARDING_STEP_COUNT
}

/** Reset to closed. For use in tests only. */
export function resetForTesting(): void {
  state.currentStep = null
  state.source = null
}
