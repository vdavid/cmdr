<script lang="ts">
    import { onMount } from 'svelte'
    import {
        checkFullDiskAccess,
        checkFullDiskAccessQuiet,
        getMacosMajorVersion,
        openExternalUrl,
        openPrivacySettings,
        startIndexingAfterFdaDecision,
    } from '$lib/tauri-commands'
    import { saveSettings } from '$lib/settings-store'
    import Button from '$lib/ui/Button.svelte'
    import LinkButton from '$lib/ui/LinkButton.svelte'
    import OnboardingStepShell from './OnboardingStepShell.svelte'
    import { getAppLogger } from '$lib/logging/logger'
    import { systemStrings } from '$lib/system-strings.svelte'
    import { isMacOS } from '$lib/shortcuts/key-capture'
    import { tString } from '$lib/intl/messages.svelte'
    import Trans from '$lib/intl/Trans.svelte'
    import type { Snippet } from 'svelte'
    import {
        getOnboardingState,
        setStep1Restart,
        setStep1Granted,
        setCurrentStep,
        setStepTwoBanner,
    } from './onboarding-state.svelte'

    /**
     * Step 1: Full Disk Access.
     *
     * Three variants (driven from `onboardingState.step1Variant`, computed in
     * `+page.svelte` from persisted flags + an FDA probe):
     *
     * - `'first-ask'`: welcome + pros/cons + Allow/Deny.
     * - `'revoked'`: "Cmdr previously had FDA but it was revoked" opener; same Allow/Deny.
     * - `'already-granted'`: collapsed single-Next variant (no Allow/Deny). Menu re-entry only.
     *
     * While the step is open and FDA isn't granted yet, a 500 ms poller watches the OS
     * status (`checkFullDiskAccessQuiet`, the side-effect-free probe). The moment the user
     * toggles Cmdr on in System Settings, the body switches to a success state and the
     * footer flips to "Restart Cmdr". The restart stays required: the FDA gate is set once
     * at boot, so the permission only takes effect on relaunch.
     *
     * The Allow path requires a restart before advancing past step 1 (plan § "FDA gate
     * clear-on-Allow"). After Allow, the footer's primary button flips to "Restart Cmdr"
     * (rendered by `OnboardingWizard.svelte`). The Allow/Deny buttons stay live so the
     * user can change their mind to Deny. On Linux, the wizard skips this step entirely.
     */

    const log = getAppLogger('onboarding')
    const onboardingState = getOnboardingState()

    /** How often the poller re-checks FDA status while the step is open and not granted. */
    const FDA_POLL_INTERVAL_MS = 500

    /** Has the user clicked "Open System Settings" this session? Drives the post-action hint. */
    let hasClickedOpenSettings = $state(false)
    /**
     * Default to Ventura+ (alphabetical list) until the backend reports the real version.
     * macOS 12 and older append new entries at the end of the FDA list, so the
     * "find Cmdr in the list" instruction reads slightly differently.
     */
    let isVenturaOrNewer = $state(true)

    onMount(async () => {
        try {
            const major = await getMacosMajorVersion()
            if (major > 0) {
                isVenturaOrNewer = major >= 13
            }
        } catch (error) {
            log.warn('Failed to read macOS major version: {error}', { error })
        }
    })

    /**
     * Live grant detection. While step 1 is on the Allow/Deny variants and FDA isn't
     * granted yet, poll the OS every 500 ms. The moment the probe flips to granted, mark
     * it (`setStep1Granted()` switches the body to the success state + footer to "Restart
     * Cmdr") and stop polling. The `already-granted` variant never polls (FDA is already
     * on). The interval is cleared on unmount and on grant, so no leaks.
     *
     * `checkFullDiskAccessQuiet` is the side-effect-free probe: it doesn't fire the
     * `mmap` / `NSData` / `read_dir` registration storm that `checkFullDiskAccess` does on
     * a denial, so polling it twice a second stays cheap and quiet.
     */
    $effect(() => {
        if (!isMacOS()) return
        if (onboardingState.step1Variant === 'already-granted') return
        if (onboardingState.step1Granted) return

        // `isStopped()` reads the flag through a function so TS doesn't narrow the
        // post-`await` re-check to always-true (it can't model the cleanup closure's
        // write). `inFlight` prevents overlapping probes if one runs past the 500 ms tick.
        let stopped = false
        let inFlight = false
        const isStopped = (): boolean => stopped

        async function poll(): Promise<void> {
            if (isStopped() || inFlight) return
            inFlight = true
            try {
                const granted = await checkFullDiskAccessQuiet()
                // Re-check: the effect may have torn down during the await (e.g. the user
                // clicked Deny and the step unmounted). Don't report a grant after that.
                if (granted && !isStopped()) setStep1Granted()
            } catch (error) {
                log.warn('FDA grant-detection poll failed: {error}', { error })
            } finally {
                inFlight = false
            }
        }

        const intervalId = setInterval(() => void poll(), FDA_POLL_INTERVAL_MS)
        void poll() // Check once immediately so a grant already in place is caught fast.

        return () => {
            stopped = true
            clearInterval(intervalId)
        }
    })

    async function handleAllow() {
        hasClickedOpenSettings = true
        // Re-probe right before opening Settings so the bundle is freshly registered
        // with TCC. Without this, the Cmdr row may not appear in the Full Disk Access
        // list (TCC only adds apps that have recently attempted to read a protected
        // path). The probe also happens to be how we detect a same-session grant; if it
        // returns true, the user already toggled it in another way and the restart is
        // still the safest path (the FDA gate is set at boot from the probe, so we need
        // a relaunch to clear it).
        try {
            await checkFullDiskAccess()
        } catch (error) {
            log.warn('FDA re-probe before opening Settings failed: {error}', { error })
        }
        if (!(await saveSettings({ fullDiskAccessChoice: 'allow' }))) {
            log.warn('Could not persist fullDiskAccessChoice=allow; the choice may not survive a restart')
        }
        try {
            await openPrivacySettings()
        } catch (error) {
            log.warn('openPrivacySettings failed: {error}', { error })
        }
        // Pre-compute the step-2 banner so if the user changes their mind and comes back,
        // step 2 reads the right state. (We treat "Allow but not granted yet" as 'stuck',
        // the same banner the resume rule lands on for first-time-stuck users.)
        setStepTwoBanner('stuck')
        // Footer flips to "Restart Cmdr". The user must relaunch before advancing.
        setStep1Restart()
    }

    async function handleDeny() {
        if (!(await saveSettings({ fullDiskAccessChoice: 'deny' }))) {
            log.warn('Could not persist fullDiskAccessChoice=deny; the choice may not survive a restart')
        }
        // Indexing was deferred at app launch (FDA gate). Now that the user has decided,
        // start it within this session so they don't need to restart for the index to
        // populate. Per-folder TCC popups will appear as the scan walks ~/Downloads,
        // ~/Documents, ~/Desktop, and the like (those are the prompts the user opted into).
        try {
            await startIndexingAfterFdaDecision()
        } catch (error) {
            log.warn('Failed to start indexing after FDA deny: {error}', { error })
        }
        setStepTwoBanner('denied')
        // Advance to step 2 immediately. No restart needed on Deny: the gate clears
        // via `startIndexingAfterFdaDecision`.
        setCurrentStep(2)
    }

    /** Linux skip safety net: the wizard's resume rule lands Linux on step 2, so this */
    /** component shouldn't ever render on Linux. Guard returns nothing if it does. */
    const renderable = isMacOS()
