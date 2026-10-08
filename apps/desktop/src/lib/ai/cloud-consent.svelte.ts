/**
 * "Allow cloud AI": the one consent that must precede anything reaching a cloud AI service, for
 * every AI feature (folder suggestions, search, select by description, Ask Cmdr, MCP `ai_search`).
 * The record lives in `main.db`, and the backend enforces it in `ai::manager::resolve_backend`
 * for every cloud call (`src-tauri/src/ai/DETAILS.md` § Cloud AI consent). This module only
 * mirrors it so the switch and the feature gates can render: nothing here grants or refuses
 * anything on its own.
 *
 * {@link cloudConsentState}.accepted is `null` (not known yet), `false`, or `true`. Every gate
 * reads it through {@link cloudAiBlocked}, where `null` counts as blocked. Each window keeps
 * its own copy: the first {@link refreshCloudConsent} subscribes to `CloudAiConsentChanged`, so
 * a switch flipped in Settings reaches the main window's gates at once.
 *
 * **Only the switch's click grants.** {@link acceptCloudConsent} is imported by
 * `AiCloudConsentToggle.svelte` alone (`cloud-consent-call-sites.test.ts` pins it).
 *
 * **A "no" the store refused is HELD, not dropped.** {@link declineCloudConsent} retries a
 * refused revoke once, then records the answer in `settings.json` (`ai.cloudConsentRevokePending`),
 * which every Rust cloud gate reads, so the "no" holds at once. {@link settleHeldCloudConsentRevoke}
 * retries the store on every refresh and at launch, and lets go once it lands.
 */

import { getAppLogger } from '$lib/logging/logger'
import type { ManagedAiRefusal } from '$lib/ipc/bindings'
import { forceSave, getSetting, setSetting, type AiProvider } from '$lib/settings'
import { openSettingsWindow, type SettingsSurface } from '$lib/settings/settings-window'
import {
  acceptCloudAiConsent,
  cloudAiConsentRevokePendingChanged,
  cloudAiConsentStatus,
  onCloudAiConsentChanged,
  revokeCloudAiConsent,
  type CloudAiConsentStatus,
} from '$lib/tauri-commands'

const log = getAppLogger('ai')

/** The hidden setting that holds a refused "no" until `main.db` takes it. */
const HELD_REVOKE = 'ai.cloudConsentRevokePending'

interface CloudConsentState {
  /** `null` = not yet known (loading); `true`/`false` = the current-version consent. */
  accepted: boolean | null
  /** Unix secs the user last allowed cloud AI under the current copy, or `null`. */
  acceptedAt: number | null
  /**
   * Set when the organization's policy rules out every cloud host: the switch is locked off for
   * this reason, whatever the record says. The backend decides it (`CloudAiConsentStatus.managed`).
   */
  managed: ManagedAiRefusal | null
}

/**
 * What an accept or decline came to. `notSaved` means the person's choice isn't what the store
 * holds, so the caller has to say so (or try again): ❌ never let it pass as `done`, because a
 * silently kept consent is a "no" that didn't stick.
 */
export type ConsentOutcome = 'done' | 'notSaved'

export const cloudConsentState = $state<CloudConsentState>({ accepted: null, acceptedAt: null, managed: null })

/** The DOM id the switch carries in Settings, so "Open AI settings" lands right on it. */
export const CLOUD_CONSENT_ANCHOR = 'settings-ai-cloud-consent'

/**
 * Open Settings > AI > Provider at the Allow cloud AI switch. Every "cloud AI is off" surface
 * (the rail's gate, the Ask Cmdr section's hint, the query dialogs, the translate toast) goes
 * here, each naming itself as `surface` for the `settings_opened` event.
 */
export function openCloudConsentSettings(surface: SettingsSurface): void {
  void openSettingsWindow(surface, ['AI', 'Provider'], CLOUD_CONSENT_ANCHOR).catch((e: unknown) => {
    log.warn('opening the AI settings failed: {error}', { error: String(e) })
  })
}

/**
 * Whether a cloud call would be refused right now: the provider is Cloud and consent doesn't
 * read accepted. Not known yet counts as refused, so a gate never flashes open. Local and off
 * are never blocked here: nothing leaves the Mac there (off has its own gates).
 */
export function cloudAiBlocked(provider: AiProvider): boolean {
  return provider === 'cloud' && cloudConsentState.accepted !== true
}

let listening = false

