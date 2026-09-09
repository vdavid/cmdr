/**
 * The beta contact email field's logic, shared by `UpdatesSection.svelte` (Settings) and
 * `$lib/onboarding/StepBeta.svelte` (first launch), so the two surfaces behave identically.
 *
 * The address persists to `analytics.email` on every keystroke (local only). On commit with
 * a valid address, it subscribes to the beta mailing list via `betaSignup`, which sends ONLY
 * the email (never an install id), so usage stats can't be tied back to it.
 *
 * What counts as a commit differs by surface, which is what `commitOnBlur` is for. Settings
 * submits when the field loses focus, the way every other row there applies itself.
 * Onboarding's checklist has an explicit Save button instead: a row that ticks itself on the
 * way past would claim the user asked for something they only tabbed through.
 *
 * A factory with per-mount `$state` behind getters, like
 * `KeyboardShortcutsSection.controller.svelte.ts`: each surface owns its own in-flight and
 * feedback state. Call it during component init.
 */
import { getSetting, setSetting } from '$lib/settings'
import { onSpecificSettingChange } from '$lib/settings/settings-store'
import { betaSignup } from '$lib/tauri-commands'

/**
 * The inline result under the field. A typed kind, not a parsed message.
 *
 * A failure carries WHY, because the two reasons want different words: the list rejected the
 * address (fix the typo) or we never reached the list (try again, nothing is lost). A surface
 * that only has one failure line can still branch on `kind` alone.
 */
export type SignupFeedback = { kind: 'success' } | { kind: 'failure'; reason: 'invalidEmail' | 'unreachable' } | null

export interface BetaEmailSignupOptions {
  /** Submit when the field loses focus. `false` leaves committing to the caller's own control. */
  commitOnBlur?: boolean
  /** Fired on a commit the mailing list accepted. */
  onSubscribed?: () => void
}

const emailPattern = /^[^\s@]+@[^\s@]+\.[^\s@]+$/

/**
 * Does this address look like one we'd submit? The same test `handleCommit` gates on, so a
 * surface that wants to show whether the field is usable agrees with what actually gets sent,
 * rather than re-deriving "looks like an email" a second time.
 */
export function isValidBetaEmail(email: string): boolean {
  return emailPattern.test(email.trim())
}

export function createBetaEmailSignup(options: BetaEmailSignupOptions = {}) {
  const { commitOnBlur = true, onSubscribed } = options

  let email = $state(getSetting('analytics.email'))
  // Another window's write lands here too (the Settings window and the onboarding sheet can be up at once).
  onSpecificSettingChange('analytics.email', (value) => {
    email = value
  })

  let signupFeedback = $state<SignupFeedback>(null)
  // The last address we successfully submitted, so re-committing an unchanged field doesn't resend.
  let lastSubmittedEmail = ''
  let signupInFlight = $state(false)

  function handleInput(event: Event) {
    const target = event.target as HTMLInputElement
    email = target.value
    setSetting('analytics.email', target.value)
    // A verdict belongs to the address it was about, so editing clears it rather than leaving
    // a green "check your inbox" hanging under a half-typed address.
    signupFeedback = null
    // Clearing the field only clears the local copy. Unsubscribing from the list happens via
    // Listmonk's own link, per the field note.
    if (target.value.trim() === '') {
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
        onSubscribed?.()
      } else {
        signupFeedback = { kind: 'failure', reason: result.kind === 'invalidEmail' ? 'invalidEmail' : 'unreachable' }
      }
    } finally {
      signupInFlight = false
    }
  }

  function handleBlur() {
    if (commitOnBlur) void handleCommit()
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
    /** Is the current text something we'd submit? */
    get isValid() {
      return isValidBetaEmail(email)
    },
    /**
     * Should the field wear its error ring? Only once there's something in it: an empty
     * optional field isn't a mistake, it's an answer.
     */
    get showInvalid() {
      return email.trim() !== '' && !isValidBetaEmail(email)
    },
    /** Is there a fresh, valid address that hasn't been sent yet? */
    get canSubmit() {
      return isValidBetaEmail(email) && email.trim() !== lastSubmittedEmail && !signupInFlight
    },
    handleInput,
    handleCommit,
    handleBlur,
    handleKeydown,
  }
}