</script>

{#snippet strong(children: Snippet)}<strong>{@render children()}</strong>{/snippet}
{#snippet em(children: Snippet)}<em>{@render children()}</em>{/snippet}
{#snippet deny(children: Snippet)}<strong>{@render children()}</strong>{/snippet}
{#snippet restart(children: Snippet)}<strong>{@render children()}</strong>{/snippet}
{#snippet sourceLink(children: Snippet)}<LinkButton
        href="https://github.com/vdavid/cmdr"
        target="_blank"
        rel="noopener noreferrer"
        onclick={(event: MouseEvent) => {
            event.preventDefault()
            void openExternalUrl('https://github.com/vdavid/cmdr')
        }}>{@render children()}</LinkButton
    >{/snippet}

{#if renderable}
    <OnboardingStepShell>
        {#if onboardingState.step1Granted}
            <h2 class="welcome">{tString('onboarding.stepFda.granted.title')}</h2>
            <p>{tString('onboarding.stepFda.granted.body')}</p>
            <p class="success-hint">{tString('onboarding.stepFda.granted.hint')}</p>
        {:else if onboardingState.step1Variant === 'already-granted'}
            <h2 class="welcome">{tString('onboarding.stepFda.alreadyGranted.title')}</h2>
            <p>{tString('onboarding.stepFda.alreadyGranted.body', { systemSettings: systemStrings.systemSettings })}</p>
        {:else}
            <h2 class="welcome">{tString('onboarding.stepFda.welcome.title')}</h2>

            {#if onboardingState.step1Variant === 'revoked'}
                <p>{tString('onboarding.stepFda.revoked.intro')}</p>
                <p><strong>{tString('onboarding.stepFda.revoked.noAccess')}</strong></p>
                <p><Trans key="onboarding.stepFda.revoked.ifIntentional" snippets={{ deny }} /></p>
                <p><Trans key="onboarding.stepFda.revoked.ifNot" snippets={{ em }} /></p>
            {:else}
                <p><Trans key="onboarding.stepFda.firstAsk.lede" snippets={{ strong }} /></p>
                <p>{tString('onboarding.stepFda.firstAsk.explain')}</p>
                <p>{tString('onboarding.stepFda.firstAsk.askPermission')}</p>
            {/if}

            <ul class="bullets">
                <li><Trans key="onboarding.stepFda.pro" snippets={{ strong }} /></li>
                <li><Trans key="onboarding.stepFda.con" snippets={{ strong, sourceLink }} /></li>
            </ul>

            <p>{tString('onboarding.stepFda.ifAllow')}</p>

            <ol class="steps">
                <li>
                    <Trans
                        key="onboarding.stepFda.step1"
                        snippets={{ strong }}
                        params={{ systemSettings: systemStrings.systemSettings }}
                    />
                </li>
                <li>
                    {#if isVenturaOrNewer}
                        <Trans key="onboarding.stepFda.step2.ventura" snippets={{ strong }} />
                    {:else}
                        <Trans key="onboarding.stepFda.step2.older" snippets={{ strong }} />
                    {/if}
                    <p class="step-tip"><Trans key="onboarding.stepFda.step2.tip" snippets={{ strong }} /></p>
                </li>
                <li><Trans key="onboarding.stepFda.step3" snippets={{ strong }} /></li>
            </ol>

            <div class="buttons">
                <Button variant="primary" onclick={handleAllow}
                    >{tString('onboarding.stepFda.openSettings', {
                        systemSettings: systemStrings.systemSettings,
                    })}</Button
                >
                <Button variant="danger" onclick={handleDeny}>{tString('onboarding.stepFda.deny')}</Button>
            </div>
            {#if hasClickedOpenSettings && onboardingState.step1FooterMode === 'restart'}
                <div class="post-action">
                    <p>{tString('onboarding.stepFda.postAction.intro')}</p>
                    <p><Trans key="onboarding.stepFda.postAction.body" snippets={{ restart, deny }} /></p>
                </div>
            {/if}
        {/if}
    </OnboardingStepShell>
{/if}

<style>
    .welcome {
        margin: 0 0 var(--spacing-md) 0;
        /* 20% larger than body font, per David's copy spec. `--font-size-lg`
           (16px) over body 14px is only ~14%; `--font-size-xl` (20px) reads as
           a hero. The `calc()` matches the other step titles (StepAi, StepOptional)
           so all three onboarding steps share one heading size. */
        font-size: calc(var(--font-size-md) * 1.2);
        font-weight: 600;
        color: var(--color-text-primary);
    }

    p {
        margin: 0 0 var(--spacing-md) 0;
    }

    p:last-child {
        margin-bottom: 0;
    }

    .bullets,
    .steps {
        margin: 0 0 var(--spacing-lg) 0;
        padding-left: var(--spacing-xl);
    }

    .bullets li,
    .steps li {
        margin-bottom: var(--spacing-sm);
    }

    .bullets li:last-child,
    .steps li:last-child {
        margin-bottom: 0;
    }

    .step-tip {
        margin: var(--spacing-xs) 0 0 0;
        color: var(--color-text-secondary);
    }

    .buttons {
        display: flex;
        gap: var(--spacing-md);
        justify-content: center;
        margin-top: var(--spacing-lg);
    }

    .post-action {
        margin-top: var(--spacing-lg);
        padding-top: var(--spacing-lg);
        border-top: 1px solid var(--color-border-subtle);
        color: var(--color-text-secondary);
    }

    .success-hint {
        color: var(--color-text-secondary);
    }
</style>
