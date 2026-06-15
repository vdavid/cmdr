<script lang="ts">
    import { onMount, onDestroy, tick, untrack } from 'svelte'
    import { relaunch } from '@tauri-apps/plugin-process'
    import Icon from '$lib/ui/Icon.svelte'
    import { notifyDialogOpened, notifyDialogClosed } from '$lib/tauri-commands'
    import Button from '$lib/ui/Button.svelte'
    import { trapFocus } from '$lib/ui/focus-trap'
    import { tooltip } from '$lib/tooltip/tooltip'
    import { getAppLogger } from '$lib/logging/logger'
    import {
        getOnboardingState,
        ONBOARDING_STEP_COUNT,
        isAtFirstStep,
        isAtLastStep,
        nextStep,
        previousStep,
        openWizard,
        type OnboardingStep,
    } from './onboarding-state.svelte'
    import StepFda from './StepFda.svelte'
    import StepAi from './StepAi.svelte'
    import StepBeta from './StepBeta.svelte'
    import StepOptional from './StepOptional.svelte'

    const log = getAppLogger('onboarding')

    interface Props {
        /** Called when the user finishes the last step. Per-step persistence is wired
         *  inside each `Step*.svelte`; this callback only triggers the close + finalize. */
        onComplete: () => void
    }

    const { onComplete }: Props = $props()

    /**
     * The wizard panel. `tabindex=-1` lets us focus it on mount so keystrokes
     * land on our handler instead of the underlying app. Tab stays inside via
     * the shared `use:trapFocus` action on this element.
     */
    let panelEl: HTMLDivElement | undefined = $state()
    /**
     * Element that had focus when the wizard opened. Restored on destroy so
     * keyboard input flows back to wherever it came from after close.
     */
    let previousActiveElement: HTMLElement | null = null

    const onboardingState = getOnboardingState()

    onMount(async () => {
        previousActiveElement = document.activeElement instanceof HTMLElement ? document.activeElement : null

        // Open the wizard machine if it isn't already open. `+page.svelte` may have called
        // `openWizard()` itself; this guard makes the component safe to mount standalone too
        // (Vitest unit tests, future re-entry from menu / palette).
        if (onboardingState.currentStep === null) {
            openWizard('force')
        }

        void notifyDialogOpened('onboarding')

        // Wait for layout, then focus the panel so our keydown handler captures Tab.
        await tick()
        panelEl?.focus()
    })

    onDestroy(() => {
        void notifyDialogClosed('onboarding')
        if (previousActiveElement?.isConnected) {
            previousActiveElement.focus()
        }
    })

    /**
     * Tab trapping itself lives in the shared `use:trapFocus` action on the panel
     * (it queries focusables fresh on every Tab, so controls added mid-step — a
     * newly-revealed API-key input — join the cycle). No `onEscape` is passed:
     * the wizard is the only path for first-launch consent and the user shouldn't
     * be able to dismiss it without choosing.
     */
    function handleKeydown(event: KeyboardEvent): void {
        if (event.key === 'Escape') {
            // Wizard intentionally swallows Escape so the user can't dismiss it without
            // committing to a step. Stop propagation so other listeners (e.g. command
            // palette) don't see it either.
            event.preventDefault()
            event.stopPropagation()
        }
    }

    function handleBack(): void {
        previousStep()
    }

    async function handleRestart(): Promise<void> {
        try {
            await relaunch()
        } catch (error) {
            log.warn('relaunch() failed: {error}', { error })
        }
    }

    function handleNext(): void {
        if (untrack(() => isAtLastStep())) {
            onComplete()
            return
        }
        nextStep()
    }

    /**
     * Step bodies (the final Optional step's "Start using Cmdr" button) can ask the
     * wizard to finish. They bump `finishRequestTick` via `requestWizardComplete()`
     * and we react here. Using a tick counter (not a boolean) means repeated requests
     * in the same session still each fire exactly once.
     */
    let lastSeenFinishTick = 0
    $effect(() => {
        const tick = onboardingState.finishRequestTick
        if (tick === 0 || tick === lastSeenFinishTick) return
        lastSeenFinishTick = tick
        onComplete()
    })

    /**
     * Buttons to render in the footer's right slot. By default the wizard computes a
     * single per-step primary button (`Next`, `Finish`, `Restart Cmdr`, or nothing for
     * step 1's decide mode where the body owns Allow/Deny). Steps that own their footer
     * (the AI step's "Go to open beta", the Beta step's "Next", the Optional step's
     * "Start using Cmdr") register their own array via `setFooterOverride()` in
     * onboarding-state and we render those instead. Rendering `[]` leaves the slot empty.
     */
    type FooterButton = {
        label: string
        onclick: () => void
        variant: 'primary' | 'secondary' | 'danger'
        disabled?: boolean
        ariaLabel?: string
    }

    const footerButtons: FooterButton[] = $derived.by(() => computeFooterButtons())

    function computeFooterButtons(): FooterButton[] {
        if (onboardingState.footerOverride) {
            return onboardingState.footerOverride.map((b) => ({
                label: b.label,
                onclick: b.onclick,
                variant: b.variant,
                disabled: b.disabled,
                ariaLabel: b.ariaLabel,
            }))
        }
        const step = onboardingState.currentStep
        if (step === null) return []
        if (step === 1) {
            if (onboardingState.step1FooterMode === 'restart') {
                return [{ label: 'Restart Cmdr', onclick: () => void handleRestart(), variant: 'primary' }]
            }
            if (onboardingState.step1Variant === 'already-granted') {
                return [{ label: 'Next', onclick: handleNext, variant: 'primary' }]
            }
            // Step 1 decide mode: Allow + Deny live in the body; footer primary is hidden.
            return []
        }
        if (isAtLastStep()) {
            return [{ label: 'Finish', onclick: handleNext, variant: 'primary' }]
        }
        return [{ label: 'Next', onclick: handleNext, variant: 'primary' }]
    }

    /**
     * Step-dot indicator. The last step (Optional) is rendered with a muted/open style
     * so users see "mandatory steps plus one optional," not an endless wizard. The Beta
     * page (step 3) is a normal mandatory dot.
     */
    const stepDots = Array.from({ length: ONBOARDING_STEP_COUNT }, (_, i) => ({
        index: (i + 1) as OnboardingStep,
        isOptional: i === ONBOARDING_STEP_COUNT - 1,
    }))
