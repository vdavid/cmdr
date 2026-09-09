<script lang="ts">
    import { onDestroy, onMount } from 'svelte'
    import OnboardingStepShell from './OnboardingStepShell.svelte'
    import SectionCard from '$lib/ui/SectionCard.svelte'
    import SettingRow from '$lib/settings/components/SettingRow.svelte'
    import SettingSwitch from '$lib/settings/components/SettingSwitch.svelte'
    import Checkbox from '$lib/ui/Checkbox.svelte'
    import LinkButton from '$lib/ui/LinkButton.svelte'
    import TextInput from '$lib/ui/TextInput.svelte'
    import StatusBadge from '$lib/ui/StatusBadge.svelte'
    import ShortcutChip from '$lib/ui/ShortcutChip.svelte'
    import { setFooterOverride, nextStep, requestWizardComplete } from './onboarding-state.svelte'
    import { forceSave, getSetting, getSettingDefinition, setSetting } from '$lib/settings'
    import { createBetaEmailSignup } from '$lib/settings/sections/beta-email-signup.svelte'
    import { openExternalUrl } from '$lib/tauri-commands'
    import {
        GITHUB_REPO_URL,
        GITHUB_ISSUES_URL,
        BOOK_A_CALL_URL,
        ABOUT_DAVID_URL,
        DISCORD_INVITE_URL,
    } from '$lib/beta-links'
    import { TERMS_URL, TERMS_VERSION } from '$lib/legal/terms'
    import { getFirstShortcutReactive } from '$lib/shortcuts/reactive-shortcuts.svelte'
    import { getAppLogger } from '$lib/logging/logger'
    import { tString } from '$lib/intl/messages.svelte'
    import Trans from '$lib/intl/Trans.svelte'
    import { dependOn } from '$lib/utils/reactivity'
    import type { Snippet } from 'svelte'

    /**
     * Step 3: Open beta disclosure.
     *
     * Four parts:
     *
     *   [Personal open-beta intro: David's first-person welcome + the three feedback
     *    channels (Help > Send feedback…, GitHub issues, book-a-call), linked through
     *    the shared `$lib/beta-links` constants]
     *   [Anonymous-analytics disclosure + an opt-out switch bound to `analytics.enabled`]
     *   [Optional contact email]
     *   [Required terms acceptance: the one gate on this page]
     *
     * The analytics + email parts reuse the exact wiring `settings/sections/UpdatesSection.svelte`
     * uses, so the Settings page and this onboarding page behave identically:
     *   - the opt-out switch is the registry-backed `<SettingSwitch id="analytics.enabled">`
     *     (default on; flipping it writes the setting immediately, like everywhere else),
     *   - the email field runs on the shared `createBetaEmailSignup()`: it persists to
     *     `analytics.email` on every keystroke (local only) and, on commit of a valid address,
     *     calls the typed `betaSignup` wrapper, which POSTs ONLY the email (never an install
     *     id) and returns a typed result mapped to a gentle inline note.
     *
     * This page is non-skippable: the AI step's forward button lands the user here. The
     * footer offers two ways forward: a secondary "Start using Cmdr!" that finishes
     * onboarding right here (skipping the optional setup), and a primary "One more optional
     * setup step" that advances to the final Optional step. Both are BLOCKED until the
     * terms checkbox is ticked. See `lib/onboarding/CLAUDE.md` § "Step 3 (Open beta)".
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

    // Terms acceptance: the one required control on this page. The checkbox is the user's
    // deliberate act of assent, and we record WHICH terms it applied to (see
    // `$lib/legal/terms`), so a later terms change can ask again instead of coasting on a
    // consent that was given to a different document.
    let termsAccepted = $state(false)
    /** The terms card's anchor, so a blocked click can bring it back on screen. */
    const TERMS_BLOCK_ID = 'onboarding-terms-block'
    /** Guards the mount-time read from overwriting a tick the user got in first. */
    let termsTouched = false

    onMount(() => {
        // Re-entry from the menu / palette (or a Back from the Optional step) shouldn't make
        // the user re-tick a box they already ticked. An acceptance of an OLDER version
        // doesn't count, which is the whole point of storing the version.
        if (termsTouched) return
        termsAccepted = getSetting('onboarding.termsAcceptedVersion') === TERMS_VERSION
    })

    function handleTermsChange(accepted: boolean): void {
        termsTouched = true
        termsAccepted = accepted
        // Write through immediately, like every other control on this page. Clearing the box
        // clears the record too: keeping a stale "accepted" after the user unticked it would
        // be a claim we can't back up.
        setSetting('onboarding.termsAcceptedVersion', accepted ? TERMS_VERSION : '')
        setSetting('onboarding.termsAcceptedAt', accepted ? new Date().toISOString() : '')
        // Consent is a record of something that happened, so don't leave it to the save
        // debounce: a quit right after ticking would lose it and ask again next launch.
        void forceSave().then((saved) => {
            if (!saved) log.warn('Could not persist the terms acceptance; the beta step may ask again')
        })
    }

    /**
     * Bring the terms checkbox back on screen and put focus on it. This is what a press on a
     * blocked footer button does, for pointer and keyboard alike: the button is far from the
     * checkbox, and a press that neither advances nor explains reads as a broken app.
     */
    function revealTermsCheckbox(): void {
        const block = document.getElementById(TERMS_BLOCK_ID)
        if (!block) return
        const reduceMotion =
            typeof window !== 'undefined' &&
            typeof window.matchMedia === 'function' &&
            window.matchMedia('(prefers-reduced-motion: reduce)').matches
        block.scrollIntoView({ block: 'center', behavior: reduceMotion ? 'auto' : 'smooth' })
        // Focus the control itself, not just the region: a scroll alone leaves a keyboard
        // user's caret back on the footer with nothing to act on.
        //
        // ❌ `preventScroll` is load-bearing. Ark's hidden 1x1 input is off screen when we
        // get here, so a plain `focus()` runs its own scroll-into-view, which cancels the
        // smooth scroll above and leaves the step where it started. The press then does
        // nothing visible and focus sits on a control the user can't see, which reads as a
        // dead button and an unclickable checkbox (measured in the app: 4 runs out of 4,
        // and only when the input wasn't already focused, which is why it looked flaky).
        block.querySelector<HTMLInputElement>('input[type="checkbox"]')?.focus({ preventScroll: true })
    }

    // Guards a double-trigger while the step tears down. Both handlers are synchronous, so
    // this only matters for a rapid double-click on the same button.
    let advanceBusy = $state(false)

    function handleStart(): void {
        if (advanceBusy) return
        if (!termsAccepted) {
            revealTermsCheckbox()
            return
        }
        advanceBusy = true
        // Finish onboarding right here, skipping the optional setup step.
        requestWizardComplete()
    }

    function handleContinue(): void {
        if (advanceBusy) return
        if (!termsAccepted) {
            revealTermsCheckbox()
            return
        }
        advanceBusy = true
        nextStep()
    }

    // Re-register on `advanceBusy` / `termsAccepted` change so the blocked state stays fresh.
    $effect(() => {
        dependOn(advanceBusy)
        // `blockedReason` (not `disabled`) so the press still reaches the handler above.
        const blockedReason = termsAccepted ? undefined : tString('onboarding.stepBeta.terms.blockedTooltip')
        setFooterOverride([
            {
                label: tString('onboarding.stepBeta.footer.start'),
                variant: 'secondary',
                disabled: advanceBusy,
                blockedReason,
                onclick: () => {
                    handleStart()
                },
            },
            {
                label: tString('onboarding.stepBeta.footer.continue'),
                variant: 'primary',
                disabled: advanceBusy,
                blockedReason,
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

    // The beta contact email field, on the same logic as `UpdatesSection.svelte`.
    const emailSignup = createBetaEmailSignup()
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
{#snippet terms(children: Snippet)}<LinkButton
        href={TERMS_URL}
        target="_blank"
        rel="noopener noreferrer"
        onclick={openLink(TERMS_URL)}>{@render children()}</LinkButton
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
    <!-- Each row's sentence lives in ONE span. The `<li>` is a flex row (marker + text),
         and flex makes every ELEMENT child its own item: without the span, the leading
         `<LinkButton>` would be separated from the ": …" after it by the row's own gap,
         and the list read "GitHub : Add issues". -->
    <ol class="feedback-list">
        <li>
            <span class="feedback-text">
                {#if commandPaletteShortcut}
                    <Trans key="onboarding.stepBeta.feedback.inAppBound" snippets={{ strong, chip }} />
                {:else}
                    <Trans key="onboarding.stepBeta.feedback.inAppUnbound" snippets={{ strong }} />
                {/if}
            </span>
        </li>
        <li>
            <span class="feedback-text"><Trans key="onboarding.stepBeta.feedback.github" snippets={{ github }} /></span>
        </li>
        <li>
            <span class="feedback-text"><Trans key="onboarding.stepBeta.feedback.discord" snippets={{ discord }} /></span>
        </li>
        <li>
            <span class="feedback-text"><Trans key="onboarding.stepBeta.feedback.call" snippets={{ call }} /></span>
        </li>
    </ol>
    <p class="lede"><Trans key="onboarding.stepBeta.star" snippets={{ github: repoLink, code }} /></p>

    <p class="lede analytics-lede">{tString('onboarding.stepBeta.analyticsLede')}</p>

    <SectionCard>
        <SettingRow
            id="analytics.enabled"
            label={tString('onboarding.stepBeta.analyticsTitle')}
            description={analyticsDef.description}
        >
            <SettingSwitch id="analytics.enabled" />
        </SettingRow>
        <p class="card-note">{tString('onboarding.stepBeta.analyticsCaption')}</p>
    </SectionCard>

    <!-- Crash reports default on too, and a default that sends something has to be disclosed
         where the analytics one is, not only in Settings. No toggle: the switch lives in
         Settings > Updates & privacy, and this step already asks enough of a first launch. -->
    <p class="lede crash-reports-note">{tString('onboarding.stepBeta.crashReportsNote')}</p>

    <SectionCard label={tString('onboarding.stepBeta.emailTitle')}>
        <TextInput
            type="email"
            placeholder={tString('onboarding.stepBeta.emailPlaceholder')}
            value={emailSignup.email}
            oninput={emailSignup.handleInput}
            onblur={emailSignup.handleCommit}
            onkeydown={emailSignup.handleKeydown}
            disabled={emailSignup.signupInFlight}
            ariaLabel={tString('onboarding.stepBeta.emailTitle')}
        />
        {#if emailSignup.signupFeedback?.kind === 'success'}
            <p class="signup-feedback success" role="status">{tString('onboarding.stepBeta.signup.success')}</p>
        {:else if emailSignup.signupFeedback?.kind === 'failure'}
            <p class="signup-feedback failure" role="status">{tString('onboarding.stepBeta.signup.failure')}</p>
        {/if}
        <p class="card-note">{tString('onboarding.stepBeta.emailNote')}</p>
    </SectionCard>

    <!-- The card carries the id a blocked footer press scrolls back to; that's what
         `SectionCard`'s `id` is for. -->
    <SectionCard id={TERMS_BLOCK_ID} label={tString('onboarding.stepBeta.terms.title')}>
        {#snippet badge()}
            <!-- Decoration; `required` on the checkbox is what a screen reader hears. -->
            <span class="required-mark" aria-hidden="true">*</span>
        {/snippet}
        <p class="card-note">{tString('onboarding.stepBeta.terms.lede')}</p>
        <div class="terms-consent">
            <Checkbox checked={termsAccepted} required onCheckedChange={handleTermsChange}>
                <Trans key="onboarding.stepBeta.terms.consent" snippets={{ terms }} />
            </Checkbox>
        </div>
    </SectionCard>
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
        line-height: var(--font-line-height-prose);
        color: var(--color-text-primary);
    }

    /* Keep the inline ALPHA badge centered on the text baseline run rather than riding high. */
    .lede :global(.feature-status-badge) {
        vertical-align: middle;
    }

    .analytics-lede {
        margin-bottom: var(--spacing-lg);
    }

    .crash-reports-note {
        margin-top: var(--spacing-lg);
        margin-bottom: var(--spacing-lg);
    }

    /* The list belongs to the paragraphs around it, so its numbers start on the same
       left edge they do; only a wrapped line hangs in under the words.
       ❌ Not `text-indent`, which is INHERITED: it reaches into every inline-flex
       descendant's anonymous item and yanked the `ShortcutChip` keys out of their pill.
       A counter plus a flex row keeps the effect inside the row that asked for it. */
    .feedback-list {
        margin: 0 0 var(--spacing-lg);
        padding-left: 0;
        list-style: none;
        counter-reset: feedback-item;
        line-height: var(--font-line-height-prose);
        color: var(--color-text-primary);
    }

    .feedback-list li {
        display: flex;
        gap: var(--spacing-xs);
        margin-bottom: var(--spacing-xs);
        counter-increment: feedback-item;
    }

    .feedback-list li::before {
        content: counter(feedback-item) '.';
        flex: none;
    }

    .feedback-text {
        min-width: 0;
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

    /* A quieter line inside a card: the note under the analytics switch, the email
       small print, the terms lede. */
    .card-note {
        margin: 0;
        font-size: var(--font-size-sm);
        line-height: var(--font-line-height-prose);
        color: var(--color-text-secondary);
    }

    /* The row above already ends on its own divider, so the note only needs air. */
    :global(.setting-row) + .card-note {
        margin-top: var(--spacing-sm);
    }

    .terms-consent {
        margin-top: var(--spacing-md);
        line-height: var(--font-line-height-prose);
    }

    /* The required marker, sitting on the card's label. Red and set slightly apart from it,
       matching how forms everywhere mark a required field; `aria-hidden` keeps it out of the
       a11y tree, where the control's own `aria-required` carries the same fact. The negative
       margin cancels the header's own gap, which is sized for a real badge, not a glyph that
       belongs to the last letter of the label. */
    .required-mark {
        margin-left: calc(var(--spacing-xxs) - var(--spacing-sm));
        color: var(--color-error-text);
    }

    .signup-feedback {
        margin: var(--spacing-xs) 0 0;
        font-size: var(--font-size-sm);
    }

    .signup-feedback.success {
        color: var(--color-toast-success-stripe);
    }

    .signup-feedback.failure {
        color: var(--color-text-primary);
    }

    /* The email field and its inline verdict both sit above the small print. */
    :global(.text-field) + .card-note,
    .signup-feedback + .card-note {
        margin-top: var(--spacing-sm);
    }
</style>
