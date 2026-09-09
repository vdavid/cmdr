<script lang="ts">
    import { onMount, onDestroy } from 'svelte'
    import OnboardingStepShell from './OnboardingStepShell.svelte'
    import CloudProviderPicker from './CloudProviderPicker.svelte'
    import CloudProviderSetup from './CloudProviderSetup.svelte'
    import Icon from '$lib/ui/Icon.svelte'
    import {
        getOnboardingState,
        setFooterOverride,
        setFooterNote,
        setStepTwoBanner,
        nextStep,
    } from './onboarding-state.svelte'
    import {
        checkFullDiskAccess,
        getAiApiKeyStatus,
        getAiRuntimeStatus,
        startAiDownload,
        cancelAiDownload,
        openPrivacySettings,
    } from '$lib/tauri-commands'
    import { systemStrings } from '$lib/system-strings.svelte'
    import { getCloudProvider, getSetting, setSetting, type AiProvider } from '$lib/settings'
    import { pushConfigToBackend } from '$lib/settings/ai-config'
    import { revokeConsent } from '$lib/ask-cmdr/ask-cmdr-consent.svelte'
    import { tooltip } from '$lib/tooltip/tooltip'
    import LinkButton from '$lib/ui/LinkButton.svelte'
    import ShortcutChip from '$lib/ui/ShortcutChip.svelte'
    import { getFirstShortcutReactive } from '$lib/shortcuts/reactive-shortcuts.svelte'
    import { getAppLogger } from '$lib/logging/logger'
    import { tString } from '$lib/intl/messages.svelte'
    import Trans from '$lib/intl/Trans.svelte'
    import { dependOn } from '$lib/utils/reactivity'
    import type { Snippet } from 'svelte'

    /**
     * Step 2: AI provider picker.
     *
     * Layout:
     *
     *   [FDA-outcome banner]      ← three branches: granted, denied, stuck (Linux: hidden)
     *   [Comparison table]        ← David's verbatim "with AI vs without" copy
     *   [Three radio choices]
     *     ○ Yes, cloud + [picker | setup]
     *     ○ Yes, local (Apple Silicon only; tooltip when disabled)
     *     ○ Thanks but no thanks
     *
     * Footer: a single forward button ("Next") registered via `setFooterOverride` so the
     * wizard renders it in its right slot. The Beta page (step 3) is non-skippable, so this
     * step never completes onboarding; it always advances to Beta.
     *
     * Persistence on the footer button:
     *   - `ai.provider` (always)
     *   - `ai.cloudProvider` + `ai.cloudProviderConfigs` (when cloud is picked; the API
     *     key is already persisted live by `CloudProviderSetup`)
     *   - on 'off' only: Ask Cmdr consent revoked + `askCmdr.proactive = false`, so the
     *     answer lands on all three pieces of state that say AI is on (see `persist()`
     *     and DETAILS § "What 'off' turns off")
     *   - `pushConfigToBackend()` (belt + braces; the applier listener also fires on the
     *     same setting changes, but we await this here so backend state is fresh by the
     *     time the user lands in the app)
     *
     * Local-pick side effect: kicks off `startAiDownload()` in the background so the
     * model is ready (or close to it) by the time the wizard closes. Switching away
     * from local within the same session cancels; the existing AI toast handles any
     * surfaced progress.
     *
     * **No-key-blocks-advance rule**: if the user picks cloud but the connection check
     * hasn't gone green, the forward button stays enabled. The auto-check status is
     * right there as feedback; forcing key entry as a precondition would fight users
     * who want to come back later.
     */

    const log = getAppLogger('onboarding-step-ai')
    const onboardingState = getOnboardingState()

    // Drives the "Select" comparison cell: when `selection.selectFiles` is unbound the
    // chip renders nothing, so we swap to a wording that names the action instead.
    const selectFilesShortcut = $derived(getFirstShortcutReactive('selection.selectFiles'))

    type WizardChoice = 'cloud' | 'local' | 'off'

    // Pre-select from the persisted provider so a crash-then-resume user sees their last pick.
    function initialChoice(): WizardChoice {
        const provider = getSetting('ai.provider')
        if (provider === 'cloud') return 'cloud'
        if (provider === 'local') return 'local'
        return 'off'
    }

    let choice = $state<WizardChoice>(initialChoice())
    let cloudProviderId = $state<string>(getSetting('ai.cloudProvider'))
    let localAiSupported = $state<boolean>(true)
    let didStartLocalDownload = $state(false)
    let showResumeCue = $state(false)
    let advanceBusy = $state(false)

    /**
     * Track whether `local` was the user's choice when they leave the step. The wizard
     * keeps the backend download running across step transitions; only switching back
     * to non-local inside the same wizard session cancels. Not reactive (we only need
     * the previous value at `handleChoiceChange` time).
     */
    let previousChoice: WizardChoice = choice

    onMount(() => {
        // Show the passive resume cue if the user previously picked something other than
        // 'off' (per plan § "Step persistence resume, edge case"). `isOnboarded` lives
        // in settings-store; we can't read it sync here without churn, so we infer
        // "user resumed mid-wizard" from "the persisted provider isn't the default".
        showResumeCue = choice !== 'off'

        // One-shot FDA probe to pick the right banner branch (the resume-rule precomputed
        // banner may be stale if the user toggled FDA in Settings between sessions).
        // Linux short-circuits in the resume rule, no probe needed.
        void refreshFdaBanner()

        // Intel-Mac gate: disable local radio if the runtime says so. Default to
        // supported = true so a slow / failing IPC doesn't lock Apple Silicon users out.
        void refreshLocalSupport()
    })

    onDestroy(() => {
        // Clear the footer override so the wizard's default buttons render on other steps
        // (and so a teardown-then-remount doesn't leak stale closures).
        setFooterOverride(null)
        dismissKeyWarning()
    })

    /**
     * The missing-API-key warning is a CONFIRM-ONCE gate, not a block: the first Next
     * with cloud picked and no key stored stops and says so, the second one goes through.
     * That keeps the no-key-blocks-advance rule (a user who wants to paste the key later
     * still gets out of the step) while making sure nobody sails past a half-configured
     * AI setup without being told.
     *
     * `true` means the warning is up and the next press is the confirmation.
     */
    let keyWarningShown = $state(false)
    let dismissKeyWarning: () => void = () => undefined

    /**
     * Show the warning and arm the "anything the user does next clears it" listeners.
     *
     * Events inside the wizard FOOTER are exempt, and that exemption is what makes the
     * confirm-once flow work at all: pressing Next again (pointer or keyboard) has to
     * reach `handleGoToBeta` with the flag still set. Keyboard is why the exemption can't
     * be "ignore the click that opened this" — Enter on the button fires `keydown` BEFORE
     * the click, so a blanket clear would disarm the gate a beat before the press it
     * belongs to.
     *
     * Capture phase, on `document`: a step control that stops propagation (the provider
     * listbox does) still counts as the user moving on.
     */
    function showKeyWarning(): void {
        keyWarningShown = true
        setFooterNote(tString('onboarding.stepAi.missingKeyWarning'))

        const clear = (event: Event): void => {
            const target = event.target
            if (target instanceof Element && target.closest('.wizard-footer') !== null) return
            dismissKeyWarning()
        }
        document.addEventListener('click', clear, true)
        document.addEventListener('keydown', clear, true)

        dismissKeyWarning = () => {
            document.removeEventListener('click', clear, true)
            document.removeEventListener('keydown', clear, true)
            dismissKeyWarning = () => undefined
            keyWarningShown = false
            setFooterNote(null)
        }
    }

    /**
     * True when the user picked cloud AI, the chosen provider needs a key, and none is
     * stored for it. Reads the OS secret store rather than any local field: a key the
     * user pasted earlier in the session is already persisted there, and one is never
     * read back into the window (`docs/security.md` § "AI API keys").
     */
    async function isMissingCloudApiKey(): Promise<boolean> {
        if (choice !== 'cloud') return false
        if (!(getCloudProvider(cloudProviderId)?.requiresApiKey ?? false)) return false
        try {
            const status = await getAiApiKeyStatus(cloudProviderId)
            return !status.isSet
        } catch (error) {
            // Don't turn a failed keychain read into a blocked wizard.
            log.warn('getAiApiKeyStatus() before advancing failed: {error}', { error })
            return false
        }
    }

    async function refreshFdaBanner(): Promise<void> {
        // Skip on Linux: the resume rule sets `linux` and there's no banner to render.
        if (onboardingState.stepTwoBanner === 'linux') return
        try {
            const hasFda = await checkFullDiskAccess()
            if (hasFda) {
                // Celebrate the grant ONLY when the user just came through the FDA step this
                // launch (fresh first-run, not yet onboarded). On menu / palette re-entry after
                // onboarding finished, FDA being on is the steady state, not news, so show no
                // banner.
                setStepTwoBanner(getSetting('onboarding.completed') ? 'none' : 'granted')
            } else if (getSetting('onboarding.fullDiskAccessChoice') === 'deny') {
                setStepTwoBanner('denied')
            } else {
                setStepTwoBanner('stuck')
            }
        } catch (error) {
            log.warn('checkFullDiskAccess() on step 2 entry failed: {error}', { error })
        }
    }

    async function refreshLocalSupport(): Promise<void> {
        try {
            const status = await getAiRuntimeStatus()
            localAiSupported = status.localAiSupported
        } catch (error) {
            log.warn('getAiRuntimeStatus() failed; defaulting localAiSupported=true: {error}', { error })
        }
    }

    function handleChoiceChange(next: WizardChoice): void {
        if (next === choice) return
        // Switching away from local mid-wizard cancels the background download.
        // Switching back to local re-starts it (HTTP-Range resume picks up where we left off).
        if (previousChoice === 'local' && next !== 'local') {
            void cancelAiDownload().catch((error: unknown) => {
                log.warn("Couldn't cancel AI download on choice change: {error}", { error })
            })
        }
        previousChoice = next
        choice = next
        showResumeCue = false
        if (next === 'local' && localAiSupported) {
            startBackgroundDownload()
        }
    }

    function startBackgroundDownload(): void {
        didStartLocalDownload = true
        void startAiDownload().catch((error: unknown) => {
            log.warn("Couldn't start AI download: {error}", { error })
        })
    }

    /**
     * Commit the user's pick. "Thanks but no thanks" is the clearest answer a user can
     * give, so it lands on all THREE pieces of state that say AI is on, not only the
     * provider row: `ai.provider`, the Ask Cmdr consent record in `main.db`, and the
     * `askCmdr.proactive` setting (which ships ON).
     *
     * ❌ The reverse never happens: picking cloud or local ACCEPTS nothing. Consent is a
     * separate, explicit act behind the disclosure copy, and the backend enforces it in
     * the send path. Switching back off 'off' also leaves `askCmdr.proactive` off:
     * turning AI on again shouldn't silently re-arm an agent that starts conversations
     * on its own.
     */
    async function persist(): Promise<void> {
        const provider: AiProvider = choice
        setSetting('ai.provider', provider)
        if (provider === 'cloud') {
            setSetting('ai.cloudProvider', cloudProviderId)
        }
        if (provider === 'off') {
            setSetting('askCmdr.proactive', false)
            // Revoking also purges the proactive pipeline's stored rows in the backend,
            // which is the intent of an explicit "no". It's a no-op for someone who never
            // consented (the store just deletes two absent rows). Never fatal: a wizard
            // that traps the user because `main.db` hiccuped is worse than a logged warning.
            try {
                await revokeConsent()
            } catch (error) {
                log.warn("Couldn't turn Ask Cmdr off for a 'no AI' pick: {error}", { error })
            }
        }
        // Belt-and-braces: the applier listener fires on each setSetting above, but we
        // await this explicitly so the backend is reconfigured before the user lands in
        // the app. The forward button commits on click.
        await pushConfigToBackend()
    }

    /**
     * Forward footer button exposed to the wizard via `setFooterOverride`. We re-register
     * on every relevant state change so the closure always sees fresh `$state` values
     * (Svelte 5 closures capture by reference, but `choice` / `cloudProviderId` would be
     * stale through `untrack`). It persists the AI choice, then advances to the Beta page
     * (step 3). The Beta page is non-skippable, so there's no "skip to end" path here.
     */
    $effect(() => {
        // Track the reactive bits the closure reads so the effect re-runs.
        dependOn(choice, cloudProviderId, advanceBusy)
        setFooterOverride([
            {
                label: tString('onboarding.wizard.next'),
                variant: 'primary' as const,
                disabled: advanceBusy,
                onclick: () => void handleGoToBeta(),
            },
        ])
    })

    async function handleGoToBeta(): Promise<void> {
        if (advanceBusy) return
        advanceBusy = true
        try {
            if (!keyWarningShown && (await isMissingCloudApiKey())) {
                showKeyWarning()
                return
            }
            await persist()
        } catch (error) {
            // The wizard never traps the user on a step (same reasoning as the
            // no-key-blocks-advance rule). A persist that fell over is worth a log line,
            // not a dead end: Settings is the way to fix whatever didn't land.
            log.warn("Couldn't persist the AI choice: {error}", { error })
        } finally {
            advanceBusy = false
        }
        dismissKeyWarning()
        nextStep()
    }

    // Banner copy lives in the catalog (`onboarding.stepAi.bannerTitle.*` / `bannerBody.*`);
    // see `lib/onboarding/CLAUDE.md` § "Step 2 (AI provider)" for the three FDA-outcome modes.
    const localTooltip = $derived(tString('onboarding.stepAi.localTooltip'))
