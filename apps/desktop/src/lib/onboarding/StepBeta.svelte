<script lang="ts">
    import { onDestroy } from 'svelte'
    import OnboardingStepShell from './OnboardingStepShell.svelte'
    import SettingSwitch from '$lib/settings/components/SettingSwitch.svelte'
    import LinkButton from '$lib/ui/LinkButton.svelte'
    import StatusBadge from '$lib/ui/StatusBadge.svelte'
    import ShortcutChip from '$lib/ui/ShortcutChip.svelte'
    import { setFooterOverride, nextStep, requestWizardComplete } from './onboarding-state.svelte'
    import { getSetting, getSettingDefinition, setSetting } from '$lib/settings'
    import { onSpecificSettingChange } from '$lib/settings/settings-store'
    import { betaSignup, openExternalUrl } from '$lib/tauri-commands'
    import {
        GITHUB_REPO_URL,
        GITHUB_ISSUES_URL,
        BOOK_A_CALL_URL,
        ABOUT_DAVID_URL,
        DISCORD_INVITE_URL,
    } from '$lib/beta-links'
    import { getFirstShortcutReactive } from '$lib/shortcuts/reactive-shortcuts.svelte'
    import { getAppLogger } from '$lib/logging/logger'
    import { tString } from '$lib/intl/messages.svelte'
    import Trans from '$lib/intl/Trans.svelte'
    import type { Snippet } from 'svelte'

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
     * This page is non-skippable: the AI step's forward button lands the user here. The
     * footer offers two ways forward: a secondary "Start using Cmdr!" that finishes
     * onboarding right here (skipping the optional setup), and a primary "One more optional
     * setup step" that advances to the final Optional step. See
     * `lib/onboarding/CLAUDE.md` § "Step 3 (Open beta)".
     */

    const log = getAppLogger('onboarding-beta')

    const analyticsDef = getSettingDefinition('analytics.enabled') ?? { label: '', description: '' }

    // Drives the command-palette mention: when `app.commandPalette` is unbound the chip
    // renders nothing, so we drop the "with <chip>" tail rather than leave a gap.
    const commandPaletteShortcut = $derived(getFirstShortcutReactive('app.commandPalette'))

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

    // Guards a double-trigger while the step tears down. Both handlers are synchronous, so
    // this only matters for a rapid double-click on the same button.
    let advanceBusy = $state(false)

    function handleStart(): void {
        if (advanceBusy) return
        advanceBusy = true
        // Finish onboarding right here, skipping the optional setup step.
        requestWizardComplete()
    }

    function handleContinue(): void {
        if (advanceBusy) return
        advanceBusy = true
        nextStep()
    }

    // Re-register on `advanceBusy` change so the disabled state stays fresh.
    $effect(() => {
        void advanceBusy
        setFooterOverride([
            {
                label: tString('onboarding.stepBeta.footer.start'),
                variant: 'secondary',
                disabled: advanceBusy,
                onclick: () => {
                    handleStart()
                },
            },
            {
                label: tString('onboarding.stepBeta.footer.continue'),
                variant: 'primary',
                disabled: advanceBusy,
                onclick: () => {
                    handleContinue()
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

{#snippet david(children: Snippet)}<LinkButton
        href={ABOUT_DAVID_URL}
        target="_blank"
        rel="noopener noreferrer"
        onclick={openLink(ABOUT_DAVID_URL)}>{@render children()}</LinkButton
    >{/snippet}
{#snippet alpha(children: Snippet)}<StatusBadge status="alpha" />{@render children()}{/snippet}
{#snippet chip(children: Snippet)}<ShortcutChip commandId="app.commandPalette" clickable={false} />{@render children()}{/snippet}
{#snippet strong(children: Snippet)}<strong>{@render children()}</strong>{/snippet}
{#snippet code(children: Snippet)}<code>{@render children()}</code>{/snippet}
{#snippet github(children: Snippet)}<LinkButton
        href={GITHUB_ISSUES_URL}
        target="_blank"
        rel="noopener noreferrer"
        onclick={openLink(GITHUB_ISSUES_URL)}>{@render children()}</LinkButton
    >{/snippet}
{#snippet discord(children: Snippet)}<LinkButton
        href={DISCORD_INVITE_URL}
        target="_blank"
        rel="noopener noreferrer"
        onclick={openLink(DISCORD_INVITE_URL)}>{@render children()}</LinkButton
    >{/snippet}
{#snippet call(children: Snippet)}<LinkButton
        href={BOOK_A_CALL_URL}
        target="_blank"
        rel="noopener noreferrer"
        onclick={openLink(BOOK_A_CALL_URL)}>{@render children()}</LinkButton
    >{/snippet}
{#snippet repoLink(children: Snippet)}<LinkButton
        href={GITHUB_REPO_URL}
        target="_blank"
        rel="noopener noreferrer"
        onclick={openLink(GITHUB_REPO_URL)}>{@render children()}</LinkButton
    >{/snippet}

<OnboardingStepShell>
    <h2 class="step-title">{tString('onboarding.stepBeta.title')}</h2>
    <p class="lede"><Trans key="onboarding.stepBeta.greeting" snippets={{ david }} /></p>
    <p class="lede"><Trans key="onboarding.stepBeta.openBeta" snippets={{ alpha }} /></p>
    <p class="lede">{tString('onboarding.stepBeta.feedbackIntro')}</p>
    <ol class="feedback-list">
        <li>
            {#if commandPaletteShortcut}
                <Trans key="onboarding.stepBeta.feedback.inAppBound" snippets={{ strong, chip }} />
            {:else}
                <Trans key="onboarding.stepBeta.feedback.inAppUnbound" snippets={{ strong }} />
            {/if}
        </li>
        <li><Trans key="onboarding.stepBeta.feedback.github" snippets={{ github }} /></li>
        <li><Trans key="onboarding.stepBeta.feedback.discord" snippets={{ discord }} /></li>
        <li><Trans key="onboarding.stepBeta.feedback.call" snippets={{ call }} /></li>
    </ol>
    <p class="lede"><Trans key="onboarding.stepBeta.star" snippets={{ github: repoLink, code }} /></p>

    <p class="lede analytics-lede">{tString('onboarding.stepBeta.analyticsLede')}</p>

    <section class="toggle-block" aria-labelledby="toggle-analytics-title">
        <header class="toggle-header">
            <div class="toggle-text">
                <h3 id="toggle-analytics-title" class="toggle-title">{tString('onboarding.stepBeta.analyticsTitle')}</h3>
                <p class="toggle-desc">{analyticsDef.description}</p>
            </div>
            <div class="toggle-control">
                <SettingSwitch id="analytics.enabled" />
                <p class="toggle-caption">{tString('onboarding.stepBeta.analyticsCaption')}</p>
            </div>
        </header>
    </section>

    <section class="email-block" aria-labelledby="beta-email-title">
        <h3 id="beta-email-title" class="toggle-title">{tString('onboarding.stepBeta.emailTitle')}</h3>
        <input
            type="email"
            class="email-input"
            placeholder={tString('onboarding.stepBeta.emailPlaceholder')}
            value={email}
            oninput={handleEmailInput}
            onblur={handleEmailCommit}
            onkeydown={handleEmailKeydown}
            disabled={signupInFlight}
            aria-label={tString('onboarding.stepBeta.emailTitle')}
        />
        {#if signupFeedback?.kind === 'success'}
            <p class="signup-feedback success" role="status">{tString('onboarding.stepBeta.signup.success')}</p>
        {:else if signupFeedback?.kind === 'failure'}
            <p class="signup-feedback failure" role="status">{tString('onboarding.stepBeta.signup.failure')}</p>
        {/if}
        <p class="email-note">{tString('onboarding.stepBeta.emailNote')}</p>
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

    /* Keep the inline ALPHA badge centered on the text baseline run rather than riding high. */
    .lede :global(.feature-status-badge) {
        vertical-align: middle;
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

    .lede code {
        font-family: var(--font-mono);
        font-size: var(--font-size-xs);
        background: var(--color-bg-tertiary);
        padding: var(--spacing-xxs) var(--spacing-xs);
        border-radius: var(--radius-sm);
        color: var(--color-text-primary);
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
