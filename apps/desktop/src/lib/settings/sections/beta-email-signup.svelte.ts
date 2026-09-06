/**
 * The beta contact email field's logic, shared by `UpdatesSection.svelte` (Settings) and
 * `$lib/onboarding/StepBeta.svelte` (first launch), so the two surfaces behave identically.
 *
 * The address persists to `analytics.email` on every keystroke (local only). On commit (blur
 * or Enter) with a valid address, it subscribes to the beta mailing list via `betaSignup`,
 * which sends ONLY the email (never an install id), so usage stats can't be tied back to it.
 *
 * A factory with per-mount `$state` behind getters, like
 * `KeyboardShortcutsSection.controller.svelte.ts`: each surface owns its own in-flight and
 * feedback state. Call it during component init.
 */
import { getSetting, setSetting } from '$lib/settings'
import { onSpecificSettingChange } from '$lib/settings/settings-store'
import { betaSignup } from '$lib/tauri-commands'

/** The inline result under the field. A typed kind, not a parsed message. */
export type SignupFeedback = { kind: 'success' | 'failure' } | null

const emailPattern = /^[^\s@]+@[^\s@]+\.[^\s@]+$/

export function createBetaEmailSignup() {
  let email = $state(getSetting('analytics.email'))
  // Another window's write lands here too (the Settings window and the onboarding sheet can be up at once).
  onSpecificSettingChange('analytics.email', (value) => {
    email = value
  })

  let signupFeedback = $state<SignupFeedback>(null)
  // The last address we successfully submitted, so re-blurring an unchanged field doesn't resend.
  let lastSubmittedEmail = ''
  let signupInFlight = $state(false)

  function handleInput(event: Event) {
    const target = event.target as HTMLInputElement
    email = target.value
    setSetting('analytics.email', target.value)
    // Clearing the field only clears the local copy. Unsubscribing from the list happens via
    // Listmonk's own link, per the field note.
    if (target.value.trim() === '') {
      signupFeedback = null
      lastSubmittedEmail = ''
    }
  }

  async function handleCommit() {
    const trimmed = email.trim()
    if (trimmed === '' || trimmed === lastSubmittedEmail || !emailPattern.test(trimmed)) {
      return
    }

    signupInFlight = true
    try {
      const result = await betaSignup(trimmed)
      if (result.kind === 'subscribed') {
        signupFeedback = { kind: 'success' }
        lastSubmittedEmail = trimmed
      } else {
        // `invalidEmail` or `softFailure`: a gentle try-again either way.
        signupFeedback = { kind: 'failure' }
      }
    } finally {
      signupInFlight = false
    }
  }

  function handleKeydown(event: KeyboardEvent) {
    if (event.key === 'Enter') {
      void handleCommit()
    }
  }

  return {
    get email() {
      return email
    },
    get signupFeedback() {
      return signupFeedback
    },
    get signupInFlight() {
      return signupInFlight
    },
    handleInput,
    handleCommit,
    handleKeydown,
  }
}
