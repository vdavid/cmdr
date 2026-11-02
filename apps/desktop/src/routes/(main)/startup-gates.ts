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

import {
  isForceOnboarding,
  checkFullDiskAccess,
  getMacosMajorVersion,
  getLaunchDayCount,
  getDockPinState,
  getFinderExtensionStatus,
  getRevealHandlerState,
  startIndexingAfterFdaDecision,
} from '#lib/tauri-commands/index.js'
import { openWizard as openOnboardingWizard } from '#lib/onboarding/onboarding-state.svelte.js'
import { runWhatsNewStartupTrigger } from '#lib/whats-new/whats-new-trigger.svelte.js'
import { forceSave, getSetting, setSetting } from '#lib/settings/index.js'
import { isE2eRun } from '#lib/app-mode.js'
import { notifyOnboardingComplete } from '#lib/updates/updater.svelte.js'
import { isMacOS } from '#lib/shortcuts/key-capture.js'
import { addToast } from '#lib/ui/toast/index.js'
import { tString } from '#lib/intl/messages.svelte.js'
import { getAppLogger } from '#lib/logging/logger.js'
import { isBelowSupportedMacOs, macosVersionLabel } from '#lib/utils/webkit-compat.js'
import { shouldShowDockNudge } from '#lib/dock/should-show-dock-nudge.js'
import { offerDockPin } from '#lib/dock/dock-nudge.js'
import { nudgeCouldFire, type NudgeContext } from '#lib/nudges/nudge-ledger.js'
import { readNudgeLedger } from '#lib/nudges/nudge-store.js'
import { shouldShowRevealNudge } from '#lib/reveal/should-show-reveal-nudge.js'
import { offerRevealHandler } from '#lib/reveal/reveal-nudge.js'
import {
  FINDER_EXTENSION_OFFER_LIVE,
  shouldShowFinderExtensionNudge,
} from '#lib/reveal/should-show-finder-extension-nudge.js'
import { offerFinderExtension } from '#lib/reveal/finder-extension-nudge.js'

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
  /**
   * Raises the one-time best-effort notice, given the version to name in it
   * ('10.15', '11'). The component owns the dialog; this only says "now".
   */
  showOldMacosNotice: (versionLabel: string) => void
}

/**
 * The FDA probe for a gate that has to go on either way. `check_full_disk_access` answers a bare
 * bool on every platform, so a rejection means the IPC bridge itself broke: it logs at error,
 * since nothing else will say so, and the gate carries on with `fallback` instead of stranding
 * launch on a blank window or the menu on a dead click.
 */
async function probeFullDiskAccess(fallback: boolean): Promise<boolean> {
  try {
    return await checkFullDiskAccess()
  } catch (error) {
    log.error('Full disk access probe returned an error; reading it as {fallback}: {error}', {
      fallback,
      error,
    })
    return fallback
  }
}

/**
 * Opens the backend FDA gate, for a launch that lands in the explorer without ever showing
 * the FDA question.
 *
 * A closed gate defers drive indexing, the network scan, the Downloads watcher, cloud volume
 * enumeration, and every launch-time `NSWorkspace` call, so that it means exactly one thing:
 * step 1 is on screen asking. The boot predicate (`src-tauri/src/fda_gate.rs::is_fda_pending`)
 * aims at the same rule from the other side, and this call is what keeps a disagreement
 * between the two from deferring all of it indefinitely with nothing on screen to explain it.
 *
 * The command is idempotent (a no-op once indexing is running, and the MTP watcher starts at
 * most once), so the ordinary launch, whose gate was already open at boot, pays one cheap
 * round trip.
 *
 * A refusal is logged, never thrown: whatever broke, launch carries on.
 */
async function openFdaGateForLaunch(): Promise<void> {
  try {
    await startIndexingAfterFdaDecision()
  } catch (error) {
    log.warn('Could not open the FDA gate at launch; indexing may stay deferred this session: {error}', {
      error,
    })
  }
}

