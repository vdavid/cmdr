<script lang="ts">
    import { onMount, onDestroy } from 'svelte'
    import OnboardingStepShell from './OnboardingStepShell.svelte'
    import CloudProviderPicker from './CloudProviderPicker.svelte'
    import CloudProviderSetup from './CloudProviderSetup.svelte'
    import IconShieldCheck from '~icons/lucide/shield-check'
    import IconShieldOff from '~icons/lucide/shield-off'
    import IconTriangleAlert from '~icons/lucide/triangle-alert'
    import {
        getOnboardingState,
        setFooterOverride,
        setStepTwoBanner,
        nextStep,
        requestWizardComplete,
    } from './onboarding-state.svelte'
    import {
        checkFullDiskAccess,
        getAiRuntimeStatus,
        startAiDownload,
        cancelAiDownload,
        openPrivacySettings,
    } from '$lib/tauri-commands'
    import { systemStrings } from '$lib/system-strings.svelte'
    import { getSetting, setSetting, type AiProvider } from '$lib/settings'
    import { loadSettings } from '$lib/settings-store'
    import { pushConfigToBackend } from '$lib/settings/ai-config'
    import { tooltip } from '$lib/tooltip/tooltip'
    import LinkButton from '$lib/ui/LinkButton.svelte'
    import { getAppLogger } from '$lib/logging/logger'

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
     * Footer: dual buttons registered via `setFooterOverride` so the wizard renders
     * them in its right slot (secondary "Start using Cmdr!" + primary "One more
     * optional setup step"). Per David's spec the primary one nudges users into step 3
     * without forcing them.
     *
     * Persistence on either footer button:
     *   - `ai.provider` (always)
     *   - `ai.cloudProvider` + `ai.cloudProviderConfigs` (when cloud is picked; the API
     *     key is already persisted live by `CloudProviderSetup`)
     *   - `pushConfigToBackend()` (belt + braces; the applier listener also fires on the
     *     same setting changes, but we await this here so backend state is fresh by the
     *     time the user lands in the app)
     *
     * Local-pick side effect: kicks off `startAiDownload()` in the background so the
     * model is ready (or close to it) by the time the wizard closes. Switching away
     * from local within the same session cancels; the existing AI toast handles any
     * surfaced progress.
     *
     * **No-key-blocks-advance rule** (plan M3): if the user picks cloud but the
     * connection check hasn't gone green, both footer buttons stay enabled. The
     * auto-check status is right there as feedback; forcing key entry as a precondition
     * would fight users who want to come back later.
     */

    const log = getAppLogger('onboarding-step-ai')
    const onboardingState = getOnboardingState()

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
    })

    async function refreshFdaBanner(): Promise<void> {
        // Skip on Linux: the resume rule sets `linux` and there's no banner to render.
        if (onboardingState.stepTwoBanner === 'linux') return
        try {
            const [hasFda, settings] = await Promise.all([checkFullDiskAccess(), loadSettings()])
            if (hasFda) {
                setStepTwoBanner('granted')
            } else if (settings.fullDiskAccessChoice === 'deny') {
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

    async function persist(): Promise<void> {
        const provider: AiProvider = choice
        setSetting('ai.provider', provider)
        if (provider === 'cloud') {
            setSetting('ai.cloudProvider', cloudProviderId)
        }
        // Belt-and-braces: the applier listener fires on each setSetting above, but we
        // await this explicitly so the backend is reconfigured before the user lands in
        // the app. Per plan M3 § "Persist on click of either footer button".
        await pushConfigToBackend()
    }

    /**
     * Footer buttons exposed to the wizard via `setFooterOverride`. The closures need
     * access to the step's `onComplete` callback, which lives in the wizard. We
     * re-register on every relevant state change so the closures always see fresh
     * values (Svelte 5 closures capture by reference, but `choice` / `cloudProviderId`
     * are `$state` and would be stale through `untrack`).
     */
    $effect(() => {
        // Track the reactive bits the closures read so the effect re-runs.
        void choice
        void cloudProviderId
        void advanceBusy
        const buttons = [
            {
                label: 'Start using Cmdr!',
                variant: 'secondary' as const,
                disabled: advanceBusy,
                onclick: () => void handleStart(),
            },
            {
                label: 'One more optional setup step',
                variant: 'primary' as const,
                disabled: advanceBusy,
                onclick: () => void handleContinue(),
            },
        ]
        setFooterOverride(buttons)
    })

    async function handleStart(): Promise<void> {
        if (advanceBusy) return
        advanceBusy = true
        try {
            await persist()
        } finally {
            advanceBusy = false
        }
        // Skip step 3. The wizard observes `finishRequestTick` and calls `onComplete()`.
        requestWizardComplete()
    }

    async function handleContinue(): Promise<void> {
        if (advanceBusy) return
        advanceBusy = true
        try {
            await persist()
        } finally {
            advanceBusy = false
        }
        nextStep()
    }

    // ─────────────────────────────────────────────────────────────────────────────
    // Banner copy. See `lib/onboarding/CLAUDE.md` § "Step 2 (AI provider)" for the three FDA-outcome modes.
    // ─────────────────────────────────────────────────────────────────────────────

    const bannerTitleByMode = {
        granted: 'Full disk access granted',
        denied: 'No full disk access',
        stuck: "Cmdr doesn't seem to have full disk access yet",
        linux: '',
    } as const

    const localTooltip = 'Local LLM requires Apple Silicon. Cloud works on Intel.'
</script>

<OnboardingStepShell>
    {#if onboardingState.stepTwoBanner === 'granted'}
        <section class="banner banner-ok" role="status">
            <span class="banner-icon"><IconShieldCheck width="20" height="20" /></span>
            <div class="banner-body">
                <p class="banner-title">{bannerTitleByMode.granted}</p>
                <p>Thanks for granting full disk access! Now, the app can access your disk. Great!</p>
            </div>
        </section>
    {:else if onboardingState.stepTwoBanner === 'denied'}
        <section class="banner banner-info" role="status">
            <span class="banner-icon"><IconShieldOff width="20" height="20" /></span>
            <div class="banner-body">
                <p class="banner-title">{bannerTitleByMode.denied}</p>
                <p>
                    You chose not to enable full disk access. We respect that. You'll then shortly get a few permission
                    requests from macOS for Cmdr to access your Desktop, Downloads, and similar folders. Accept or reject
                    these at will. You can change all of this later in your {systemStrings.systemSettings}.
                </p>
            </div>
        </section>
    {:else if onboardingState.stepTwoBanner === 'stuck'}
        <section class="banner banner-warn" role="status">
            <span class="banner-icon"><IconTriangleAlert width="20" height="20" /></span>
            <div class="banner-body">
                <p class="banner-title">{bannerTitleByMode.stuck}</p>
                <p>
                    You said you wanted to enable full disk access, but Cmdr doesn't seem to have gotten it. You might
                    need to restart the app (do it now, we'll continue from here!), or go to your
                    <LinkButton
                        onclick={() => {
                            void openPrivacySettings().catch((error: unknown) => {
                                log.warn('openPrivacySettings() failed: {error}', { error })
                            })
                        }}
                    >
                        {systemStrings.systemSettings} &gt; Privacy &amp; Security &gt; Full Disk Access
                    </LinkButton>
                    and find Cmdr, or manually add it with the little "+" button at the bottom.
                </p>
            </div>
        </section>
    {/if}

    {#if onboardingState.stepTwoBanner === 'linux'}
        <h2 class="step-title">Welcome to Cmdr!</h2>
        <p class="step-subtitle">Let's set up AI.</p>
    {:else}
        <h2 class="step-title">Now, the last necessary step: AI stuff</h2>
    {/if}

    <p>
        Cmdr has a bunch of AI features that you <em>may</em> want and may not want. AI is a controversial topic these
        days.
    </p>

    <p>Here is how you do common actions with and without AI:</p>

    <table class="comparison">
        <thead>
            <tr>
                <th scope="col">Feature</th>
                <th scope="col">With AI</th>
                <th scope="col">Without AI</th>
            </tr>
        </thead>
        <tbody>
            <tr>
                <th scope="row">Search</th>
                <td>You say "my recent fish-related presentations", agent sets your filters.</td>
                <td>You type something like <code>*fish*.ppt</code>, and select the "after 1st of this month" filter.</td>
            </tr>
            <tr>
                <th scope="row">Mass-rename</th>
                <td>You say "add ISO date prefix", agent sets your rename pattern, you review and apply at will.</td>
                <td>You use the batch rename UI to manually set the rename pattern, review and apply.</td>
            </tr>
            <tr>
                <th scope="row">Select</th>
                <td>You say "select all image files", agent suggests a selection, you review and apply at will.</td>
                <td>
                    You press <kbd>⌘+</kbd> and type something like
                    <code>*.jpg,*.png,*.gif,*.heic,*.webp,*.jpeg</code>, review and apply.
                </td>
            </tr>
        </tbody>
    </table>

    {#if showResumeCue}
        <p class="resume-cue">You picked this last time. Confirm or change below.</p>
    {/if}

    <fieldset class="choices" role="radiogroup" aria-label="AI choice">
        <legend class="sr-only">Based on this, do you want AI or not?</legend>

        <label class="choice" class:active={choice === 'cloud'}>
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
                <strong>Yes, I want AI</strong>
                <span class="choice-recommended">(recommended)</span>
            </span>
            <span class="choice-help">
                Use any cloud provider with your own API key. Fast, high-quality models. Pick a provider below.
            </span>
        </label>

        {#if choice === 'cloud'}
            <div class="cloud-grid">
                <div class="cloud-grid-picker">
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

        <label
            class="choice"
            class:active={choice === 'local'}
            class:disabled={!localAiSupported}
        >
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
                <strong>Yes, I want AI, but I want to be super private</strong>
            </span>
            <span class="choice-help">
                A bit dumber model that takes up about 2 GB of space and a bit of CPU at every use. Still an okay
                solution. No data leaves your machine. Cmdr tries to deliver updates for the best small local model
                available.
            </span>
        </label>

        {#if choice === 'local' && didStartLocalDownload}
            <p class="local-note">
                Started downloading the local model in the background. You can finish onboarding now; the toast in the
                corner will keep you posted.
            </p>
        {/if}

        <label class="choice" class:active={choice === 'off'}>
            <input
                type="radio"
                name="onboarding-ai-choice"
                value="off"
                checked={choice === 'off'}
                onchange={() => {
                    handleChoiceChange('off')
                }}
            />
            <span class="choice-label"><strong>Thanks but no thanks, no AI for me</strong></span>
            <span class="choice-help">
                Cmdr works fully without AI. You can turn it on later in Settings.
            </span>
        </label>
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
        line-height: 1.5;
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
        line-height: 1.4;
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

    .comparison code,
    .comparison kbd {
        font-family: var(--font-mono);
        font-size: var(--font-size-xs);
        background: var(--color-bg-tertiary);
        padding: var(--spacing-xxs) var(--spacing-xs);
        border-radius: var(--radius-sm);
        color: var(--color-text-primary);
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
        line-height: 1.4;
    }

    .cloud-grid {
        display: grid;
        grid-template-columns: minmax(180px, 240px) 1fr;
        gap: var(--spacing-lg);
        margin: 0;
        padding: var(--spacing-md) 0 var(--spacing-sm);
        max-height: 340px;
        min-height: 0;
    }

    .cloud-grid-picker {
        min-height: 0;
        display: flex;
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
