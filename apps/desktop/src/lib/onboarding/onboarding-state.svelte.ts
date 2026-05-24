/**
 * Onboarding wizard state machine.
 *
 * M2 scope: step 1 (FDA) is real. The wizard now needs three pieces of state beyond
 * the bare step cursor:
 *
 * 1. A persisted-flag-aware resume step (`resumeStepFor()`). Production launches read
 *    `fullDiskAccessChoice` + `isOnboarded` + an FDA probe and land the user on the
 *    first not-yet-decided step. Linux skips step 1 entirely (no FDA on Linux).
 * 2. A step-1 variant tag (first-ask / revoked / already-granted) — the body switches
 *    copy + buttons by variant. `'already-granted'` is a single-Next variant for the
 *    menu re-entry case.
 * 3. A step-1 footer mode (`'decide' | 'restart'`). The "Allow" path requires the
 *    user to restart the app before advancing (see plan § "FDA gate clear-on-Allow"):
 *    after clicking Allow, the wizard swaps the primary footer button from "Open
 *    System Settings" to "Restart Cmdr".
 *
 * Step 2 reads the same flag triple via `stepTwoFdaBanner()` so the M3 banner can pick
 * the right copy. Even though M2 doesn't render step 2 content, we set this up here so
 * the M3 work just slots into an existing field.
 */

import { isMacOS } from '$lib/shortcuts/key-capture'
import type { FullDiskAccessChoice } from '$lib/settings-store'

/** Where the wizard was opened from. */
export type OnboardingSource = 'force' | 'first-launch' | 'menu' | 'palette'

/** Step index (1-based to match the visible dot indicator). */
export type OnboardingStep = 1 | 2 | 3

/** Number of steps. Kept as a constant so the dot indicator and bounds checks agree. */
export const ONBOARDING_STEP_COUNT = 3 as const

/**
 * Step 1 variant.
 *
 * - `'first-ask'`: user has never decided. Welcome + pros/cons + Allow/Deny.
 * - `'revoked'`: user accepted before, then later revoked FDA in System Settings.
 *   Different opening paragraph; same Allow/Deny choices.
 * - `'already-granted'`: re-entry from the menu / palette while FDA is currently
 *   granted. Single-Next variant ("Cmdr currently has Full Disk Access. You can
 *   revoke any time in System Settings.").
 */
export type Step1Variant = 'first-ask' | 'revoked' | 'already-granted'

/**
 * Step 1 footer mode.
 *
 * - `'decide'`: Allow + Deny buttons (or the single Next button for the already-granted
 *   variant). The wizard's footer renders nothing here — the step body owns the buttons.
 * - `'restart'`: user has clicked Allow this session. The wizard's footer shows a
 *   "Restart Cmdr" primary button (which calls `relaunch()`). The step body keeps the
 *   Allow/Deny buttons live so the user can change their mind to Deny.
 *
 * Why we model this here, not in `StepFda.svelte`: the footer button lives in
 * `OnboardingWizard.svelte` for a uniform layout across all steps. The wizard needs to
 * know whether to render "Next", "Restart Cmdr", or nothing on each step.
 */
export type Step1FooterMode = 'decide' | 'restart'

/**
 * Which step-2 banner copy to render. M3 reads this; M2 only sets it on step transition.
 *
 * - `'granted'`: FDA is now granted (`hasFda === true`). "Thanks for granting…"
 * - `'denied'`: user clicked Deny on step 1. "You chose not to enable…"
 * - `'stuck'`: user clicked Allow but FDA still isn't granted in-session. "You said
 *   you wanted to enable Full Disk Access, but Cmdr doesn't seem to have gotten it…"
 *   Also covers Linux (no FDA, no banner needed — the step renders a Welcome).
 */
export type StepTwoFdaBanner = 'granted' | 'denied' | 'stuck' | 'linux'

interface OnboardingStateData {
  /** `null` when the wizard is closed; an integer step when open. */
  currentStep: OnboardingStep | null
  /** What opened the wizard. `null` when closed. */
  source: OnboardingSource | null
  /** Step 1 variant. Driven from persisted flags + the FDA probe on open. */
  step1Variant: Step1Variant
  /** Step 1 footer mode. Flips to `'restart'` when the user clicks Allow this session. */
  step1FooterMode: Step1FooterMode
  /** Pre-computed step-2 banner mode. M3 reads this; M2 stores it. */
  stepTwoBanner: StepTwoFdaBanner
}

const state = $state<OnboardingStateData>({
  currentStep: null,
  source: null,
  step1Variant: 'first-ask',
  step1FooterMode: 'decide',
  stepTwoBanner: 'stuck',
})

export function getOnboardingState(): Readonly<OnboardingStateData> {
  return state
}

/**
 * Inputs the resume rule reads. Kept in one shape so `+page.svelte` can pass a snapshot
 * of the flags + FDA probe in one call without mutating the live settings object.
 */
export interface ResumeContext {
  fullDiskAccessChoice: FullDiskAccessChoice
  isOnboarded: boolean
  hasFda: boolean
  /** Override platform for tests; defaults to `isMacOS()`. */
  isMac?: boolean
}

/**
 * Returns the step the wizard should resume at, given the persisted flags + FDA probe.
 * See plan § "Step persistence resume — edge cases" for the truth table:
 *
 * macOS:
 *   - `notAskedYet`                            → step 1 (first-ask)
 *   - `allow` && !hasFda && isOnboarded        → step 1 (revoked-later)
 *   - `allow` && hasFda                        → step 2 (already-granted banner)
 *   - `allow` && !hasFda && !isOnboarded       → step 2 (first-time stuck banner)
 *   - `deny`                                   → step 2 (denied banner)
 *
 * Linux: always step 2 (no FDA gate).
 */
