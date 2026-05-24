<script lang="ts">
    import { onMount } from 'svelte'
    import {
        checkFullDiskAccess,
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
    import {
        getOnboardingState,
        setStep1Restart,
        setCurrentStep,
        setStepTwoBanner,
    } from './onboarding-state.svelte'

    /**
     * Step 1 — Full Disk Access.
     *
     * Three variants (driven from `onboardingState.step1Variant`, computed in
     * `+page.svelte` from persisted flags + an FDA probe):
     *
     * - `'first-ask'`: welcome + pros/cons + Allow/Deny.
     * - `'revoked'`: "Cmdr previously had FDA but it was revoked" opener; same Allow/Deny.
     * - `'already-granted'`: collapsed single-Next variant (no Allow/Deny). Menu re-entry only.
     *
     * The Allow path requires a restart before advancing past step 1 (plan § "FDA gate
     * clear-on-Allow"). After Allow, the footer's primary button flips to "Restart Cmdr"
     * (rendered by `OnboardingWizard.svelte`). The Allow/Deny buttons stay live so the
     * user can change their mind to Deny. On Linux, the wizard skips this step entirely.
     */

    const log = getAppLogger('onboarding')
    const onboardingState = getOnboardingState()

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

    async function handleAllow() {
        hasClickedOpenSettings = true
        // Re-probe right before opening Settings so the bundle is freshly registered
        // with TCC. Without this, the Cmdr row may not appear in the Full Disk Access
        // list — TCC only adds apps that have recently attempted to read a protected
        // path. The probe also happens to be how we detect a same-session grant; if it
        // returns true, the user already toggled it in another way and the restart is
        // still the safest path (the FDA gate is set at boot from the probe, so we need
        // a relaunch to clear it).
        try {
            await checkFullDiskAccess()
        } catch (error) {
            log.warn('FDA re-probe before opening Settings failed: {error}', { error })
        }
        try {
            await saveSettings({ fullDiskAccessChoice: 'allow' })
        } catch (error) {
            log.warn('Failed to persist fullDiskAccessChoice=allow: {error}', { error })
        }
        try {
            await openPrivacySettings()
        } catch (error) {
            log.warn('openPrivacySettings failed: {error}', { error })
        }
        // Pre-compute the step-2 banner so if the user changes their mind and comes back,
        // step 2 reads the right state. (We treat "Allow but not granted yet" as 'stuck'
        // — same banner the resume rule lands on for first-time-stuck users.)
        setStepTwoBanner('stuck')
        // Footer flips to "Restart Cmdr". The user must relaunch before advancing.
        setStep1Restart()
    }

    async function handleDeny() {
        try {
            await saveSettings({ fullDiskAccessChoice: 'deny' })
        } catch (error) {
            log.warn('Failed to persist fullDiskAccessChoice=deny: {error}', { error })
        }
        // Indexing was deferred at app launch (FDA gate). Now that the user has decided,
        // start it within this session so they don't need to restart for the index to
        // populate. Per-folder TCC popups will appear as the scan walks ~/Downloads,
        // ~/Documents, ~/Desktop, ... — those are the prompts the user opted into.
        try {
            await startIndexingAfterFdaDecision()
        } catch (error) {
            log.warn('Failed to start indexing after FDA deny: {error}', { error })
        }
        setStepTwoBanner('denied')
        // Advance to step 2 immediately. No restart needed on Deny — the gate clears
        // via `startIndexingAfterFdaDecision`.
        setCurrentStep(2)
    }

    /** Linux skip safety net: the wizard's resume rule lands Linux on step 2, so this */
    /** component shouldn't ever render on Linux. Guard returns nothing if it does. */
    const renderable = isMacOS()
</script>

{#if renderable}
    <OnboardingStepShell>
        {#if onboardingState.step1Variant === 'already-granted'}
            <h2 class="welcome">Cmdr currently has full disk access</h2>
            <p>You can revoke it any time in {systemStrings.systemSettings}.</p>
        {:else}
            <h2 class="welcome">Welcome to Cmdr!</h2>

            {#if onboardingState.step1Variant === 'revoked'}
                <p>It looks like you accepted full disk access before but then revoked it.</p>
                <p><strong>The app currently has no full disk access.</strong></p>
                <p>
                    If that was intentional, click <strong>Deny</strong> and the app won't bother you again.
                </p>
                <p>If it <em>wasn't</em> intentional, consider allowing full disk access again. Here are the pros and cons:</p>
            {:else}
                <p><strong>You probably just want to start using the app.</strong> Sorry to bother you with this first, but it's needed.</p>
                <p>
                    You see, Cmdr is a file manager, and it needs to access your disk to see all your files. macOS doesn't
                    automatically grant permission to this.
                </p>
                <p>Would you like to give this app full disk access? Here's what that means:</p>
            {/if}

            <ul class="bullets">
                <li>
                    <strong>Pro:</strong> The app will access your entire disk without nagging you for permissions to each folder
                    like Downloads, Documents, and Desktop.
                </li>
                <li>
                    <strong>Con:</strong> Full disk access is pretty powerful. It lets the app read any file on your Mac. Only
                    grant this if you trust Cmdr. Cmdr uses this right respectfully, and is
                    <LinkButton
                        href="https://github.com/vdavid/cmdr"
                        target="_blank"
                        rel="noopener noreferrer"
                        onclick={(event) => {
                            event.preventDefault()
                            void openExternalUrl('https://github.com/vdavid/cmdr')
                        }}
                    >
                        source-available
                    </LinkButton>
                    if you feel unsure.
                </li>
            </ul>

            <p>If you decide to allow:</p>

            <ol class="steps">
                <li>Click <strong>Open {systemStrings.systemSettings}</strong> below</li>
                <li>
                    {#if isVenturaOrNewer}
                        Find <strong>Cmdr</strong> in the list and toggle it on
                    {:else}
                        Find <strong>Cmdr</strong> at the end of the list and toggle it on
                    {/if}
                    <p class="step-tip">
                        Tip: Is Cmdr not in the list? Click the "+" button at the bottom, and choose <strong>Cmdr</strong> from your
                        <strong>Applications</strong> folder.
                    </p>
                </li>
                <li>Confirm and click <strong>Quit & Reopen</strong></li>
            </ol>

            <div class="buttons">
                <Button variant="primary" onclick={handleAllow}>Open {systemStrings.systemSettings}</Button>
                <Button variant="danger" onclick={handleDeny}>Deny</Button>
            </div>
            {#if hasClickedOpenSettings && onboardingState.step1FooterMode === 'restart'}
                <div class="post-action">
                    <p>Cmdr needs to restart so the new permission takes effect.</p>
                    <p>
                        When you're ready, click <strong>Restart Cmdr</strong> below. If you change your mind, click
                        <strong>Deny</strong> above instead.
                    </p>
                </div>
            {/if}
        {/if}
    </OnboardingStepShell>
{/if}

<style>
    .welcome {
        margin: 0 0 var(--spacing-md) 0;
        /* About 20% larger than body font, per David's copy spec. */
        font-size: var(--font-size-lg);
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
</style>
