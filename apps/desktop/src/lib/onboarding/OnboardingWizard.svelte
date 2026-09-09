<script lang="ts">
    import { onMount, onDestroy, tick, untrack } from 'svelte'
    import { relaunch } from '@tauri-apps/plugin-process'
    import Icon from '$lib/ui/Icon.svelte'
    import { notifyDialogOpened, notifyDialogClosed } from '$lib/tauri-commands'
    import Button from '$lib/ui/Button.svelte'
    import { trapFocus } from '$lib/ui/focus-trap'
    import { tooltip, showTooltipNow, hideTooltipFor } from '$lib/tooltip/tooltip'
    import { getAppLogger } from '$lib/logging/logger'
    import { tString } from '$lib/intl/messages.svelte'
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
    import OnboardingLanguagePicker from './OnboardingLanguagePicker.svelte'

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
     * The overlay, handed to the header's language picker as its portal target: the
     * open menu has to escape `.wizard-panel`'s `overflow: hidden` without leaving the
     * focus trap (which lives on this element) or the modal stacking context.
     */
    let overlayEl: HTMLDivElement | undefined = $state()
    /**
     * Element that had focus when the wizard opened. Restored on destroy so
     * keyboard input flows back to wherever it came from after close.
     */
    let previousActiveElement: HTMLElement | null = null
    /** The footer's forward-button group, so a note can be shown ON the button it answers. */
    let primarySlotEl: HTMLDivElement | undefined = $state()

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
        /** Set by a step that wants the button blocked-but-clickable; see `WizardFooterButton`. */
        blockedReason?: string
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
                blockedReason: b.blockedReason,
                ariaLabel: b.ariaLabel,
            }))
        }
        const step = onboardingState.currentStep
        if (step === null) return []
        if (step === 1) {
            if (onboardingState.step1FooterMode === 'restart') {
                return [
                    { label: tString('onboarding.wizard.restart'), onclick: () => void handleRestart(), variant: 'primary' },
                ]
            }
            if (onboardingState.step1Variant === 'already-granted') {
                return [{ label: tString('onboarding.wizard.next'), onclick: handleNext, variant: 'primary' }]
            }
            // Step 1 decide mode: Allow + Deny live in the body; footer primary is hidden.
            return []
        }
        if (isAtLastStep()) {
            return [{ label: tString('onboarding.wizard.finish'), onclick: handleNext, variant: 'primary' }]
        }
        return [{ label: tString('onboarding.wizard.next'), onclick: handleNext, variant: 'primary' }]
    }

    /**
     * A step's answer to a press lands as a tooltip ON the button that was pressed, not
     * as text painted beside it: the footer is a tight row, and a sentence long enough to
     * be useful pushed the buttons around. The tooltip has to be forced open, because the
     * pointer is already over the button (no `mouseenter` is coming) and a keyboard press
     * has just set the hover-suppress flag. It stays as the button's `tooltipContent` too,
     * so hovering back re-shows it while the note stands.
     */
    $effect(() => {
        const note = onboardingState.footerNote
        const buttonEl = primarySlotEl?.querySelector<HTMLElement>('button:last-of-type')
        if (!buttonEl) return
        if (note === null) {
            hideTooltipFor(buttonEl)
            return
        }
        showTooltipNow(buttonEl, { contentEl: note })
    })

    /**
     * Step-dot indicator. The last step (Optional) is rendered with a muted/open style
     * so users see "mandatory steps plus one optional," not an endless wizard. The Beta
     * page (step 3) is a normal mandatory dot.
     */
    const stepDots = Array.from({ length: ONBOARDING_STEP_COUNT }, (_, i) => ({
        index: (i + 1) as OnboardingStep,
        isOptional: i === ONBOARDING_STEP_COUNT - 1,
    }))

    /**
     * What the dots say when you point at them. The count reads "3+1" rather than "4",
     * because the last step is optional and a plain "of 4" would promise one more
     * required page than there is.
     */
    const stepTooltip = $derived.by(() => {
        const step = onboardingState.currentStep
        if (step === null) return undefined
        return tString('onboarding.wizard.stepTooltip', {
            step,
            mandatory: ONBOARDING_STEP_COUNT - 1,
            note: step === ONBOARDING_STEP_COUNT ? 'last' : step === ONBOARDING_STEP_COUNT - 1 ? 'remaining' : 'none',
        })
    })
</script>

<!-- No `onEscape` on the trap: the wizard must swallow Escape (see `handleKeydown`). -->
<div
    bind:this={overlayEl}
    class="wizard-overlay"
    role="dialog"
    aria-modal="true"
    aria-labelledby="onboarding-wizard-title"
    use:trapFocus