export function resumeStepFor(ctx: ResumeContext): OnboardingStep {
  const isMac = ctx.isMac ?? isMacOS()
  if (!isMac) return 2
  if (ctx.fullDiskAccessChoice === 'notAskedYet') return 1
  if (ctx.fullDiskAccessChoice === 'allow' && !ctx.hasFda && ctx.isOnboarded) return 1
  return 2
}

/**
 * Returns the step 1 variant for the given resume context. Only meaningful when
 * `resumeStepFor()` says step 1. The `'already-granted'` variant fires when re-entering
 * from the menu / palette while FDA is currently granted (see plan round-2 #1).
 */
export function step1VariantFor(ctx: ResumeContext, source: OnboardingSource): Step1Variant {
  if (ctx.hasFda) return 'already-granted'
  if (ctx.fullDiskAccessChoice === 'allow' && ctx.isOnboarded) return 'revoked'
  // Menu / palette re-entry without FDA: treat as a fresh ask. The user opted into
  // re-opening; we don't want to show "revoked" framing unless `isOnboarded` actually
  // says they finished onboarding once.
  if (source === 'menu' || source === 'palette') {
    return ctx.isOnboarded ? 'revoked' : 'first-ask'
  }
  return 'first-ask'
}

/** Returns the step 2 banner mode for the given context. Used by M3 step 2 + by the */
/** state machine itself when advancing from step 1. */
export function stepTwoBannerFor(ctx: ResumeContext): StepTwoFdaBanner {
  const isMac = ctx.isMac ?? isMacOS()
  if (!isMac) return 'linux'
  if (ctx.hasFda) return 'granted'
  if (ctx.fullDiskAccessChoice === 'deny') return 'denied'
  return 'stuck'
}

/**
 * Open the wizard. Computes the resume step and step-1 variant from the resume context.
 * Callers that don't have flag data yet (e.g. M1 dev-force path with no settings load)
 * can pass `null` for `ctx` to default to step 1 first-ask on macOS / step 2 on Linux.
 */
export function openWizard(source: OnboardingSource, ctx: ResumeContext | null = null): void {
  state.source = source
  if (ctx === null) {
    // No flags available (M1-style dev force). Use sensible defaults: macOS lands on
    // step 1 with the first-ask variant; Linux skips to step 2.
    state.currentStep = isMacOS() ? 1 : 2
    state.step1Variant = 'first-ask'
    state.step1FooterMode = 'decide'
    state.stepTwoBanner = isMacOS() ? 'stuck' : 'linux'
    return
  }
  state.currentStep = resumeStepFor(ctx)
  state.step1Variant = step1VariantFor(ctx, source)
  state.step1FooterMode = 'decide'
  state.stepTwoBanner = stepTwoBannerFor(ctx)
}

export function closeWizard(): void {
  state.currentStep = null
  state.source = null
  state.step1Variant = 'first-ask'
  state.step1FooterMode = 'decide'
  state.stepTwoBanner = 'stuck'
}

/**
 * Advance to the next step. Refuses to advance past step 1 while the footer mode is
 * `'restart'` (Allow path requires an actual relaunch). No-op if already on the last step.
 */
export function nextStep(): void {
  if (state.currentStep === null) return
  if (state.currentStep === 1 && state.step1FooterMode === 'restart') return
  if (state.currentStep < ONBOARDING_STEP_COUNT) {
    state.currentStep = (state.currentStep + 1) as OnboardingStep
  }
}

/**
 * Go back one step. Disabled on the first reachable step:
 * - macOS: step 1 has no previous step.
 * - Linux: step 2 is the first reachable step (step 1 is skipped).
 *
 * When going back from step 2 to step 1, reset the footer to `'decide'` so the
 * Allow/Deny buttons are live again (plan M2 § "Back-from-step-2 with prior Deny").
 */
export function previousStep(): void {
  if (state.currentStep === null) return
  if (!isMacOS() && state.currentStep === 2) return
  if (state.currentStep > 1) {
    state.currentStep = (state.currentStep - 1) as OnboardingStep
    state.step1FooterMode = 'decide'
  }
}

/** True when on the first reachable step (Back should be disabled). */
export function isAtFirstStep(): boolean {
  if (!isMacOS()) return state.currentStep === 2
  return state.currentStep === 1
}

/** True when on the last step (Next should read "Finish" / submit instead). */
export function isAtLastStep(): boolean {
  return state.currentStep === ONBOARDING_STEP_COUNT
}

/** Flip step 1's footer mode to `'restart'`. Called by `StepFda.svelte` after Allow. */
export function setStep1Restart(): void {
  state.step1FooterMode = 'restart'
}

/** Manually set the step-1 variant. Tests use this; the wizard sets it via `openWizard()`. */
export function setStep1Variant(variant: Step1Variant): void {
  state.step1Variant = variant
}

/** Manually set the current step (tests + StepFda Deny flow that advances to step 2). */
export function setCurrentStep(step: OnboardingStep): void {
  state.currentStep = step
}

/**
 * Re-compute and store the step-2 banner mode. Called from the Allow / Deny handlers in
 * step 1 right after the choice is persisted so step 2 sees the freshest banner.
 */
export function setStepTwoBanner(banner: StepTwoFdaBanner): void {
  state.stepTwoBanner = banner
}

/** Reset to closed. For use in tests only. */
export function resetForTesting(): void {
  state.currentStep = null
  state.source = null
  state.step1Variant = 'first-ask'
  state.step1FooterMode = 'decide'
  state.stepTwoBanner = 'stuck'
}
