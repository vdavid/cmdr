/**
 * The gates the app shell runs once the window is up: whether the user lands in
 * the onboarding wizard or straight in the explorer, the one-time upgrade nudge,
 * and the automatic "What's new" check. Wizard re-entry from the menu / palette
 * lives here too, since it reads the same settings + FDA probe.
 *
 * These decide what a first-run user SEES, so they live outside `+page.svelte`:
 * every branch is then exercisable without mounting the whole shell. The
 * reactive `$state` they flip stays in the component and crosses through
 * `StartupGatesContext` — setters for writes, GETTERS for reads, so a moved
 * closure can't answer "is the wizard up?" from a setup-time snapshot.
 */

import { isForceOnboarding, checkFullDiskAccess } from '$lib/tauri-commands'
import { openWizard as openOnboardingWizard } from '$lib/onboarding/onboarding-state.svelte'
import { runWhatsNewStartupTrigger } from '$lib/whats-new/whats-new-trigger.svelte'
import { forceSave, getSetting, setSetting } from '$lib/settings'
import { isE2eRun } from '$lib/app-mode'
import { notifyOnboardingComplete } from '$lib/updates/updater.svelte'
import { isMacOS } from '$lib/shortcuts/key-capture'
import { addToast } from '$lib/ui/toast'
import { tString } from '$lib/intl/messages.svelte'
import { getAppLogger } from '$lib/logging/logger'

// Same category as the rest of the app shell's logging: these gates read as
// `+page.svelte`'s startup in a log file, wherever the code sits.
const log = getAppLogger('main-page')

/** The component-owned state these gates read and write. */
export interface StartupGatesContext {
  /**
   * Shows or hides the onboarding wizard. The component flips its own
   * `showOnboarding` state AND mirrors the flag the updater reads
   * (`setOnboardingShowing`), so the two can never drift.
   */
  setOnboardingVisible: (visible: boolean) => void
  /** Live read of the wizard's visibility. */
  isOnboardingVisible: () => boolean
  /** Reveals the app shell (`showApp = true`). */
  showApp: () => void
  /** True while the expiration or commercial-reminder modal is up. */
  isOtherStartupModalOpen: () => boolean
}

/**
 * Reads `CMDR_FORCE_ONBOARDING`, settings, and the FDA probe, then flips the right
 * top-level state. See `apps/desktop/src/lib/onboarding/CLAUDE.md` § "Mount + onboarding
 * flag" for the truth table this implements.
 */
export async function resolveOnboardingMount(ctx: StartupGatesContext): Promise<void> {
  const forceOnboarding = await isForceOnboarding().catch(() => false)
  const hasFda = await checkFullDiskAccess()
  const fullDiskAccessChoice = getSetting('onboarding.fullDiskAccessChoice')
  const isOnboarded = getSetting('onboarding.completed')
  const wizardCtx = { fullDiskAccessChoice, isOnboarded, hasFda }

  if (forceOnboarding) {
    openOnboardingWizard('force', wizardCtx)
    ctx.setOnboardingVisible(true)
    ctx.showApp()
    return
  }

  if (hasFda) {
    // Granted-now: mirror the setting if it diverged (covers OS-side toggles), then
    // either skip or mark onboarded based on the `isOnboarded` flag.
    if (fullDiskAccessChoice !== 'allow') {
      setSetting('onboarding.fullDiskAccessChoice', 'allow')
      if (!(await forceSave())) {
        log.warn('Could not mirror onboarding.fullDiskAccessChoice=allow; FDA may re-prompt on next launch')
      }
    }
    if (!isOnboarded) {
      // Pre-wizard users who granted FDA before the wizard existed: unblock the
      // update toast by marking them onboarded.
      await notifyOnboardingComplete()
    }
    ctx.showApp()
    maybeFireUpgradeNudge()
    return
  }

  if (fullDiskAccessChoice === 'deny' && isOnboarded) {
    // User explicitly denied and already finished onboarding. Don't re-prompt.
    ctx.showApp()
    maybeFireUpgradeNudge()
    return
  }

  // Everything else routes through the wizard: first-launch (notAskedYet),
  // revoke-after-allow, first-time-stuck (Allow but didn't grant), or
  // Deny-but-not-onboarded.
  openOnboardingWizard('first-launch', wizardCtx)
  ctx.setOnboardingVisible(true)
  ctx.showApp()
}

/**
 * Fires the one-time `info` toast pointing existing users at the new
 * `Cmdr > Onboarding…` menu item (and the matching palette entry on Linux).
 * Persists `onboarding.upgradeNudgeShown: true` after firing so the toast
 * never appears again on this machine.
 *
 * Only the two `showApp` branches of `resolveOnboardingMount` call it, so the
 * wizard is closed by definition; no extra visibility check needed.
 *
 * Suppressed under E2E mode: the toast would leak into the first Playwright
 * test after every fresh-data-dir launch (each shard gets its own data dir),
 * tripping the fixture safety net. E2E mode isn't a real user and the
 * upgrade-discovery affordance doesn't matter there; `startup-gates.test.ts`
 * covers the firing behaviour instead.
 */
export function maybeFireUpgradeNudge(): void {
  if (isE2eRun()) return
  if (getSetting('onboarding.upgradeNudgeShown')) return
  const message = isMacOS() ? tString('main.upgradeNudge.mac') : tString('main.upgradeNudge.other')
  addToast(message, { level: 'info' })
  setSetting('onboarding.upgradeNudgeShown', true)
}

/**
 * Runs the automatic "What's new" post-update check. Reads `onboarding.completed` from settings
 * and the live startup-modal flags, then hands off to the pure decision in
 * `whats-new-trigger`. Called once after onboarding resolves and re-attempted when the
 * onboarding wizard closes (mirroring the update-toast re-attempt in `updater.svelte.ts`).
 * The trigger itself no-ops if its dialog is already open, so the re-attempt is safe.
 *
 * Suppressed at boot under E2E mode (`force` stays false): E2E grants FDA via the mock,
 * so the app boots onboarded, which would make the inaugural-showcase popup auto-open and
 * leak into whichever spec runs first (tripping the overlay leak guard). The dedicated
 * `whats-new.spec.ts` drives the real auto path explicitly through `e2e-rerun-whats-new`,
 * which calls this with `force: true`.
 */
export async function maybeRunWhatsNew(ctx: StartupGatesContext, force = false): Promise<void> {
  if (!force && isE2eRun()) return
  await runWhatsNewStartupTrigger({
    onboarded: getSetting('onboarding.completed'),
    onboardingShowing: ctx.isOnboardingVisible(),
    otherStartupModalOpen: ctx.isOtherStartupModalOpen(),
  })
}

/**
 * Opens the onboarding wizard for re-entry from the menu item or the command
 * palette. Always opens at step 1 on macOS (step 2 on Linux) regardless of
 * `isOnboarded`: `openWizard()` itself enforces this for both the `menu` and the
 * `palette` source (only `force` / `first-launch` honour the resume rule). No-op
 * when the wizard is already open.
 */
export async function openOnboardingFromMenuOrPalette(
  ctx: StartupGatesContext,
  source: 'menu' | 'palette',
): Promise<void> {
  if (ctx.isOnboardingVisible()) return
  const hasFda = await checkFullDiskAccess()
  openOnboardingWizard(source, {
    fullDiskAccessChoice: getSetting('onboarding.fullDiskAccessChoice'),
    isOnboarded: getSetting('onboarding.completed'),
    hasFda,
  })
  ctx.setOnboardingVisible(true)
}
