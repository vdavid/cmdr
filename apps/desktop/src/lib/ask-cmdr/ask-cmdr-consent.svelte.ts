/**
 * Ask Cmdr consent gate: the opt-in that must precede any message reaching a provider
 * (spec §2.1 privacy line; plan §12). The record lives in `main.db` (agent state, not a
 * preference), read/written through the backend commands.
 *
 * The rail gates on {@link consentState}.accepted: while `null` (unknown/loading) it shows
 * nothing, `false` shows the consent screen, `true` shows the chat. Both the rail's consent
 * screen and the settings section drive the same accept/revoke here, so the two surfaces
 * stay in sync (each refreshes on mount / open).
 */

import { getAppLogger } from '$lib/logging/logger'
import {
  acceptAskCmdrConsent,
  askCmdrConsentStatus,
  revokeAskCmdrConsent,
  type AskCmdrConsentStatus,
} from '$lib/tauri-commands'

const log = getAppLogger('askCmdr')

interface ConsentState {
  /** `null` = not yet known (loading); `true`/`false` = the current-version opt-in. */
  accepted: boolean | null
  /** Unix secs the user last accepted the current copy, or `null`. */
  acceptedAt: number | null
}

export const consentState = $state<ConsentState>({ accepted: null, acceptedAt: null })

function apply(status: AskCmdrConsentStatus): void {
  consentState.accepted = status.accepted
  consentState.acceptedAt = status.accepted ? status.acceptedAt : null
}

/** Refresh the cached consent status from the store. Called on rail open and settings mount. */
export async function refreshConsent(): Promise<void> {
  try {
    apply(await askCmdrConsentStatus())
  } catch (e) {
    log.warn('reading consent status failed: {error}', { error: String(e) })
    // Fail closed: an unreadable status keeps the gate shut rather than opening it.
    consentState.accepted = false
    consentState.acceptedAt = null
  }
}

/** Record the opt-in (turn Ask Cmdr on) and refresh. Resolves to the new accepted state. */
export async function acceptConsent(): Promise<boolean> {
  try {
    await acceptAskCmdrConsent()
    await refreshConsent()
  } catch (e) {
    log.warn('recording consent failed: {error}', { error: String(e) })
  }
  return consentState.accepted === true
}

/** Turn Ask Cmdr off (clear consent) and refresh. Chats are kept. */
export async function revokeConsent(): Promise<void> {
  try {
    await revokeAskCmdrConsent()
    await refreshConsent()
  } catch (e) {
    log.warn('turning Ask Cmdr off failed: {error}', { error: String(e) })
  }
}