</script>

{#snippet em(children: Snippet)}<em>{@render children()}</em>{/snippet}
{#snippet code(children: Snippet)}<code>{@render children()}</code>{/snippet}
{#snippet chip(children: Snippet)}<ShortcutChip commandId="selection.selectFiles" clickable={false} />{@render children()}{/snippet}
{#snippet settingsLink(children: Snippet)}<LinkButton
        onclick={() => {
            void openPrivacySettings().catch((error: unknown) => {
                log.warn('openPrivacySettings() failed: {error}', { error })
            })
        }}>{@render children()}</LinkButton
    >{/snippet}

<OnboardingStepShell>
    {#if onboardingState.stepTwoBanner === 'granted'}
        <section class="banner banner-ok" role="status">
            <span class="banner-icon"><Icon name="shield-check" size={20} /></span>
            <div class="banner-body">
                <p class="banner-title">{tString('onboarding.stepAi.bannerTitle.granted')}</p>
                <p>{tString('onboarding.stepAi.bannerBody.granted')}</p>
            </div>
        </section>
    {:else if onboardingState.stepTwoBanner === 'denied'}
        <section class="banner banner-info" role="status">
            <span class="banner-icon"><Icon name="shield-off" size={20} /></span>
            <div class="banner-body">
                <p class="banner-title">{tString('onboarding.stepAi.bannerTitle.denied')}</p>
                <p>
                    {tString('onboarding.stepAi.bannerBody.denied', {
                        systemSettings: systemStrings.systemSettings,
                    })}
                </p>
            </div>
        </section>
    {:else if onboardingState.stepTwoBanner === 'stuck'}
        <section class="banner banner-warn" role="status">
            <span class="banner-icon"><Icon name="triangle-alert" size={20} /></span>
            <div class="banner-body">
                <p class="banner-title">{tString('onboarding.stepAi.bannerTitle.stuck')}</p>
                <p>
                    <Trans
                        key="onboarding.stepAi.bannerBody.stuck"
                        snippets={{ settingsLink }}
                        params={{ systemSettings: systemStrings.systemSettings }}
                    />
                </p>
            </div>
        </section>
    {/if}

    {#if onboardingState.stepTwoBanner === 'linux'}
        <h2 class="step-title">{tString('onboarding.stepAi.welcomeLinux.title')}</h2>
        <p class="step-subtitle">{tString('onboarding.stepAi.welcomeLinux.subtitle')}</p>
    {:else}
        <h2 class="step-title">{tString('onboarding.stepAi.title')}</h2>
    {/if}

    <p><Trans key="onboarding.stepAi.intro" snippets={{ em }} /></p>

    <p>{tString('onboarding.stepAi.comparisonIntro')}</p>

    <table class="comparison">
        <thead>
            <tr>
                <!-- The row labels say what each row is, so a visible "Feature" heading over
                     them is noise. The header still EXISTS for screen readers, which announce
                     it with every cell in the column and would otherwise hear a blank. -->
                <th scope="col"><span class="sr-only">{tString('onboarding.stepAi.table.colFeature')}</span></th>
                <th scope="col">{tString('onboarding.stepAi.table.colWithout')}</th>
                <th scope="col" class="with-ai">
                    <span class="with-ai-head">
                        <Icon name="sparkles" size={14} />
                        {tString('onboarding.stepAi.table.colWith')}
                    </span>
                </th>
            </tr>
        </thead>
        <tbody>
            <tr>
                <th scope="row">
                    <span class="row-head">
                        <span class="row-head-icon"><Icon name="search" size={14} aria-hidden="true" /></span>
                        {tString('onboarding.stepAi.table.rowSearch')}
                    </span>
                </th>
                <td><Trans key="onboarding.stepAi.table.searchWithout" snippets={{ code }} /></td>
                <td class="with-ai">{tString('onboarding.stepAi.table.searchWith')}</td>
            </tr>
            <tr>
                <th scope="row">
                    <span class="row-head">
                        <span class="row-head-icon"><Icon name="pencil" size={14} aria-hidden="true" /></span>
                        {tString('onboarding.stepAi.table.rowRename')}
                    </span>
                </th>
                <td>{tString('onboarding.stepAi.table.renameWithout')}</td>
                <td class="with-ai">{tString('onboarding.stepAi.table.renameWith')}</td>
            </tr>
            <tr>
                <th scope="row">
                    <span class="row-head">
                        <span class="row-head-icon"><Icon name="list-checks" size={14} aria-hidden="true" /></span>
                        {tString('onboarding.stepAi.table.rowSelect')}
                    </span>
                </th>
                <td>
                    <!-- The chip reads the real `selection.selectFiles` binding (bare `+`),
                         not a hardcoded combo. Non-clickable: this is onboarding prose, and
                         deep-linking to Settings mid-wizard would be jarring. Worded "the … key"
                         so a bare `+` doesn't read as a key separator. The chip renders nothing
                         when the command is unbound, so the sentence falls back to naming the
                         action instead of leaving a gap where the key would sit. -->
                    {#if selectFilesShortcut}
                        <Trans key="onboarding.stepAi.table.selectWithoutBound" snippets={{ chip, code }} />
                    {:else}
                        <Trans key="onboarding.stepAi.table.selectWithoutUnbound" snippets={{ code }} />
                    {/if}
                </td>
                <td class="with-ai">{tString('onboarding.stepAi.table.selectWith')}</td>
            </tr>
        </tbody>
    </table>

    {#if showResumeCue}
        <p class="resume-cue">{tString('onboarding.stepAi.resumeCue')}</p>
    {/if}

    <fieldset class="choices" role="radiogroup" aria-label={tString('onboarding.stepAi.choiceGroupAria')}>
        <legend class="sr-only">{tString('onboarding.stepAi.choiceLegend')}</legend>

        <!-- Order runs off → local → cloud, cheapest commitment first, so all three cards
             fit above the fold and only the LAST one opens a provider panel underneath.
             Cloud is still the pre-selected default. -->
        <label class="choice" class:active={choice === 'off'}>
            <!-- eslint-disable-next-line cmdr/prefer-ui-primitive -- Bespoke radio-cards: each option is a rich card (label + help text), which a plain RadioGroup option list can't express; already keyboard-accessible via the fieldset radiogroup. -->
            <input
                type="radio"
                name="onboarding-ai-choice"
                value="off"
                checked={choice === 'off'}
                onchange={() => {
                    handleChoiceChange('off')
                }}
            />
            <span class="choice-label"><strong>{tString('onboarding.stepAi.off.label')}</strong></span>
            <span class="choice-help">{tString('onboarding.stepAi.off.help')}</span>
        </label>

        <label
            class="choice"
            class:active={choice === 'local'}
            class:disabled={!localAiSupported}
        >
            <!-- eslint-disable-next-line cmdr/prefer-ui-primitive -- Bespoke radio-cards: each option is a rich card (label, help text, disabled/tooltip states), which a plain RadioGroup option list can't express; already keyboard-accessible via the fieldset radiogroup. -->
            <input
                type="radio"
                name="onboarding-ai-choice"
                value="local"
                checked={choice === 'local'}
                disabled={!localAiSupported}
                onchange={() => {
                    handleChoiceChange('local')
                }}
            />
            <span
                class="choice-label"
                use:tooltip={!localAiSupported ? localTooltip : undefined}
            >
                <strong>{tString('onboarding.stepAi.local.label')}</strong>
            </span>
            <span class="choice-help">{tString('onboarding.stepAi.local.help')}</span>
        </label>

        {#if choice === 'local' && didStartLocalDownload}
            <p class="local-note">{tString('onboarding.stepAi.local.note')}</p>
        {/if}

        <label class="choice" class:active={choice === 'cloud'}>
            <!-- eslint-disable-next-line cmdr/prefer-ui-primitive -- Bespoke radio-cards: each option is a rich card (label, "recommended" tag, help text, and an inline provider picker), which a plain RadioGroup option list can't express; already keyboard-accessible via the fieldset radiogroup. -->
            <input
                type="radio"
                name="onboarding-ai-choice"
                value="cloud"
                checked={choice === 'cloud'}
                onchange={() => {
                    handleChoiceChange('cloud')
                }}
            />
            <span class="choice-label">
                <strong>{tString('onboarding.stepAi.cloud.label')}</strong>
                <span class="choice-recommended">{tString('onboarding.stepAi.cloud.recommended')}</span>
            </span>
            <span class="choice-help">{tString('onboarding.stepAi.cloud.help')}</span>
        </label>

        {#if choice === 'cloud'}
            <div class="cloud-grid">
                <div class="cloud-grid-picker">
                    <h3 class="picker-title">{tString('onboarding.stepAi.cloud.pickerTitle')}</h3>
                    <CloudProviderPicker
                        value={cloudProviderId}
                        onChange={(id: string) => {
                            cloudProviderId = id
                            setSetting('ai.cloudProvider', id)
                        }}
                    />
                </div>
                <div class="cloud-grid-setup">
                    <CloudProviderSetup providerId={cloudProviderId} />
                </div>
            </div>
        {/if}
    </fieldset>
</OnboardingStepShell>

<style>
    .step-title {
        margin: var(--spacing-lg) 0 var(--spacing-md);
        /* 20% larger than body font (same calc() as StepFda/.welcome and
           StepOptional/.step-title so all onboarding step headings match). */
        font-size: calc(var(--font-size-md) * 1.2);
        font-weight: 600;
        color: var(--color-text-primary);
    }

    .step-subtitle {
        margin: calc(-1 * var(--spacing-sm)) 0 var(--spacing-md);
        font-size: var(--font-size-md);
        color: var(--color-text-secondary);
    }

    p {
        margin: 0 0 var(--spacing-md);
        line-height: var(--font-line-height-prose);
    }

    .banner {
        display: flex;
        align-items: flex-start;
        gap: var(--spacing-md);
        padding: var(--spacing-md) var(--spacing-lg);
        border-radius: var(--radius-md);
        border: 1px solid var(--color-border);
        background: var(--color-bg-tertiary);
    }

    .banner-ok {
        border-color: color-mix(in srgb, var(--color-allow), transparent 60%);
        background: color-mix(in srgb, var(--color-allow), transparent 92%);
    }

    .banner-info {
        border-color: var(--color-border);
        background: var(--color-bg-tertiary);
    }

    .banner-warn {
        border-color: color-mix(in srgb, var(--color-warning), transparent 60%);
        background: var(--color-warning-bg);
    }

    .banner-icon {
        display: inline-flex;
        flex-shrink: 0;
        align-items: center;
        justify-content: center;
        width: 24px;
        height: 24px;
        color: var(--color-text-secondary);
    }

    .banner-ok .banner-icon {
        color: var(--color-allow);
    }

    .banner-warn .banner-icon {
        color: var(--color-warning);
    }

    .banner-body {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xs);
        min-width: 0;
    }

    .banner-title {
        margin: 0;
        font-weight: 600;
        color: var(--color-text-primary);
    }

    .banner-body p {
        margin: 0;
    }

    .comparison {
        width: 100%;
        margin: 0 0 var(--spacing-lg);
        border-collapse: collapse;
        font-size: var(--font-size-sm);
    }

    .comparison th,
    .comparison td {
        padding: var(--spacing-sm) var(--spacing-md);
        text-align: left;
        vertical-align: top;
        border-bottom: 1px solid var(--color-border-subtle);
    }

    .comparison thead th {
        font-weight: 600;
        color: var(--color-text-secondary);
        border-bottom: 1px solid var(--color-border);
    }

    .comparison tbody th {
        font-weight: 500;
        color: var(--color-text-primary);
        white-space: nowrap;
    }

    .comparison code {
        font-family: var(--font-mono);
        font-size: var(--font-size-xs);
        background: var(--color-bg-tertiary);
        padding: var(--spacing-xxs) var(--spacing-xs);
        border-radius: var(--radius-sm);
        color: var(--color-text-primary);
    }

    /* "With AI" is the column we want to draw the eye to: tint it with the accent, give the
       header an accent color and a sparkle. The code chips inside still read on the tint. */
    .comparison th.with-ai,
    .comparison td.with-ai {
        background: var(--color-accent-subtle);
    }

    .comparison thead th.with-ai {
        color: var(--color-accent-text);
    }

    .with-ai-head {
        display: inline-flex;
        align-items: center;
        gap: var(--spacing-xxs);
    }

    /* The glyph carries the row's meaning at a glance; the label right after it is what
       actually names the row, so the icon stays decorative (`aria-hidden`) and one step
       quieter than the label it sits beside. */
    .row-head {
        display: inline-flex;
        align-items: center;
        gap: var(--spacing-xs);
    }

    .row-head-icon {
        display: inline-flex;
        flex: none;
        color: var(--color-text-secondary);
    }

    .resume-cue {
        margin: 0 0 var(--spacing-md);
        padding: var(--spacing-sm) var(--spacing-md);
        border-radius: var(--radius-sm);
        background: var(--color-accent-subtle);
        color: var(--color-text-primary);
        font-size: var(--font-size-sm);
    }

    .choices {
        border: none;
        padding: 0;
        margin: 0;
        display: flex;
        flex-direction: column;
        gap: var(--spacing-md);
    }

    .choice {
        display: grid;
        grid-template-columns: auto 1fr;
        grid-template-rows: auto auto;
        column-gap: var(--spacing-sm);
        row-gap: var(--spacing-xs);
        padding: var(--spacing-md) var(--spacing-lg);
        border: 1px solid var(--color-border);
        border-radius: var(--radius-md);
        background: var(--color-bg-primary);
        transition: border-color var(--transition-base), background var(--transition-base);
    }

    .choice:hover:not(.disabled) {
        border-color: var(--color-border-strong);
    }

    .choice.active {
        border-color: var(--color-accent);
        background: var(--color-accent-subtle);
    }

    .choice.disabled {
        opacity: 0.5;
    }

    .choice input[type='radio'] {
        grid-row: 1 / span 2;
        align-self: center;
        margin: 0;
    }

    .choice-label {
        grid-column: 2;
        grid-row: 1;
        display: inline-flex;
        align-items: baseline;
        gap: var(--spacing-xs);
        font-size: var(--font-size-md);
        color: var(--color-text-primary);
    }

    .choice-recommended {
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
    }

    .choice-help {
        grid-column: 2;
        grid-row: 2;
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
    }

    .cloud-grid {
        display: grid;
        grid-template-columns: minmax(180px, 240px) 1fr;
        gap: var(--spacing-lg);
        margin: 0;
        padding: var(--spacing-md) 0 var(--spacing-sm);
        /* A DEFINITE height (not max-height) so the flex heights inside resolve and each
           column scrolls internally. With only max-height, the picker list grew to its
           content height and spilled over the radio options below it (the overlap bug). */
        height: 22rem;
        flex: none;
        min-height: 0;
    }

    .cloud-grid-picker {
        min-height: 0;
        display: flex;
        flex-direction: column;
        gap: var(--spacing-sm);
    }

    /* Lines up horizontally with `CloudProviderSetup`'s "Set up {provider}" title: same
       font size / weight, both sit at the top of their grid column. */
    .picker-title {
        margin: 0;
        font-size: var(--font-size-md);
        font-weight: 600;
        color: var(--color-text-primary);
    }

    .cloud-grid-setup {
        min-height: 0;
        overflow-y: auto;
    }

    .local-note {
        margin: calc(-1 * var(--spacing-xs)) 0 0;
        padding: var(--spacing-sm) var(--spacing-md);
        border-radius: var(--radius-sm);
        background: var(--color-bg-tertiary);
        color: var(--color-text-secondary);
        font-size: var(--font-size-sm);
    }

    /* `.sr-only` is defined globally in `app.css`; no scoped override needed. */
</style>