/**
 * Reads `CMDR_FORCE_ONBOARDING`, settings, and the FDA probe, then flips the right
 * top-level state. See `apps/desktop/src/lib/onboarding/CLAUDE.md` § "Mount + onboarding
 * flag" for the truth table this implements.
 */
export async function resolveOnboardingMount(ctx: StartupGatesContext): Promise<void> {
  const forceOnboarding = await isForceOnboarding().catch(() => false)
  // A broken probe reads as granted, so launch still reaches the explorer.
  const hasFda = await probeFullDiskAccess(true)
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
    await openFdaGateForLaunch()
    return
  }

  if (fullDiskAccessChoice === 'deny' && isOnboarded) {
    // User explicitly denied and already finished onboarding. Don't re-prompt.
    ctx.showApp()
    maybeFireUpgradeNudge()
    await openFdaGateForLaunch()
    return
  }

  // Everything else routes through the wizard: first-launch (unanswered),
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
 * Raises the one-time notice telling a Mac below `SUPPORTED_MACOS_MAJOR` that
 * Cmdr here is best effort, then records it so no later launch repeats it.
 *
 * The floor the BUNDLE refuses to launch on (10.15) sits below the floor Cmdr is
 * developed against (12), and that gap is deliberate. This is the soft half of
 * saying so; the hard block for a WebKit that can't run the app at all is the
 * inline guard in `apps/desktop/src/app.html`.
 *
 * Order of the guards is the cheap ones first, so the common case (a modern Mac
 * that has already been told, or any Linux box) never pays for the IPC round
 * trip. A probe that throws counts as supported: `isBelowSupportedMacOs` reads an
 * unparsed version the same way, and warning somebody because a probe broke is
 * worse than staying quiet.
 *
 * Marks the flag when the notice is RAISED, not when it's dismissed, matching
 * `maybeFireUpgradeNudge`. A crash between the two costs one reading of a notice
 * about cosmetic imperfection; the alternative risks repeating it forever if the
 * dismissal write never lands.
 *
 * Suppressed under E2E mode for the same reason the upgrade nudge is: it would
 * leak into whichever spec ran first. E2E runs on modern macOS anyway, so the
 * branch is unreachable there in practice; the guard is belt and braces.
 */
export async function maybeShowOldMacosNotice(ctx: StartupGatesContext): Promise<void> {
  if (isE2eRun()) return
  if (!isMacOS()) return
  if (getSetting('advanced.oldMacosNoticeShown')) return
  const major = await getMacosMajorVersion().catch(() => 0)
  if (!isBelowSupportedMacOs(major)) return
  log.info(`macOS ${macosVersionLabel(major)} is below the supported floor; showing the best-effort notice once`)
  setSetting('advanced.oldMacosNoticeShown', true)
  ctx.showOldMacosNotice(macosVersionLabel(major))
}

/**
 * The free half of every nudge's decision, read fresh.
 *
 * A getter rather than a value because both offers run twice per launch (boot
 * and the wizard re-attempt), and the wizard flag and the ledger both move
 * between the two.
 */
function nudgeContext(ctx: StartupGatesContext): NudgeContext {
  return {
    automatedRun: isE2eRun(),
    onMacOs: isMacOS(),
    onboarded: getSetting('onboarding.completed'),
    onboardingShowing: ctx.isOnboardingVisible(),
    ledger: readNudgeLedger(),
    now: new Date(),
  }
}

/**
 * Runs the once-ever offers, in the order they earn their slot.
 *
 * ❗ Sequential, ❌ never `Promise.all`: each offer stamps the shared ledger only
 * after its own IPC round trips, so two started together would both read an
 * unstamped ledger and both speak on the same launch — exactly what the cooldown
 * exists to prevent. The reveal handler goes first because it's the more
 * valuable offer; the Dock then waits out the cooldown.
 */