/** Subscribe once per window, so a change made in any window re-reads the status here. */
function ensureListening(): void {
  if (listening) return
  listening = true
  // Outside Tauri (a unit test with no event mock) there's no event bus, and the call can throw
  // before it returns a promise. The gates still refresh on mount and on open.
  const giveUp = (e: unknown): void => {
    listening = false
    log.debug('listening for cloud consent changes failed: {error}', { error: String(e) })
  }
  try {
    void onCloudAiConsentChanged(() => {
      void refreshCloudConsent()
    }).catch(giveUp)
  } catch (e) {
    giveUp(e)
  }
}

function apply(status: CloudAiConsentStatus): void {
  cloudConsentState.accepted = status.accepted
  cloudConsentState.acceptedAt = status.accepted ? status.acceptedAt : null
  cloudConsentState.managed = status.managed
}

/**
 * Sets or lets go of the held "no", saves `settings.json` NOW (the Rust gates read the file, and
 * the store's usual save is debounced), then tells the gates to re-read. Resolves to whether the
 * save landed.
 */
async function setHeldRevoke(held: boolean): Promise<boolean> {
  setSetting(HELD_REVOKE, held)
  const saved = await forceSave()
  try {
    await cloudAiConsentRevokePendingChanged()
  } catch (e) {
    log.warn('telling the cloud gates about a held "no" failed: {error}', { error: String(e) })
  }
  return saved
}

/**
 * Retry a held "no" against the store, and let go of it once the store takes it. A no-op when
 * nothing is held. Runs on every {@link refreshCloudConsent} and once at launch.
 */
export async function settleHeldCloudConsentRevoke(): Promise<void> {
  if (!getSetting(HELD_REVOKE)) return
  try {
    await revokeCloudAiConsent()
  } catch (e) {
    log.warn("a held 'no' to cloud AI still can't reach the store, so it stays held: {error}", {
      error: String(e),
    })
    return
  }
  if (!(await setHeldRevoke(false))) {
    log.warn("the store took a held 'no', but settings.json wouldn't let go of it; the next refresh retries")
  }
}

/** Refresh the cached status from the store. Called on mount by every surface that gates on it. */
export async function refreshCloudConsent(): Promise<void> {
  ensureListening()
  await settleHeldCloudConsentRevoke()
  try {
    apply(await cloudAiConsentStatus())
  } catch (e) {
    log.warn('reading the cloud AI consent status failed: {error}', { error: String(e) })
    // Fail closed: an unreadable status keeps every gate shut rather than opening it.
    cloudConsentState.accepted = false
    cloudConsentState.acceptedAt = null
  }
}

/**
 * Record "Allow cloud AI" and refresh. `done` only when the store now reads accepted.
 *
 * ❌ Only the switch's own click calls this (`AiCloudConsentToggle.svelte`). A deliberate yes
 * lets go of any held "no" FIRST, or the next refresh would revoke it again.
 */
export async function acceptCloudConsent(): Promise<ConsentOutcome> {
  if (getSetting(HELD_REVOKE)) await setHeldRevoke(false)
  try {
    await acceptCloudAiConsent()
  } catch (e) {
    log.warn('recording cloud AI consent failed: {error}', { error: String(e) })
  }
  await refreshCloudConsent()
  return cloudConsentState.accepted === true ? 'done' : 'notSaved'
}

/** One revoke attempt, then a re-read so the surfaces show what the store holds. */
async function revokeOnce(): Promise<ConsentOutcome> {
  let outcome: ConsentOutcome = 'done'
  try {
    await revokeCloudAiConsent()
  } catch (e) {
    log.warn('turning cloud AI off failed: {error}', { error: String(e) })
    outcome = 'notSaved'
  }
  await refreshCloudConsent()
  return outcome
}

/**
 * Turn cloud AI off as a person's answer, wherever they gave it (the switch, onboarding's
 * "no AI"). Every "no" goes through here: it revokes (which also stops in-flight cloud calls),
 * gives a refusal one more try, and holds the "no" when the store refuses both.
 *
 * `done` when the "no" holds (recorded, or held for the store); `notSaved` only when neither the
 * store nor `settings.json` took it. `ai.provider` is untouched.
 */
export async function declineCloudConsent(): Promise<ConsentOutcome> {
  if ((await revokeOnce()) === 'done' || (await revokeOnce()) === 'done') return 'done'
  if (await setHeldRevoke(true)) {
    log.warn("the store refused to turn cloud AI off twice; holding the 'no' until it takes it")
    await refreshCloudConsent()
    return 'done'
  }
  log.warn(
    "the store refused to turn cloud AI off twice, and settings.json wouldn't hold the 'no' either; consent stays recorded",
  )
  return 'notSaved'
}

/** Test-only: back to "not known yet", with no subscription. */
export function _resetCloudConsentForTests(): void {
  cloudConsentState.accepted = null
  cloudConsentState.acceptedAt = null
  cloudConsentState.managed = null
  listening = false
}