</script>

<!-- No `onEscape` on the trap: the wizard must swallow Escape (see `handleKeydown`). -->
<div class="wizard-overlay" role="dialog" aria-modal="true" aria-labelledby="onboarding-wizard-title" use:trapFocus>
    <!-- Panel takes focus on mount so Esc routing lands here; trapFocus keeps Tab inside. -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
        bind:this={panelEl}
        class="wizard-panel"
        data-dialog-id="onboarding"
        tabindex="-1"
        onkeydown={handleKeydown}
    >
        <header class="wizard-header">
            <h2 id="onboarding-wizard-title" class="sr-only">Cmdr onboarding</h2>
            <ol class="step-dots" aria-label="Onboarding progress">
                {#each stepDots as dot (dot.index)}
                    <li
                        class="step-dot"
                        class:active={onboardingState.currentStep === dot.index}
                        class:optional={dot.isOptional}
                        aria-current={onboardingState.currentStep === dot.index ? 'step' : undefined}
                    >
                        <span class="sr-only">
                            Step {dot.index} of {ONBOARDING_STEP_COUNT}{dot.isOptional ? ' (optional)' : ''}
                        </span>
                    </li>
                {/each}
            </ol>
        </header>

        <div class="wizard-body">
            {#if onboardingState.currentStep === 1}
                <StepFda />
            {:else if onboardingState.currentStep === 2}
                <StepAi />
            {:else if onboardingState.currentStep === 3}
                <StepBeta />
            {:else if onboardingState.currentStep === 4}
                <StepOptional />
            {/if}
        </div>

        <footer class="wizard-footer">
            <div class="back-slot">
                {#if !isAtFirstStep()}
                    <button
                        type="button"
                        class="back-button"
                        onclick={handleBack}
                        aria-label="Go to previous step"
                        use:tooltip={'Back'}
                    >
                        <Icon name="arrow-left" size={16} />
                    </button>
                {/if}
            </div>
            <div class="primary-slot">
                {#each footerButtons as button, i (`${String(i)}-${button.label}`)}
                    <Button
                        variant={button.variant}
                        disabled={button.disabled ?? false}
                        onclick={button.onclick}
                        aria-label={button.ariaLabel ?? button.label}
                    >
                        {button.label}
                    </Button>
                {/each}
            </div>
        </footer>
    </div>
</div>

<style>
    .wizard-overlay {
        position: fixed;
        /* Start below the title bar so the scrim never covers the OS window-drag
           region: dragging the window must always work, even mid-onboarding.
           `--titlebar-height` is per-window (see app.css § Window chrome). */
        inset: var(--titlebar-height) 0 0 0;
        background: var(--sheet-backdrop-color);
        backdrop-filter: blur(var(--sheet-backdrop-blur));
        display: flex;
        align-items: center;
        justify-content: center;
        z-index: var(--z-modal);
    }

    .wizard-panel {
        width: min(var(--sheet-max-width), var(--sheet-width-fraction));
        height: min(var(--sheet-max-height), var(--sheet-height-fraction));
        background: var(--color-bg-secondary);
        border: 1px solid var(--color-border-strong);
        border-radius: var(--sheet-radius);
        box-shadow: var(--shadow-lg);
        display: flex;
        flex-direction: column;
        overflow: hidden;
    }

    .wizard-panel:focus-visible {
        outline: 2px solid var(--color-accent);
        outline-offset: -2px;
    }

    .wizard-header {
        padding: var(--spacing-lg) var(--spacing-2xl) 0;
        display: flex;
        justify-content: center;
    }

    .step-dots {
        display: flex;
        gap: var(--spacing-sm);
        list-style: none;
        margin: 0;
        padding: 0;
    }

    .step-dot {
        width: 8px;
        height: 8px;
        border-radius: var(--radius-full);
        background: var(--color-border);
        transition: background var(--transition-base), transform var(--transition-base);
    }

    .step-dot.active {
        background: var(--color-accent);
        transform: scale(1.4);
    }

    /* Optional step: outlined dot to read as "not required" without being
       hidden. When active, it still fills with the accent so users know where
       they are. */
    .step-dot.optional {
        background: transparent;
        border: 1px solid var(--color-border);
    }

    .step-dot.optional.active {
        background: var(--color-accent);
        border-color: var(--color-accent);
    }

    .wizard-body {
        flex: 1;
        min-height: 0;
        display: flex;
        flex-direction: column;
    }

    .wizard-footer {
        display: flex;
        align-items: center;
        justify-content: space-between;
        padding: var(--spacing-lg) var(--spacing-2xl);
        border-top: 1px solid var(--color-border-subtle);
        background: var(--color-bg-secondary);
    }

    .back-slot,
    .primary-slot {
        display: flex;
        align-items: center;
    }

    .primary-slot {
        gap: var(--spacing-md);
    }

    .back-button {
        display: inline-flex;
        align-items: center;
        justify-content: center;
        width: 32px;
        height: 32px;
        padding: 0;
        background: transparent;
        color: var(--color-text-secondary);
        border: 1px solid var(--color-border);
        border-radius: var(--radius-md);
        transition: all var(--transition-base);
    }

    .back-button:hover {
        background: var(--color-bg-tertiary);
        color: var(--color-text-primary);
    }

    .back-button:focus-visible {
        outline: 2px solid var(--color-accent);
        outline-offset: 1px;
        box-shadow: var(--shadow-focus-contrast);
    }
</style>