>
    <!-- Panel takes focus on mount so Esc routing lands here; trapFocus keeps Tab inside. -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
        bind:this={panelEl}
        class="wizard-panel"
        data-dialog-id="onboarding"
        tabindex="-1"
        onkeydown={handleKeydown}
    >
        <h2 id="onboarding-wizard-title" class="sr-only">{tString('onboarding.wizard.title')}</h2>

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

        <!-- Everything that frames the flow lives down here, so the step body owns the
             whole top of the panel: Back and the language escape hatch on the left, the
             step dots centred, the forward buttons on the right. -->
        <footer class="wizard-footer">
            <div class="footer-start">
                {#if !isAtFirstStep()}
                    <button
                        type="button"
                        class="back-button"
                        onclick={handleBack}
                        aria-label={tString('onboarding.wizard.backAria')}
                        use:tooltip={tString('onboarding.wizard.back')}
                    >
                        <Icon name="arrow-left" size={16} />
                    </button>
                {/if}
                <!-- Rendered only once the overlay ref is bound, so the portal target is stable
                     from the picker's first render. Without the guard the menu mounts into
                     `document.body` for one pass and Ark's Portal then re-mounts it. See
                     `OnboardingLanguagePicker.svelte` for why it isn't a step. -->
                {#if overlayEl}
                    <OnboardingLanguagePicker portalContainer={overlayEl} />
                {/if}
            </div>
            <ol
                class="step-dots"
                aria-label={tString('onboarding.wizard.progressLabel')}
                use:tooltip={stepTooltip}
            >
                {#each stepDots as dot (dot.index)}
                    <li
                        class="step-dot"
                        class:active={onboardingState.currentStep === dot.index}
                        class:optional={dot.isOptional}
                        aria-current={onboardingState.currentStep === dot.index ? 'step' : undefined}
                    >
                        <span class="sr-only">
                            {tString('onboarding.wizard.stepProgress', {
                                step: dot.index,
                                total: ONBOARDING_STEP_COUNT,
                                isOptional: dot.isOptional,
                            })}
                        </span>
                    </li>
                {/each}
            </ol>
            <div class="primary-slot" bind:this={primarySlotEl}>
                {#each footerButtons as button, i (`${String(i)}-${button.label}`)}
                    <!-- A blocked button keeps its click and its place in the tab order on
                         purpose: pressing it is how the user finds out what's missing. -->
                    <Button
                        variant={button.variant}
                        disabled={button.disabled ?? false}
                        ariaDisabled={button.blockedReason !== undefined}
                        tooltipContent={button.blockedReason ??
                            (i === footerButtons.length - 1 && onboardingState.footerNote !== null
                                ? { contentEl: onboardingState.footerNote }
                                : undefined)}
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

    /* Drop the backdrop blur when the OS asks for reduced transparency; the
       dimming background still does its job and the panel is already opaque. */
    :global(html.reduce-transparency) .wizard-overlay {
        backdrop-filter: none;
        -webkit-backdrop-filter: none;
    }

    /* Chrome identical to `ModalDialog`'s panel: same surface, the same macOS pair of
       hairlines (a darker one outside, a lighter one just inside, both alpha so they
       work over whatever shows through), and the same three-layer shadow. A sheet is
       the largest thing the app floats over the canvas, so anything cheaper here reads
       as a different kind of window than the rest of the app's dialogs. */
    .wizard-panel {
        width: min(var(--sheet-max-width), var(--sheet-width-fraction));
        height: min(var(--sheet-max-height), var(--sheet-height-fraction));
        background: var(--color-bg-dialog);
        border: 1px solid var(--color-dialog-border-outer);
        border-radius: var(--sheet-radius);
        box-shadow:
            inset 0 0 0 1px var(--color-dialog-border-inner),
            var(--shadow-dialog);
        display: flex;
        flex-direction: column;
        overflow: hidden;
    }

    .wizard-panel:focus-visible {
        outline: 2px solid var(--color-accent);
        outline-offset: -2px;
    }

    .step-dots {
        display: flex;
        gap: var(--spacing-sm);
        list-style: none;
        margin: 0;
        /* Vertical padding widens the hover target: the dots are 8px tall, and a
           tooltip you have to hit within 8px is a tooltip nobody reads. */
        padding: var(--spacing-sm) 0;
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

    /* Three columns, so the dots sit on the panel's centre line rather than the middle of
       whatever space the two side groups leave. `1fr auto 1fr` also degrades the right
       way: a footer whose buttons outgrow their column pushes the dots off-centre instead
       of letting the two overlap. */
    .wizard-footer {
        display: grid;
        grid-template-columns: 1fr auto 1fr;
        align-items: center;
        gap: var(--spacing-md);
        padding: var(--spacing-lg) var(--spacing-2xl);
        border-top: 1px solid var(--color-border-subtle);
    }

    .footer-start,
    .primary-slot {
        display: flex;
        align-items: center;
        min-width: 0;
    }

    .footer-start {
        gap: var(--spacing-sm);
    }

    .primary-slot {
        gap: var(--spacing-md);
        justify-content: flex-end;
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
