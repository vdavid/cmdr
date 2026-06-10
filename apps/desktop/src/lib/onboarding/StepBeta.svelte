<script lang="ts">
    import { onMount, onDestroy } from 'svelte'
    import OnboardingStepShell from './OnboardingStepShell.svelte'
    import SettingSwitch from '$lib/settings/components/SettingSwitch.svelte'
    import LinkButton from '$lib/ui/LinkButton.svelte'
    import { setFooterOverride, nextStep } from './onboarding-state.svelte'
    import { getSetting, getSettingDefinition, setSetting } from '$lib/settings'
    import { onSpecificSettingChange } from '$lib/settings/settings-store'
    import { betaSignup, openExternalUrl } from '$lib/tauri-commands'
    import { GITHUB_ISSUES_URL, BOOK_A_CALL_URL, ABOUT_DAVID_URL } from '$lib/beta-links'
    import { getAppLogger } from '$lib/logging/logger'

    /**
     * Step 3: Open beta disclosure.
     *
     * Three parts:
     *
     *   [Personal open-beta intro: David's first-person welcome + the three feedback
     *    channels (Help > Send feedback…, GitHub issues, book-a-call), linked through
     *    the shared `$lib/beta-links` constants]
     *   [Anonymous-analytics disclosure + an opt-out switch bound to `analytics.enabled`]
     *   [Optional contact email]
     *
     * The analytics + email parts reuse the exact wiring `settings/sections/UpdatesSection.svelte`
     * uses, so the Settings page and this onboarding page behave identically:
     *   - the opt-out switch is the registry-backed `<SettingSwitch id="analytics.enabled">`
     *     (default on; flipping it writes the setting immediately, like everywhere else),
     *   - the email field persists to `analytics.email` on every keystroke (local only) and,
     *     on commit of a valid address, calls the typed `betaSignup` wrapper, which POSTs
     *     ONLY the email (never an install id) and returns a typed result we map to a gentle
     *     inline note.
     *
     * This page is non-skippable: the AI step's forward button lands the user here, and the
     * footer here is a normal "Next" that advances to the final Optional step. See
     * `lib/onboarding/CLAUDE.md` § "Step 3 (Open beta)".
     */

    const log = getAppLogger('onboarding-beta')

    const analyticsDef = getSettingDefinition('analytics.enabled') ?? { label: '', description: '' }

    /** Click handler factory for the feedback links: intercepts the decorative href and routes
     * through `openExternalUrl` (Tauri blocks raw `<a>` navigation), logging on failure. */
    function openLink(url: string) {
        return (event: MouseEvent) => {
            event.preventDefault()
            void openExternalUrl(url).catch((error: unknown) => {
                log.warn('openExternalUrl({url}) failed: {error}', { url, error })
            })
        }
    }

    onMount(() => {
        // The "Next" button has no reactive deps (its handler closes over a module-level
        // function), so register once on mount rather than re-running an `$effect`.
        setFooterOverride([
            {
                label: 'Next',
                variant: 'primary',
                onclick: () => {
                    nextStep()
                },
            },
        ])
    })

    onDestroy(() => {
        // Clear the footer override so other steps' default buttons render again, and so a
        // teardown-then-remount doesn't leak stale closures.
        setFooterOverride(null)
    })

    // The beta contact email persists to settings on every keystroke (local only). On commit
    // (blur or Enter) with a valid address, we subscribe it to the beta mailing list via
    // `betaSignup`, which sends ONLY the email (never an install id), so usage stats can't be
    // tied back to it. This mirrors `UpdatesSection.svelte`'s logic exactly.
    let email = $state(getSetting('analytics.email'))
    onSpecificSettingChange('analytics.email', (value) => {
        email = value
    })

    // The inline result under the field. A typed kind, not a parsed message.
    type SignupFeedback = { kind: 'success' | 'failure' } | null
    let signupFeedback = $state<SignupFeedback>(null)
    // The last address we successfully submitted, so re-blurring an unchanged field doesn't resend.
    let lastSubmittedEmail = $state('')
    let signupInFlight = $state(false)

    const emailPattern = /^[^\s@]+@[^\s@]+\.[^\s@]+$/

    function handleEmailInput(event: Event) {
        const target = event.target as HTMLInputElement
        email = target.value
        setSetting('analytics.email', target.value)
        // Clearing the field only clears the local copy. Unsubscribing from the list happens
        // via Listmonk's own link.
        if (target.value.trim() === '') {
            signupFeedback = null
            lastSubmittedEmail = ''
        }
    }

    async function handleEmailCommit() {
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

    function handleEmailKeydown(event: KeyboardEvent) {
        if (event.key === 'Enter') {
            void handleEmailCommit()
        }
    }
</script>

<OnboardingStepShell>
    <h2 class="step-title">Help improve Cmdr!</h2>
    <p class="lede">
        Hi, I'm <LinkButton
            href={ABOUT_DAVID_URL}
            target="_blank"
            rel="noopener noreferrer"
            onclick={openLink(ABOUT_DAVID_URL)}>David</LinkButton
        >! I'm building Cmdr, and you're one of the very first people using it. Thanks for your trust! ❤️ Cmdr is in open beta:
        most of the app is solid, but some parts are still rough (especially Search and Select now). Your feedback helps me spot bugs and prioritize
        what to build next.
    </p>
    <ul class="feedback-list">
        <li>Found a bug or have an idea? <strong>Help &gt; Send feedback…</strong> sends it straight to me.</li>
        <li>
            Want to affect the roadmap? Vote on features on
            <LinkButton
                href={GITHUB_ISSUES_URL}
                target="_blank"
                rel="noopener noreferrer"
                onclick={openLink(GITHUB_ISSUES_URL)}>GitHub</LinkButton
            >. I'm happy if you take two minutes to add your ideas, we need items to get GitHub Issues kicked off.
        </li>
        <li>
            Up for a chat?
            <LinkButton
                href={BOOK_A_CALL_URL}
                target="_blank"
                rel="noopener noreferrer"
                onclick={openLink(BOOK_A_CALL_URL)}>Schedule a call with me</LinkButton
            >. I'd love to chat about all the nasty stuff you do with your files! And/or hear how you use Cmdr. (I obviously won't be doing this for very long, but while Cmdr is an Open beta, I don't expect many people booking calls.)
        </li>
    </ul>

    <p class="lede analytics-lede">
        To learn what's working and what isn't, during the open beta Cmdr sends anonymous usage stats: which features
        get used and how often, never anything from your files. It's on now, and you can turn it off anytime.
    </p>

    <section class="toggle-block" aria-labelledby="toggle-analytics-title">
        <header class="toggle-header">
            <div class="toggle-text">
                <h3 id="toggle-analytics-title" class="toggle-title">Send anonymous usage stats</h3>
                <p class="toggle-desc">{analyticsDef.description}</p>
            </div>
            <div class="toggle-control">
                <SettingSwitch id="analytics.enabled" />
                <p class="toggle-caption">Note that it's ON by default to encourage people to send me data during the Beta. You can change this any time in Settings.</p>
            </div>
        </header>
    </section>

    <section class="email-block" aria-labelledby="beta-email-title">
        <h3 id="beta-email-title" class="toggle-title">Stay in touch (optional)</h3>
        <input
            type="email"
            class="email-input"
            placeholder="you@example.com"
            value={email}
            oninput={handleEmailInput}
            onblur={handleEmailCommit}
            onkeydown={handleEmailKeydown}
            disabled={signupInFlight}
            aria-label="Stay in touch (optional)"
        />
        {#if signupFeedback?.kind === 'success'}
            <p class="signup-feedback success" role="status">
                Check your inbox to confirm your email. Thanks for helping out!
            </p>
        {:else if signupFeedback?.kind === 'failure'}
            <p class="signup-feedback failure" role="status">Sorry, we couldn't sign you up right now. Try again?</p>
        {/if}
        <p class="email-note">
            Drop your email and I'll reach out with the occasional question or update. The email address you enter here is stored only on your Mac and
            it's never connected to your usage stats, the two are intentionally two separate subsystems.
        </p>
    </section>
</OnboardingStepShell>

<style>
    .step-title {
        margin: 0 0 var(--spacing-md);
        /* 20% larger than body font (same calc() as StepFda/.welcome, StepAi/.step-title,
           and StepOptional/.step-title so all onboarding step headings match). */
        font-size: calc(var(--font-size-md) * 1.2);
        font-weight: 600;
        color: var(--color-text-primary);
    }

    .lede {
        margin: 0 0 var(--spacing-md);
        line-height: 1.5;
        color: var(--color-text-primary);
    }

    .analytics-lede {
        margin-bottom: var(--spacing-lg);
    }

    .feedback-list {
        margin: 0 0 var(--spacing-lg);
        padding-left: var(--spacing-lg);
        line-height: 1.5;
        color: var(--color-text-primary);
    }

    .feedback-list li {
        margin-bottom: var(--spacing-xs);
    }

    .feedback-list li:last-child {
        margin-bottom: 0;
    }

    .toggle-block {
        margin-bottom: var(--spacing-lg);
        padding: var(--spacing-lg);
        border: 1px solid var(--color-border);
        border-radius: var(--radius-md);
        background: var(--color-bg-primary);
    }

    .toggle-header {
        display: flex;
        align-items: flex-start;
        gap: var(--spacing-lg);
    }

    .toggle-text {
        flex: 1;
        min-width: 0;
    }

    .toggle-control {
        flex-shrink: 0;
        display: flex;
        flex-direction: column;
        align-items: flex-end;
        gap: var(--spacing-xs);
        padding-top: var(--spacing-xxs);
    }

    .toggle-caption {
        margin: 0;
        max-width: 14rem;
        text-align: right;
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
        line-height: 1.4;
    }

    .toggle-title {
        margin: 0 0 var(--spacing-xs);
        font-size: var(--font-size-md);
        font-weight: 600;
        color: var(--color-text-primary);
    }

    .toggle-desc {
        margin: 0;
        font-size: var(--font-size-sm);
        line-height: 1.5;
        color: var(--color-text-secondary);
    }

    .email-block {
        padding: var(--spacing-lg);
        border: 1px solid var(--color-border);
        border-radius: var(--radius-md);
        background: var(--color-bg-primary);
    }

    .email-input {
        width: 100%;
        margin-top: var(--spacing-sm);
        padding: var(--spacing-xs) var(--spacing-sm);
        font-size: var(--font-size-sm);
        color: var(--color-text-primary);
        background: var(--color-bg-secondary);
        border: 1px solid var(--color-border);
        border-radius: var(--radius-sm);
    }

    .email-input:focus {
        outline: none;
        border-color: var(--color-accent);
        box-shadow: var(--shadow-focus);
    }

    .email-input:disabled {
        opacity: 0.6;
    }

    .signup-feedback {
        margin: var(--spacing-xs) 0 0;
        font-size: var(--font-size-sm);
        line-height: 1.4;
    }

    .signup-feedback.success {
        color: var(--color-toast-success-stripe);
    }

    .signup-feedback.failure {
        color: var(--color-text-primary);
    }

    .email-note {
        margin: var(--spacing-sm) 0 0;
        font-size: var(--font-size-xs);
        line-height: 1.5;
        color: var(--color-text-secondary);
    }
</style>
