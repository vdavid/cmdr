<script lang="ts">
    import { onDestroy, onMount } from 'svelte'
    import OnboardingStepShell from './OnboardingStepShell.svelte'
    import SectionCard from '$lib/ui/SectionCard.svelte'
    import Button from '$lib/ui/Button.svelte'
    import Checkbox from '$lib/ui/Checkbox.svelte'
    import Icon from '$lib/ui/Icon.svelte'
    import InfoTip from '$lib/ui/InfoTip.svelte'
    import LinkButton from '$lib/ui/LinkButton.svelte'
    import TextInput from '$lib/ui/TextInput.svelte'
    import StatusBadge from '$lib/ui/StatusBadge.svelte'
    import ShortcutChip from '$lib/ui/ShortcutChip.svelte'
    import {
        setFooterOverride,
        nextStep,
        requestWizardComplete,
        getOnboardingState,
        setBetaChecklistItem,
        type BetaChecklistItem,
    } from './onboarding-state.svelte'
    import { forceSave, getSetting, getSettingDefinition, setSetting } from '$lib/settings'
    import { useBooleanSetting } from '$lib/settings/components/boolean-setting.svelte'
    import { createBetaEmailSignup } from '$lib/settings/sections/beta-email-signup.svelte'
    import { openExternalUrl } from '$lib/tauri-commands'
    import {
        GITHUB_REPO_URL,
        GITHUB_ISSUES_URL,
        BOOK_A_CALL_URL,
        ABOUT_DAVID_URL,
        DISCORD_INVITE_URL,
        ALTERNATIVE_TO_URL,
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
     * Three parts:
     *
     *   [David's first-person welcome and what "open beta" means here]
     *   [The open-beta checklist: usage stats, a GitHub star, an AlternativeTo like, an
     *    email address. Four small favors, each a tick, each about half a minute]
     *   [Required terms acceptance: the one gate on this page]
     *
     * The checklist is the whole middle of the page on purpose. Everything it asks for used
     * to be a paragraph of prose apiece, and the step read as a wall the user had to get
     * past rather than four things they could just do. Every row leads with a line and parks
     * its detail behind an `<InfoTip>`, and the analytics disclosure is a full four
     * paragraphs in there: an opt-out default has to be disclosed, but it doesn't have to be
     * the first thing on the screen.
     *
     * The two link rows tick themselves `CHECKLIST_TICK_DELAY_MS` after the click, since the
     * app can't see what happened in the browser; their ticks live in `onboarding-state` so a
     * Back and forward doesn't forget. Both are real checkboxes too, so someone who starred
     * the repo last week can just say so.
     *
     * The email field runs on the shared `createBetaEmailSignup()`, exactly as
     * `settings/sections/UpdatesSection.svelte` does: it persists to `analytics.email` on
     * every keystroke (local only) and, on commit of a valid address, calls the typed
     * `betaSignup` wrapper, which POSTs ONLY the email (never an install id).
     *
     * This page is non-skippable: the AI step's forward button lands the user here. The
     * footer offers two ways forward: a secondary "Start using Cmdr!" that finishes
     * onboarding right here (skipping the optional setup), and a primary "One more optional
     * setup step" that advances to the final Optional step. Both are BLOCKED until the
     * terms checkbox is ticked. See `lib/onboarding/CLAUDE.md` § "Step 3 (Open beta)".
     */

    /**
     * The feedback-channel list (in-app, GitHub issues, Discord, book-a-call). Parked rather
     * than deleted: the checklist took the middle of the page and the paragraph that used to
     * introduce this list ("here is how you can engage:") went with the rewrite, so the list
     * has nothing to hang off. Flip to `true` to bring it back.
     */
    const SHOW_FEEDBACK_CHANNELS = false
    /**
     * The "Stay in touch (optional)" card. Parked, not deleted: the checklist's inline email
     * field replaced it. Flip to `true` to bring it back.
     */
    const SHOW_STAY_IN_TOUCH_CARD = false

    /**
     * How long after following a checklist link before its row ticks itself. The app can't
     * see what happened in the browser, so this is a "you've had time to do it" delay, not a
     * confirmation: long enough that the tick doesn't land while the page is still opening,
     * short enough that the user is still looking at the row when it does.
     */
    const CHECKLIST_TICK_DELAY_MS = 3_000

    const log = getAppLogger('onboarding-beta')
    const onboardingState = getOnboardingState()

    const analyticsDef = getSettingDefinition('analytics.enabled') ?? { label: '', description: '' }
    /** The usage-stats row's tick IS the setting, on the same wiring `<SettingSwitch>` uses. */
    const analytics = useBooleanSetting('analytics.enabled')

    const statsLabel = $derived(tString('onboarding.stepBeta.analyticsTitle'))
    const starLabel = $derived(tString('onboarding.stepBeta.checklist.star'))
    const alternativeToLabel = $derived(tString('onboarding.stepBeta.checklist.alternativeTo'))

    /** Accessible name for a row's info glyph, which has no visible text of its own. */
    function moreAbout(topic: string): string {
        return tString('onboarding.moreAbout', { topic })
    }

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

    /**
     * Timers armed by `openAndTick`, cleared on destroy. A tick landing after the wizard
     * closed would write into state `closeWizard()` has already reset, so the next launch
     * would open on a checklist that ticks itself.
     */
    const tickTimers: number[] = []

    /**
     * Click handler for a checklist link: open the page, then tick the row once the user has
     * had time to act on it. Ticking on the click itself would claim they did something they
     * hadn't yet even seen.
     */
    function openAndTick(url: string, item: BetaChecklistItem) {
        const open = openLink(url)
        return (event: MouseEvent) => {
            open(event)
            tickTimers.push(
                window.setTimeout(() => {
                    setBetaChecklistItem(item, true)
                }, CHECKLIST_TICK_DELAY_MS),
            )
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
        for (const timer of tickTimers) window.clearTimeout(timer)
        tickTimers.length = 0
    })

    /**
     * The beta contact email field, on the same logic as `UpdatesSection.svelte` but with an
     * explicit Save: a checklist row that ticked itself as the user tabbed past would claim
     * they asked for something they only walked through. The tick follows the mailing list's
     * own answer, ❌ never a valid-looking address: `analytics.email` is written on every
     * keystroke, so a stored address proves nothing about whether it was ever sent.
     */
    const emailSignup = createBetaEmailSignup({
        commitOnBlur: false,
        onSubscribed: () => {
            setBetaChecklistItem('email', true)
        },
    })
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

<!-- The field and its Save button ride INSIDE the sentence, so the row reads as one line
     with a box in it. `baseline` on the wrapper is what puts the text inside the box on the
     same line as the words around it. -->
{#snippet emailField(children: Snippet)}<span class="email-control">
        <TextInput
            type="email"
            placeholder={tString('onboarding.stepBeta.emailPlaceholder')}
            value={emailSignup.email}
            oninput={emailSignup.handleInput}
            onblur={emailSignup.handleBlur}
            onkeydown={emailSignup.handleKeydown}
            invalid={emailSignup.showInvalid}
            disabled={emailSignup.signupInFlight}
            ariaLabel={tString('onboarding.stepBeta.emailTitle')}
        />
        <Button
            variant="secondary"
            size="mini"
            disabled={!emailSignup.canSubmit}
            onclick={() => {
                void emailSignup.handleCommit()
            }}>{tString('onboarding.stepBeta.checklist.emailSave')}</Button
        >
    </span>{@render children()}{/snippet}

<OnboardingStepShell>
    <h2 class="step-title">{tString('onboarding.stepBeta.title')}</h2>
    <p class="lede"><Trans key="onboarding.stepBeta.greeting" snippets={{ david }} /></p>
    <p class="lede"><Trans key="onboarding.stepBeta.openBeta" snippets={{ alpha }} /></p>

    {#if SHOW_FEEDBACK_CHANNELS}
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
                <span class="feedback-text"
                    ><Trans key="onboarding.stepBeta.feedback.github" snippets={{ github }} /></span
                >
            </li>
            <li>
                <span class="feedback-text"
                    ><Trans key="onboarding.stepBeta.feedback.discord" snippets={{ discord }} /></span
                >
            </li>
            <li>
                <span class="feedback-text"><Trans key="onboarding.stepBeta.feedback.call" snippets={{ call }} /></span>
            </li>
        </ol>
        <p class="lede"><Trans key="onboarding.stepBeta.star" snippets={{ github: repoLink, code }} /></p>
    {/if}

    <p class="lede checklist-title">{tString('onboarding.stepBeta.checklist.title')}</p>
    <!-- One grid, three columns (tick, glyph, text), with each row `display: contents` so
         all four line up on the same three edges however far the text wraps. -->
    <ul class="checklist">
        <li class="checklist-row">
            <Checkbox
                checked={analytics.checked}
                ariaLabel={statsLabel}
                onCheckedChange={(checked: boolean) => { analytics.set(checked); }}
            />
            <span class="row-glyph"><Icon name="chart-no-axes-column" size={16} aria-hidden="true" /></span>
            <span class="row-text">
                {statsLabel}
                <InfoTip label={moreAbout(statsLabel)}>
                    <p class="tip-para">{tString('onboarding.stepBeta.analyticsLede')}</p>
                    <p class="tip-para">{analyticsDef.description}</p>
                    <p class="tip-para">{tString('onboarding.stepBeta.analyticsCaption')}</p>
                    <!-- Crash reports default on too, and a default that sends something has to
                         be disclosed beside the analytics one, not only in Settings. No toggle:
                         that switch lives in Settings > Updates & privacy. -->
                    <p class="tip-para">{tString('onboarding.stepBeta.crashReportsNote')}</p>
                </InfoTip>
            </span>
        </li>

        <li class="checklist-row">
            <Checkbox
                checked={onboardingState.betaChecklist.star}
                ariaLabel={starLabel}
                onCheckedChange={(checked: boolean) => { setBetaChecklistItem('star', checked); }}
            />
            <span class="row-glyph"><Icon name="star" size={16} aria-hidden="true" /></span>
            <span class="row-text">
                <LinkButton
                    href={GITHUB_REPO_URL}
                    target="_blank"
                    rel="noopener noreferrer"
                    onclick={openAndTick(GITHUB_REPO_URL, 'star')}>{starLabel}</LinkButton
                >
                <span class="row-note"><Trans key="onboarding.stepBeta.checklist.starNote" snippets={{ code }} /></span>
            </span>
        </li>

        <li class="checklist-row">
            <Checkbox
                checked={onboardingState.betaChecklist.alternativeTo}
                ariaLabel={alternativeToLabel}
                onCheckedChange={(checked: boolean) => { setBetaChecklistItem('alternativeTo', checked); }}
            />
            <span class="row-glyph"><Icon name="heart" size={16} aria-hidden="true" /></span>
            <span class="row-text">
                <LinkButton
                    href={ALTERNATIVE_TO_URL}
                    target="_blank"
                    rel="noopener noreferrer"
                    onclick={openAndTick(ALTERNATIVE_TO_URL, 'alternativeTo')}>{alternativeToLabel}</LinkButton
                >
                <span class="row-note">{tString('onboarding.stepBeta.checklist.alternativeToNote')}</span>
            </span>
        </li>

        <li class="checklist-row">
            <!-- A progress mark, not a control: the field beside it is what ticks it, so it
                 takes no clicks. `disabled` is how that's said to a screen reader; the local
                 style below keeps it from also looking switched off. -->
            <span class="derived-mark">
                <Checkbox
                    checked={onboardingState.betaChecklist.email}
                    disabled
                    ariaLabel={tString('onboarding.stepBeta.checklist.emailMark')}
                />
            </span>
            <span class="row-glyph"><Icon name="mail" size={16} aria-hidden="true" /></span>
            <span class="row-text">
                <Trans key="onboarding.stepBeta.checklist.email" snippets={{ field: emailField }} />
                <InfoTip label={moreAbout(tString('onboarding.stepBeta.emailTitle'))}>
                    <p class="tip-para">{tString('onboarding.stepBeta.emailNote')}</p>
                </InfoTip>
                <!-- A setback says which one it was and what to do next: a typo and an
                     unreachable server want completely different moves from the user. -->
                {#if emailSignup.signupFeedback?.kind === 'success'}
                    <span class="signup-feedback success" role="status"
                        >{tString('onboarding.stepBeta.signup.success')}</span
                    >
                {:else if emailSignup.signupFeedback?.reason === 'invalidEmail'}
                    <span class="signup-feedback failure" role="status"
                        >{tString('onboarding.stepBeta.signup.rejected')}</span
                    >
                {:else if emailSignup.signupFeedback?.reason === 'unreachable'}
                    <span class="signup-feedback failure" role="status"
                        >{tString('onboarding.stepBeta.signup.unreachable')}</span
                    >
                {/if}
            </span>
        </li>
    </ul>

    {#if SHOW_STAY_IN_TOUCH_CARD}
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
            <p class="card-note">{tString('onboarding.stepBeta.emailNote')}</p>
        </SectionCard>
    {/if}

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

    .checklist-title {
        margin-top: var(--spacing-lg);
    }

    /* Three columns shared by all four rows (tick, glyph, text), so the rows line up on the
       same edges however far any of them wraps. Each `<li>` is `display: contents`, which
       makes its three children the grid's own items instead of one box per row. */
    .checklist {
        display: grid;
        grid-template-columns: auto auto 1fr;
        align-items: start;
        gap: var(--spacing-md) var(--spacing-sm);
        margin: 0 0 var(--spacing-xl);
        padding: 0;
        list-style: none;
    }

    .checklist-row {
        display: contents;
    }

    /* The tick and the glyph are 16px next to a ~21px line, so they sit a hair high without
       this. Both cells take the same nudge, so they stay level with each other. */
    .checklist :global(.checkbox-root),
    .row-glyph {
        margin-top: var(--spacing-xxs);
    }

    /* The field, its Save button, and the words around them share one baseline, so the row
       reads as a sentence with a box in it rather than a control dropped into a paragraph. */
    .email-control {
        display: inline-flex;
        align-items: baseline;
        gap: var(--spacing-sm);
        /* A little air on each side, so the box never touches the words. */
        margin: 0 var(--spacing-xs);
    }

    /* The stock field's padding is sized for a form row. Trimmed here so the email row ends
       up the same height as the three plain rows above it, which is what keeps the run of
       ticks evenly spaced. */
    .email-control :global(.text-field) {
        width: 14rem;
        padding: var(--spacing-xxs) var(--spacing-sm);
    }

    .row-glyph {
        display: flex;
        color: var(--color-accent-text);
    }

    .row-text {
        min-width: 0;
        line-height: var(--font-line-height-prose);
        color: var(--color-text-primary);
    }

    /* The explanation after a checklist link: same line, quieter, wrapping under it. */
    .row-note {
        color: var(--color-text-secondary);
    }

    .row-note code {
        font-family: var(--font-mono);
        font-size: var(--font-size-xs);
        background: var(--color-bg-tertiary);
        padding: var(--spacing-xxs) var(--spacing-xs);
        border-radius: var(--radius-sm);
        color: var(--color-text-primary);
    }

    /* A tick the user can't set by hand still shouldn't look switched off: `disabled` is
       here to say "not a control" to a screen reader, not to grey the mark out. */
    .derived-mark :global(.checkbox-control[data-disabled]) {
        opacity: 1;
        cursor: default;
    }

    /* One sentence per line inside the info tips, which is what the `\n`s in the catalog
       are for; a tooltip has the vertical room a paragraph of six sentences doesn't. */
    .tip-para {
        margin: 0 0 var(--spacing-md);
        font-size: var(--font-size-sm);
        line-height: var(--font-line-height-prose);
        color: var(--color-text-secondary);
        white-space: pre-line;
    }

    .tip-para:last-child {
        margin-bottom: 0;
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

    /* Its own line under the email row, so a verdict never squeezes in beside the field. */
    .signup-feedback {
        display: block;
        margin-top: var(--spacing-xs);
        font-size: var(--font-size-sm);
    }

    .signup-feedback.success {
        color: var(--color-toast-success-stripe);
    }

    .signup-feedback.failure {
        color: var(--color-text-primary);
    }

    /* The parked "Stay in touch" card stacks its field over its small print. */
    :global(.text-field) + .card-note {
        margin-top: var(--spacing-sm);
    }
</style>