export async function maybeOfferNudges(ctx: StartupGatesContext): Promise<void> {
  await maybeOfferRevealHandler(ctx)
  await maybeOfferDockPin(ctx)
  if (FINDER_EXTENSION_OFFER_LIVE) await maybeOfferFinderExtension(ctx)
}

/**
 * Offers the Finder extension, once, a week of launch days in. The last of the offers,
 * since it's the only one with an ongoing cost. Re-attempted when the wizard closes, for
 * the reason `maybeOfferDockPin` is.
 */
export async function maybeOfferFinderExtension(ctx: StartupGatesContext): Promise<void> {
  const context = nudgeContext(ctx)
  if (!nudgeCouldFire(context, 'finderExtension')) return

  const [launchDayCount, extensionStatus] = await Promise.all([getLaunchDayCount(), getFinderExtensionStatus()])
  if (!shouldShowFinderExtensionNudge({ ...context, launchDayCount, extensionStatus })) return

  log.info('Offering the Finder extension after {launchDayCount} launch days', { launchDayCount })
  offerFinderExtension()
}

/**
 * Offers to catch "Show in Finder", once, a couple of days into using it.
 *
 * The first of the two offers, because it's the more valuable one; the shared
 * cooldown in `#lib/nudges/` then holds the Dock offer back for three days.
 *
 * Async because the decision needs two backend answers: how many days the
 * launch-day ledger holds, and who holds the `NSFileViewer` key. `nudgeCouldFire`
 * runs first so the ordinary launch — the offer already made, or a machine with
 * no such key — pays for neither round trip.
 *
 * Re-attempted when the wizard closes, for the reason `maybeOfferDockPin` is.
 */
export async function maybeOfferRevealHandler(ctx: StartupGatesContext): Promise<void> {
  const context = nudgeContext(ctx)
  if (!nudgeCouldFire(context, 'reveal')) return

  const [launchDayCount, handlerStatus] = await Promise.all([getLaunchDayCount(), getRevealHandlerState()])
  if (!shouldShowRevealNudge({ ...context, launchDayCount, handlerStatus })) return

  log.info('Offering the reveal handler after {launchDayCount} launch days', { launchDayCount })
  offerRevealHandler()
}

/**
 * Offers to put Cmdr in the Dock, once, a few days into using it.
 *
 * Async because the decision needs two backend answers: how many days the
 * launch-day ledger holds, and whether a Cmdr tile could go in the Dock at all.
 * `nudgeCouldFire` runs first so the ordinary launch — the offer already made,
 * or a machine with no Dock — pays for neither round trip.
 *
 * Stamps `behavior.dockPinNudgeOfferedAt` when the toast is RAISED (inside
 * `offerDockPin`), not when it's answered, matching `maybeFireUpgradeNudge`: a
 * crash between the two costs one offer, where the other order risks repeating
 * the toast forever.
 *
 * Re-attempted when the wizard closes, so a person finishing onboarding on their
 * fourth day isn't quietly skipped (mirrors the "What's new" re-attempt).
 */
export async function maybeOfferDockPin(ctx: StartupGatesContext): Promise<void> {
  const context = nudgeContext(ctx)
  if (!nudgeCouldFire(context, 'dockPin')) return

  const [launchDayCount, pinState] = await Promise.all([getLaunchDayCount(), getDockPinState()])
  if (!shouldShowDockNudge({ ...context, launchDayCount, pinState })) return

  log.info('Offering the Dock pin after {launchDayCount} launch days', { launchDayCount })
  offerDockPin()
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
  // A broken probe reads as not granted, so the wizard the person asked for still opens on step 1.
  const hasFda = await probeFullDiskAccess(false)
  openOnboardingWizard(source, {
    fullDiskAccessChoice: getSetting('onboarding.fullDiskAccessChoice'),
    isOnboarded: getSetting('onboarding.completed'),
    hasFda,
  })
  ctx.setOnboardingVisible(true)
}
