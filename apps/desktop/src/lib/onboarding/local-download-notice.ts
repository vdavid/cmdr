/**
 * Says it when the local AI model download that onboarding's step 2 started ends without
 * finishing, so the user doesn't walk away believing the local model is ready.
 *
 * It waits for the wizard to close: a toast under the soft sheet would go unseen, and the
 * download can end at any point, before or after the wizard does (`startAiDownload()` resolves
 * only when the whole download is over). So a failure while the wizard is up is HELD and raised
 * on close; one landing after it closed is raised at once.
 *
 * The wizard's visibility arrives through {@link setWizardShowingForDownloadNotice}, fed by
 * `setOnboardingVisible()` in `routes/(main)/+page.svelte`, the one place it moves.
 */

import { addToast, dismissToast } from '$lib/ui/toast'
import LocalDownloadFailedToastContent from './LocalDownloadFailedToastContent.svelte'

export const LOCAL_DOWNLOAD_FAILED_TOAST_ID = 'onboarding-local-download-failed'

let wizardShowing = false
let heldFailure = false

/** Mirror the wizard's visibility. Closing it raises a failure held while it was up. */
export function setWizardShowingForDownloadNotice(showing: boolean): void {
  wizardShowing = showing
  if (showing || !heldFailure) return
  heldFailure = false
  showToast()
}

/**
 * The download step 2 started ended without finishing, for a reason other than the user
 * switching away from Local. Raised now, or when the wizard closes.
 */
export function noteLocalDownloadFailed(): void {
  if (wizardShowing) {
    heldFailure = true
    return
  }
  showToast()
}

/**
 * Forget a failure: the user picked something else, or started a new attempt, so the old
 * failure no longer describes what they'll have when the wizard closes.
 */
export function clearLocalDownloadFailure(): void {
  heldFailure = false
  dismissToast(LOCAL_DOWNLOAD_FAILED_TOAST_ID)
}

function showToast(): void {
  addToast(LocalDownloadFailedToastContent, {
    id: LOCAL_DOWNLOAD_FAILED_TOAST_ID,
    level: 'warn',
    dismissal: 'persistent',
  })
}

/** Reset the module's state. For tests only. */
export function resetLocalDownloadNoticeForTesting(): void {
  wizardShowing = false
  heldFailure = false
}
